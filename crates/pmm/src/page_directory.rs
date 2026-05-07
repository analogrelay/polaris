//! Architecture-independent page table management.
//!
//! This module provides the `PageDirectory` type, which wraps the architecture-specific
//! `PageTable` and provides high-level operations for mapping and unmapping virtual addresses.

use crate::{
    PhysicalAddress, VirtualAddress,
    address::AddressTranslator,
    arch::{self, PageEntry, PageFlags, PageTable},
};

#[cfg(not(any(test, feature = "software-emulation")))]
use alloc::boxed::Box;

/// Allocates a new page table.
///
/// In test/software-emulation mode, this allocates from the emulated memory space.
/// In production mode, this uses the standard heap allocator.
#[cfg(any(test, feature = "software-emulation"))]
fn alloc_page_table() -> *mut PageTable {
    let translator = AddressTranslator::current();
    let size = core::mem::size_of::<PageTable>();
    // Page tables must be page-aligned
    let align = arch::PAGE_SIZE as usize;

    // Allocate from emulated memory
    let phys = translator
        .allocate(size, align)
        .expect("out of emulated memory");

    // Translate to virtual address
    let virt = translator.phys_to_virt(phys);

    // Initialize the page table in place
    unsafe {
        let ptr = virt as *mut PageTable;
        ptr.write(PageTable::new());
        ptr
    }
}

/// Allocates a new page table using the standard heap allocator.
#[cfg(not(any(test, feature = "software-emulation")))]
fn alloc_page_table() -> *mut PageTable {
    Box::into_raw(Box::new(PageTable::new()))
}

/// An architecture-independent page table manager.
///
/// This type manages a root page table and provides operations for mapping and unmapping
/// virtual addresses to physical addresses. It handles walking the page table hierarchy
/// and allocating intermediate tables as needed.
///
/// The root page table may be owned (heap-allocated, freed on drop) or borrowed
/// (pointing to existing page tables, e.g. those set up by the bootloader).
pub struct PageDirectory {
    /// Raw pointer to the root page table.
    ///
    /// When `owns_root` is true, this was allocated via `alloc_page_table()` and must
    /// be freed on drop. When false, this points to existing page tables (e.g. Limine's
    /// boot-time PML4) and must NOT be freed.
    root: *mut PageTable,
    /// Whether this directory owns the root page table allocation.
    owns_root: bool,
}

// SAFETY: PageDirectory is used exclusively in single-threaded kernel init code.
unsafe impl Send for PageDirectory {}
unsafe impl Sync for PageDirectory {}

impl Drop for PageDirectory {
    fn drop(&mut self) {
        // In test/software-emulation mode, emulated memory has no individual free operation.
        #[cfg(not(any(test, feature = "software-emulation")))]
        if self.owns_root {
            // SAFETY: alloc_page_table() used Box::into_raw; reclaim with Box::from_raw.
            unsafe { drop(Box::from_raw(self.root)) };
        }
    }
}

impl PageDirectory {
    /// Creates a new page directory with an empty root page table.
    pub fn new() -> Self {
        Self {
            root: alloc_page_table(),
            owns_root: true,
        }
    }

    /// Creates a `PageDirectory` wrapping the currently-active page tables.
    ///
    /// The root page table is read from CR3 and referenced non-owingly — it will not be
    /// freed when this `PageDirectory` is dropped. Use this to extend the mappings
    /// that were set up by the bootloader (e.g. to add MMIO regions absent from the HHDM).
    ///
    /// # Safety
    /// Must be called after `AddressTranslator::set_current()` (i.e. after
    /// `mem::init_allocator()`). The active page tables must be valid and properly
    /// mapped in the HHDM.
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn from_active_tables() -> Self {
        use x86_64::registers::control::Cr3;
        let (frame, _) = Cr3::read();
        let phys = frame.start_address().as_u64() as usize;
        let virt = AddressTranslator::current().phys_to_virt(phys);
        Self {
            // SAFETY: PageTable is repr(transparent) over x86_64::structures::paging::PageTable,
            // so a *mut PageTable pointing at an HHDM-mapped physical PML4 is valid.
            root: virt as *mut PageTable,
            owns_root: false,
        }
    }

    /// Returns a shared reference to the root page table.
    fn root_table(&self) -> &PageTable {
        // SAFETY: root is always a valid, initialized PageTable pointer for Self's lifetime.
        unsafe { &*self.root }
    }

    /// Returns a mutable reference to the root page table.
    fn root_table_mut(&mut self) -> &mut PageTable {
        // SAFETY: root is always a valid, initialized PageTable pointer for Self's lifetime.
        unsafe { &mut *self.root }
    }

    /// Copies root-table entries [`crate::arch::KERNEL_ENTRY_START`..len] from `source`
    /// into this directory. Used to snapshot the kernel half into a new user address space.
    pub fn copy_kernel_half_from(&mut self, source: &PageDirectory) {
        let start = crate::arch::KERNEL_ENTRY_START;
        let len = self.root_table().len();
        for i in start..len {
            *self.root_table_mut().entry_mut(i) = source.root_table().entry(i);
        }
    }

    /// Activates this page directory by loading its root table into hardware (CR3 on x86_64).
    ///
    /// # Safety
    /// The caller must ensure the address space correctly maps all memory that will be
    /// accessed after this point, including the kernel text and stack.
    pub unsafe fn activate(&self) {
        // SAFETY: propagated from caller.
        unsafe { self.root_table().activate() }
    }

    /// Maps a virtual address to a physical address with the given flags.
    ///
    /// This function walks the page table hierarchy, allocating intermediate tables
    /// as needed, and sets the final page table entry to map the virtual address
    /// to the physical address.
    ///
    /// # Panics
    /// Panics if the virtual address is not page-aligned or if the physical address
    /// is not page-aligned.
    pub fn map(&mut self, virt: VirtualAddress, phys: PhysicalAddress, flags: PageFlags) {
        assert!(
            virt.is_aligned(arch::PAGE_SIZE),
            "virtual address must be page-aligned"
        );
        assert!(
            phys.is_aligned(arch::PAGE_SIZE),
            "physical address must be page-aligned"
        );

        let user = flags.is_user();
        let entry = self.walk_or_create(virt, user);
        let mut new_flags = flags;
        new_flags.set_present(true);
        *entry = PageEntry::new(phys, new_flags);
    }

    /// Unmaps a virtual address.
    ///
    /// This function walks the page table hierarchy and clears the entry for the
    /// given virtual address. Returns the physical address that was mapped, or
    /// None if the address was not mapped.
    ///
    /// # Panics
    /// Panics if the virtual address is not page-aligned.
    pub fn unmap(&mut self, virt: VirtualAddress) -> Option<PhysicalAddress> {
        assert!(
            virt.is_aligned(arch::PAGE_SIZE),
            "virtual address must be page-aligned"
        );

        let entry = self.walk(virt)?;
        let phys = entry.address()?;
        entry.clear();

        Some(phys)
    }

    /// Walks the page table hierarchy to find the entry for a virtual address.
    ///
    /// Returns None if any intermediate table is not present.
    fn walk(&mut self, virt: VirtualAddress) -> Option<&mut PageEntry> {
        // SAFETY: root is a valid PageTable pointer (either owned or borrowed from Limine).
        let mut table = unsafe { &mut *self.root };
        let virt_addr = virt.as_usize();

        // Walk through all levels except the last
        for level in (1..arch::PAGE_TABLE_LEVELS).rev() {
            let index = arch::page_index(virt_addr, level);
            let entry = table.entry_mut(index);

            if !entry.is_present() {
                // Intermediate table doesn't exist
                return None;
            }

            let next_table_phys = entry.address()?;
            let translator = AddressTranslator::current();
            let next_table_virt_raw = translator.phys_to_virt(next_table_phys.as_usize());

            // SAFETY: The entry contains a valid physical address of a page table.
            // PageTable is repr(transparent) over the 512-entry array, so casting the
            // HHDM virtual address to *mut PageTable is correct for both Limine-allocated
            // and kernel-allocated sub-tables.
            table = unsafe { &mut *(next_table_virt_raw as *mut PageTable) };
        }

        let index = arch::page_index(virt_addr, 0);
        Some(table.entry_mut(index))
    }

    /// Walks the page table hierarchy, creating intermediate tables as needed.
    ///
    /// `user` controls whether newly-created intermediate tables are marked
    /// user-accessible. This must be `true` when mapping user-mode pages, otherwise
    /// the CPU will fault when user-mode code tries to walk the hierarchy.
    ///
    /// Returns a mutable reference to the final page table entry for the given
    /// virtual address.
    fn walk_or_create(&mut self, virt: VirtualAddress, user: bool) -> &mut PageEntry {
        // SAFETY: root is a valid PageTable pointer (either owned or borrowed from Limine).
        let mut table = unsafe { &mut *self.root };
        let virt_addr = virt.as_usize();

        // Walk through all levels except the last
        for level in (1..arch::PAGE_TABLE_LEVELS).rev() {
            let index = arch::page_index(virt_addr, level);
            let entry = table.entry_mut(index);

            if !entry.is_present() {
                // Allocate a new page table.
                // alloc_page_table() returns a *mut PageTable pointing to a zeroed,
                // page-aligned allocation whose physical address (via virt_to_phys) is
                // the address the CPU will use to walk the hierarchy.
                let new_table_ptr = alloc_page_table();
                let new_table_virt_raw = new_table_ptr as usize;

                let translator = AddressTranslator::current();
                let new_table_phys =
                    PhysicalAddress::new(translator.virt_to_phys(new_table_virt_raw));

                let mut flags = PageFlags::empty();
                flags.set_present(true);
                // Intermediate entries must be writable for writes to propagate through the
                // hierarchy; x86_64 CR0.WP enforces the writable bit at every level.
                flags.set_writable(true);
                // Propagate user-accessibility so user-mode code can walk to its own pages.
                if user {
                    flags.set_user(true);
                }

                *entry = PageEntry::new(new_table_phys, flags);
            }

            let next_table_phys = entry.address().expect("entry should be present");
            let translator = AddressTranslator::current();
            let next_table_virt_raw = translator.phys_to_virt(next_table_phys.as_usize());

            // SAFETY: The entry contains a valid physical address of a page table.
            // PageTable is repr(transparent) over the 512-entry array, so this cast is correct
            // for both Limine-allocated and freshly-allocated sub-tables.
            table = unsafe { &mut *(next_table_virt_raw as *mut PageTable) };
        }

        // Return the entry at level 0
        let index = arch::page_index(virt_addr, 0);
        table.entry_mut(index)
    }
}

impl Default for PageDirectory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        use crate::address::AddressTranslator;

        // Set up emulated memory for testing
        let _ = std::panic::catch_unwind(|| {
            AddressTranslator::set_current(AddressTranslator::emulated(64 * 1024));
        });
    }

    #[test]
    fn map_single_page() {
        setup();
        let mut dir = PageDirectory::new();

        // Use addresses within 16-bit range and page-aligned for 16-byte pages
        let virt = VirtualAddress::new(0x0100);
        let phys = PhysicalAddress::new(0x0200);
        let mut flags = PageFlags::empty();
        flags.set_present(true);

        dir.map(virt, phys, flags);

        // The mapping should succeed without panicking
    }

    #[test]
    fn unmap_mapped_page() {
        setup();
        let mut dir = PageDirectory::new();

        // Use addresses within 16-bit range
        let virt = VirtualAddress::new(0x0100);
        let phys = PhysicalAddress::new(0x0200);
        let mut flags = PageFlags::empty();
        flags.set_present(true);

        dir.map(virt, phys, flags);
        let unmapped = dir.unmap(virt);

        assert_eq!(unmapped, Some(phys));
    }

    #[test]
    fn unmap_unmapped_page() {
        setup();
        let mut dir = PageDirectory::new();

        // Use addresses within 16-bit range
        let virt = VirtualAddress::new(0x0100);
        let unmapped = dir.unmap(virt);

        assert_eq!(unmapped, None);
    }

    #[test]
    fn map_multiple_pages() {
        setup();
        let mut dir = PageDirectory::new();

        let mut flags = PageFlags::empty();
        flags.set_present(true);

        // Map several pages using addresses within 16-bit range
        for i in 1..=10 {
            let virt = VirtualAddress::new(i * arch::PAGE_SIZE);
            let phys = PhysicalAddress::new(0x0200 + (i * arch::PAGE_SIZE));
            dir.map(virt, phys, flags);
        }
    }

    #[test]
    fn copy_kernel_half_copies_upper_entries() {
        setup();
        let mut src = PageDirectory::new();
        let mut flags = PageFlags::empty();
        flags.set_present(true);

        // Map a page in the kernel (upper) half.
        // Software arch: KERNEL_ENTRY_START=8, so root index >= 8 means kernel.
        // Root index = bits 12-15. Set bit 15 for canonical kernel address.
        // 0xFFFF_FFFF_FFFF_8000 has root index = 0x8, page-aligned at offset 0.
        let virt = VirtualAddress::new(0xFFFF_FFFF_FFFF_8000);
        let phys = PhysicalAddress::new(0x0010);
        src.map(virt, phys, flags);

        let mut dst = PageDirectory::new();
        dst.copy_kernel_half_from(&src);

        // The entry should be accessible via dst (the full sub-table chain is shared).
        assert_eq!(dst.unmap(virt), Some(phys));
    }

    #[test]
    fn copy_kernel_half_does_not_copy_lower_entries() {
        setup();
        let mut src = PageDirectory::new();
        let mut flags = PageFlags::empty();
        flags.set_present(true);

        // Map a page in the user (lower) half.
        let virt = VirtualAddress::new(arch::PAGE_SIZE);
        let phys = PhysicalAddress::new(0x0010);
        src.map(virt, phys, flags);

        let mut dst = PageDirectory::new();
        dst.copy_kernel_half_from(&src);

        // Lower half must not be copied.
        assert_eq!(dst.unmap(virt), None);
    }
}
