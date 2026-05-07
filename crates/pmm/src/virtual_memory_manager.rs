//! Virtual memory manager.
//!
//! [`VirtualMemoryManager`] is the primary memory manager for the kernel. It wraps the
//! [`PhysicalMemoryManager`] in an `Arc<Mutex<...>>` and owns the kernel [`AddressSpace`],
//! and serves as the factory for user address spaces. All physical frame allocation and
//! deallocation flows through the shared PMM reference.

extern crate alloc;

use alloc::sync::Arc;

use spin::Mutex;

use crate::{
    address_space::AddressSpace,
    page_directory::PageDirectory,
    physical_memory_manager::PhysicalMemoryManager,
};

/// The top-level memory manager.
///
/// Owns a shared reference to the physical memory allocator and the kernel's virtual
/// address space. Use [`init`](Self::init) once during kernel startup, then obtain user
/// address spaces via [`create_user_space`](Self::create_user_space).
pub struct VirtualMemoryManager {
    pmm: Arc<Mutex<PhysicalMemoryManager>>,
    kernel_space: AddressSpace,
}

impl VirtualMemoryManager {
    /// Initialises the VMM, taking ownership of `pmm`.
    ///
    /// The PMM is wrapped in `Arc<Mutex<...>>` so it can be shared with address spaces.
    ///
    /// On x86_64 (non-test) the kernel address space wraps the page tables that the
    /// bootloader loaded into CR3, so existing Limine mappings remain valid.
    ///
    /// In test / software-emulation mode a fresh, empty page directory is used instead.
    ///
    /// # Safety (x86_64)
    /// Must be called after `AddressTranslator::set_current()`. The active page tables
    /// must be valid and accessible via the HHDM.
    pub fn init(pmm: PhysicalMemoryManager) -> Self {
        let pmm = Arc::new(Mutex::new(pmm));

        #[cfg(all(target_arch = "x86_64", not(test), not(feature = "software-emulation")))]
        let page_dir = {
            // SAFETY: caller guarantees the translator is set and the active tables are valid.
            unsafe { PageDirectory::from_active_tables() }
        };

        #[cfg(any(test, feature = "software-emulation"))]
        let page_dir = PageDirectory::new();

        Self {
            kernel_space: AddressSpace::new(page_dir, Arc::clone(&pmm)),
            pmm,
        }
    }

    // ── Physical memory manager access ───────────────────────────────────────────

    /// Returns a shared reference to the Arc-wrapped physical memory manager.
    ///
    /// Clone the `Arc` to share PMM access with other components.
    pub fn pmm(&self) -> &Arc<Mutex<PhysicalMemoryManager>> {
        &self.pmm
    }

    // ── Kernel address space access ──────────────────────────────────────────────

    /// Returns a shared reference to the kernel address space.
    pub fn kernel_space(&self) -> &AddressSpace {
        &self.kernel_space
    }

    /// Returns a mutable reference to the kernel address space.
    pub fn kernel_space_mut(&mut self) -> &mut AddressSpace {
        &mut self.kernel_space
    }

    // ── User address space creation ──────────────────────────────────────────────

    /// Creates a new user address space.
    ///
    /// The kernel half of the root page table (entries
    /// [`KERNEL_ENTRY_START`](crate::arch::KERNEL_ENTRY_START)..len) is copied from the
    /// kernel space so that kernel mappings are visible when running in ring 0 on behalf
    /// of this process. The user half starts empty.
    ///
    /// The new address space shares the PMM Arc with this VMM and all other spaces.
    pub fn create_user_space(&self) -> AddressSpace {
        let mut user_dir = PageDirectory::new();
        user_dir.copy_kernel_half_from(self.kernel_space.page_directory());
        AddressSpace::new(user_dir, Arc::clone(&self.pmm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AddressTranslator, BootMemoryRegion, PhysicalAddress, VirtualAddress,
        address_space::VmaKind,
        arch::{self, PageFlags},
        memmap::MemoryMap,
    };

    fn setup() {
        let _ = std::panic::catch_unwind(|| {
            AddressTranslator::set_current(AddressTranslator::emulated(128 * 1024));
        });
    }

    struct Region {
        base: PhysicalAddress,
        size: usize,
    }
    impl BootMemoryRegion for Region {
        fn base(&self) -> PhysicalAddress {
            self.base
        }
        fn size(&self) -> usize {
            self.size
        }
        fn is_usable(&self) -> bool {
            true
        }
    }

    // NOTE: In tests, alloc_page_table() uses a bump allocator starting at phys 0.
    // PMM frames start at BASE_FRAME * PAGE_SIZE to avoid overlapping that region.
    const BASE_FRAME: usize = 256;

    fn make_pmm(num_frames: usize) -> PhysicalMemoryManager {
        let base = BASE_FRAME * arch::PAGE_SIZE;
        let boot = [Region {
            base: PhysicalAddress::new(base),
            size: num_frames * arch::PAGE_SIZE,
        }];
        let mut pmm = PhysicalMemoryManager::new(MemoryMap::from_boot_map(&boot));
        for i in 0..num_frames {
            pmm.deallocate(PhysicalAddress::new(base + i * arch::PAGE_SIZE), 0);
        }
        pmm
    }

    #[test]
    fn init_succeeds() {
        setup();
        let _vmm = VirtualMemoryManager::init(make_pmm(64));
    }

    #[test]
    fn kernel_space_alloc_and_free() {
        setup();
        let mut vmm = VirtualMemoryManager::init(make_pmm(64));
        let virt = VirtualAddress::new(arch::PAGE_SIZE);
        let mut flags = PageFlags::empty();
        flags.set_present(true);

        let before = vmm.pmm().lock().free_frames();
        vmm.kernel_space_mut()
            .alloc_area(virt, arch::PAGE_SIZE, flags, VmaKind::Anonymous)
            .expect("alloc_area failed");
        assert!(vmm.pmm().lock().free_frames() < before);

        let after_alloc = vmm.pmm().lock().free_frames();
        vmm.kernel_space_mut().free_area(virt, arch::PAGE_SIZE);
        assert_eq!(vmm.pmm().lock().free_frames(), after_alloc + 1);
    }

    #[test]
    fn free_kernel_area_returns_frames() {
        setup();
        let num_frames = 256;
        let mut vmm = VirtualMemoryManager::init(make_pmm(num_frames));
        let page = arch::PAGE_SIZE;
        let virt = VirtualAddress::new(page);
        let mut flags = PageFlags::empty();
        flags.set_present(true);

        vmm.kernel_space_mut()
            .alloc_area(virt, page, flags, VmaKind::Anonymous)
            .unwrap();

        let before = vmm.pmm().lock().free_frames();
        vmm.kernel_space_mut().free_area(virt, page);
        assert_eq!(vmm.pmm().lock().free_frames(), before + 1);
    }

    #[test]
    fn user_space_has_kernel_half() {
        setup();
        let mut vmm = VirtualMemoryManager::init(make_pmm(256));

        // Map a page in the kernel half.
        // Software arch: KERNEL_ENTRY_START=8, root index = bits 12-15.
        // 0xFFFF_FFFF_FFFF_8000 → root index 8, page-aligned.
        let kvirt = VirtualAddress::new(0xFFFF_FFFF_FFFF_8000);
        let mut flags = PageFlags::empty();
        flags.set_present(true);
        vmm.kernel_space_mut()
            .alloc_area(kvirt, arch::PAGE_SIZE, flags, VmaKind::Anonymous)
            .unwrap();

        let mut user = vmm.create_user_space();
        // The kernel-half root entry was copied, so the full chain is reachable.
        // Free the area in the user space — frames should be returned.
        let before = vmm.pmm().lock().free_frames();
        user.free_area(kvirt, arch::PAGE_SIZE);
        assert_eq!(vmm.pmm().lock().free_frames(), before + 1);
    }

    #[test]
    fn user_space_does_not_inherit_lower_half() {
        setup();
        let mut vmm = VirtualMemoryManager::init(make_pmm(256));

        // Map in the lower (user) half of the kernel space.
        let uvirt = VirtualAddress::new(arch::PAGE_SIZE);
        let mut flags = PageFlags::empty();
        flags.set_present(true);
        vmm.kernel_space_mut()
            .alloc_area(uvirt, arch::PAGE_SIZE, flags, VmaKind::Anonymous)
            .unwrap();

        let user = vmm.create_user_space();
        // Lower half must NOT be inherited — user space has no VMAs.
        assert_eq!(user.areas().len(), 0);
    }
}
