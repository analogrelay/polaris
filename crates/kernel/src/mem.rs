// cSpell:ignore Hhdm

use core::ptr::NonNull;
use limine::{
    memory_map::{self, Entry},
    request::{HhdmRequest, MemoryMapRequest, StackSizeRequest},
};
use pmm::{
    BlockAllocator, BootMemoryRegion, HumanAddress, HumanSize, MemoryMap, PhysicalAddress,
    VirtualAddress,
};

/// Represents a linker section in the kernel image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkerSection {
    /// The `.text` section containing executable code.
    Text,
    /// The `.rodata` section containing read-only data.
    ReadOnlyData,
    /// The `.data` section containing initialized writable data.
    Data,
    /// The `.bss` section containing uninitialized writable data.
    Bss,
}

/// Represents a memory area classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryArea {
    /// Address is within the kernel image in the specified linker section.
    KernelImage(LinkerSection),
    /// Address is within the kernel stack.
    KernelStack,
    /// Address is in other kernel-mode memory (higher-half).
    KernelOther,
    /// Address is in user-mode memory (lower-half).
    User,
}

impl MemoryArea {
    /// Returns true if this area is a kernel-mode area.
    pub fn is_kernel(self) -> bool {
        matches!(
            self,
            MemoryArea::KernelImage(_) | MemoryArea::KernelStack | MemoryArea::KernelOther
        )
    }

    /// Returns the memory area classification for a given virtual address.
    ///
    /// This function determines which area of memory an address belongs to by
    /// checking it against linker symbols and runtime-calculated bounds.
    pub fn containing(addr: VirtualAddress) -> Self {
        let addr_val = addr.as_usize();

        // Check if it's in user space (lower half)
        if crate::arch::is_user_space(addr_val) {
            return MemoryArea::User;
        }

        // Check kernel stack
        // SAFETY: Stack bounds are set once during initialization before this function is called
        let stack_start = unsafe { STACK_START };
        let stack_end = unsafe { STACK_END };
        if stack_start != 0 && addr_val >= stack_start && addr_val < stack_end {
            return MemoryArea::KernelStack;
        }

        // Check linker sections
        // SAFETY: Linker symbols are valid throughout the kernel's lifetime
        unsafe {
            let text_start = &__kernel_text_start as *const u8 as usize;
            let text_end = &__kernel_text_end as *const u8 as usize;
            if addr_val >= text_start && addr_val < text_end {
                return MemoryArea::KernelImage(LinkerSection::Text);
            }

            let rodata_start = &__kernel_rodata_start as *const u8 as usize;
            let rodata_end = &__kernel_rodata_end as *const u8 as usize;
            if addr_val >= rodata_start && addr_val < rodata_end {
                return MemoryArea::KernelImage(LinkerSection::ReadOnlyData);
            }

            let data_start = &__kernel_data_start as *const u8 as usize;
            let data_end = &__kernel_data_end as *const u8 as usize;
            if addr_val >= data_start && addr_val < data_end {
                return MemoryArea::KernelImage(LinkerSection::Data);
            }

            let bss_start = &__kernel_bss_start as *const u8 as usize;
            let bss_end = &__kernel_bss_end as *const u8 as usize;
            if addr_val >= bss_start && addr_val < bss_end {
                return MemoryArea::KernelImage(LinkerSection::Bss);
            }
        }

        // If it's in the higher half but not in any specific region, it's other kernel memory
        MemoryArea::KernelOther
    }
}

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HIGHER_HALF_DIRECT_MAP: HhdmRequest = HhdmRequest::new();

pub fn type_name(entry_type: memory_map::EntryType) -> &'static str {
    match entry_type {
        memory_map::EntryType::USABLE => "USABLE",
        memory_map::EntryType::RESERVED => "RESERVED",
        memory_map::EntryType::ACPI_RECLAIMABLE => "ACPI_RECLAIMABLE",
        memory_map::EntryType::ACPI_NVS => "ACPI_NVS",
        memory_map::EntryType::BAD_MEMORY => "BAD_MEMORY",
        memory_map::EntryType::BOOTLOADER_RECLAIMABLE => "BOOTLOADER_RECLAIMABLE",
        memory_map::EntryType::EXECUTABLE_AND_MODULES => "EXECUTABLE_AND_MODULES",
        memory_map::EntryType::FRAMEBUFFER => "FRAMEBUFFER",
        _ => "UNKNOWN",
    }
}

/// Wrapper around Limine's memory map entry to implement pmm's `BootMemoryRegion` trait.
#[repr(transparent)]
struct LimineMemoryRegion<'a>(&'a Entry);

impl<'a> LimineMemoryRegion<'a> {
    /// Converts a slice of Entry references to a slice of LimineMemoryRegion.
    ///
    /// # Safety
    /// This is safe because LimineMemoryRegion is `#[repr(transparent)]` over `&Entry`,
    /// so the memory layout is identical.
    fn wrap_slice(entries: &'a [&'a Entry]) -> &'a [LimineMemoryRegion<'a>] {
        // SAFETY: LimineMemoryRegion is #[repr(transparent)] over &Entry
        unsafe { core::mem::transmute(entries) }
    }
}

impl BootMemoryRegion for LimineMemoryRegion<'_> {
    fn base(&self) -> PhysicalAddress {
        PhysicalAddress::new(self.0.base as usize)
    }

    fn size(&self) -> usize {
        self.0.length as usize
    }

    fn is_usable(&self) -> bool {
        self.0.entry_type == memory_map::EntryType::USABLE
    }
}

/// Initializes the block allocator by scanning the memory map.
///
/// Sets the initialized block allocator as the kernel allocator with all
/// usable regions added and all non-usable regions reserved.
pub fn init_allocator() {
    let direct_offset = HIGHER_HALF_DIRECT_MAP
        .get_response()
        .expect("Higher-half direct map request should have been answered")
        .offset();
    pmm::AddressTranslator::set_current(pmm::AddressTranslator::hardware(direct_offset as usize));

    let boot_memmap = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Memory map request should have been answered")
        .entries();

    let mut allocator = BlockAllocator::new();
    for entry in boot_memmap {
        let start = PhysicalAddress::new(entry.base as usize);
        match entry.entry_type {
            memory_map::EntryType::USABLE => {
                allocator
                    .add(start, entry.length as usize)
                    .expect("Failed to add memory region to block allocator");
            }
            _ => {
                allocator
                    .reserve(start, entry.length as usize)
                    .expect("Failed to reserve memory region in block allocator");
            }
        }
    }

    KERNEL_ALLOCATOR.use_block_allocator(allocator);
}

/// Switches the kernel allocator to use the physical memory manager.
pub fn use_pmm(pmm: pmm::PhysicalMemoryManager) {
    KERNEL_ALLOCATOR.use_pmm(pmm);
}

/// Initializes the physical memory manager.
///
/// Returns a PhysicalMemoryManager with all non-usable regions marked as reserved
/// and all usable regions freed into the allocator.
pub fn init_pmm() -> pmm::PhysicalMemoryManager {
    let boot_memmap = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Memory map request should have been answered")
        .entries();

    // Wrap the boot memory map entries without allocation
    let wrapped_entries = LimineMemoryRegion::wrap_slice(boot_memmap);

    // Build the memory map using the trait-based API
    let memory_map = MemoryMap::from_boot_map(wrapped_entries);

    log::info!(
        "memory map: {} sections, {} frames allocated",
        memory_map.sections().len(),
        memory_map.allocated_frame_count()
    );

    let mut pmm = pmm::PhysicalMemoryManager::new(memory_map);

    // Free usable memory into the PMM
    // We deallocate at the maximum order (MAX_ORDER) to minimize fragmentation
    const MAX_ORDER: usize = 11;
    const PAGE_SIZE: u64 = 4096;
    let max_block_size = (1u64 << MAX_ORDER) * PAGE_SIZE;

    for entry in boot_memmap.iter() {
        if entry.entry_type == memory_map::EntryType::USABLE {
            let mut addr = entry.base;
            let end = entry.base + entry.length;

            // Align start address up to page boundary
            if addr % PAGE_SIZE != 0 {
                addr = (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
            }

            // Free memory in the largest possible blocks
            while addr + max_block_size <= end {
                pmm.deallocate(PhysicalAddress::new(addr as usize), MAX_ORDER);
                addr += max_block_size;
            }

            // Free remaining memory in smaller blocks
            for order in (0..MAX_ORDER).rev() {
                let block_size = (1u64 << order) * PAGE_SIZE;
                while addr + block_size <= end {
                    pmm.deallocate(PhysicalAddress::new(addr as usize), order);
                    addr += block_size;
                }
            }
        }
    }

    pmm
}

#[global_allocator]
static KERNEL_ALLOCATOR: KernelAllocator = KernelAllocator {
    inner: spin::Mutex::new(InnerAllocator::None),
};

struct KernelAllocator {
    inner: spin::Mutex<InnerAllocator>,
}

enum InnerAllocator {
    None,
    BlockAllocator(BlockAllocator),
    PhysicalMemoryManager(pmm::PhysicalMemoryManager),
}

impl KernelAllocator {
    pub fn use_block_allocator(&self, allocator: BlockAllocator) {
        let mut inner = self.inner.lock();
        *inner = InnerAllocator::BlockAllocator(allocator);
    }

    pub fn use_pmm(&self, pmm: pmm::PhysicalMemoryManager) {
        let mut inner = self.inner.lock();
        *inner = InnerAllocator::PhysicalMemoryManager(pmm);
    }
}

unsafe impl alloc::alloc::GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: alloc::alloc::Layout) -> *mut u8 {
        use alloc::alloc::Allocator;

        match &mut *self.inner.lock() {
            InnerAllocator::None => core::ptr::null_mut(),
            InnerAllocator::BlockAllocator(allocator) => allocator
                .allocate(layout)
                .map(|pa| pa.cast().as_ptr())
                .inspect_err(|e| log::error!("block allocator error: {:?}", e))
                .unwrap_or(core::ptr::null_mut()),
            InnerAllocator::PhysicalMemoryManager(pmm) => {
                // Calculate the order needed for this allocation
                let size = layout.size().max(layout.align());
                let pages = (size + 4095) / 4096; // Round up to pages
                let order = pages.next_power_of_two().trailing_zeros() as usize;

                pmm.allocate(order)
                    .ok()
                    .map(|pa| pmm::VirtualAddress::direct_mapped(pa).as_mut_ptr::<u8>())
                    .unwrap_or(core::ptr::null_mut())
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: alloc::alloc::Layout) {
        use alloc::alloc::Allocator;

        let Some(ptr_nn) = NonNull::new(ptr) else {
            return;
        };

        match &mut *self.inner.lock() {
            InnerAllocator::None => {}
            InnerAllocator::BlockAllocator(allocator) => unsafe {
                allocator.deallocate(ptr_nn, layout);
            },
            InnerAllocator::PhysicalMemoryManager(pmm) => {
                // Calculate the order for this deallocation
                let size = layout.size().max(layout.align());
                let pages = (size + 4095) / 4096;
                let order = pages.next_power_of_two().trailing_zeros() as usize;

                // Convert virtual address back to physical
                let virt = pmm::VirtualAddress::from_ptr(ptr);
                let phys = pmm::PhysicalAddress::from_direct_mapped(virt);
                pmm.deallocate(phys, order);
            }
        }
    }
}

static mut STACK_START: usize = 0;
static mut STACK_END: usize = 0;

#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE: StackSizeRequest = StackSizeRequest::new().with_size(65536); // 64 KiB stack

pub unsafe fn set_stack_bounds(stack_start: usize) {
    let stack_end = stack_start + STACK_SIZE.size() as usize;
    // SAFETY: This function must only be called once during kernel initialization.
    unsafe {
        STACK_START = stack_start;
        STACK_END = stack_end;
    }
}

// External symbols from the linker script
unsafe extern "C" {
    static __kernel_text_start: u8;
    static __kernel_text_end: u8;
    static __kernel_rodata_start: u8;
    static __kernel_rodata_end: u8;
    static __kernel_data_start: u8;
    static __kernel_data_end: u8;
    static __kernel_bss_start: u8;
    static __kernel_bss_end: u8;
}
