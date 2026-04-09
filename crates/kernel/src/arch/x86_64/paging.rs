use pmm::{AddressTranslator, PageDirectory, PageFlags, PhysicalAddress, VirtualAddress};

/// The kernel's view of the active (Limine-set-up) page tables.
///
/// Initialized lazily on first call to `map_mmio()`. The `PageDirectory` wraps the existing
/// PML4 non-owingly — it will not free the underlying Limine page tables on drop.
static KERNEL_PAGE_DIR: spin::Once<spin::Mutex<PageDirectory>> = spin::Once::new();

fn kernel_page_dir() -> &'static spin::Mutex<PageDirectory> {
    KERNEL_PAGE_DIR.call_once(|| {
        // SAFETY: Called after `mem::use_pmm()`, which sets up the `AddressTranslator`.
        // The active page tables set up by Limine are valid and HHDM-accessible.
        spin::Mutex::new(unsafe { PageDirectory::from_active_tables() })
    })
}

/// Maps `[phys_base, phys_base + size)` into the active page tables at the corresponding
/// HHDM virtual addresses.
///
/// Pages are mapped with cache-disable flags (PWT+PCD, UC- memory type) for MMIO access.
/// After all pages are mapped, the full TLB is flushed.
///
/// # Safety
/// Must be called after `mem::use_pmm()` so that the global allocator is available for
/// allocating any intermediate page table pages.
pub(crate) unsafe fn map_mmio(phys_base: usize, size: usize) {
    let translator = AddressTranslator::current();
    let start = phys_base & !(4096 - 1);
    let end = (phys_base + size + 4095) & !(4096 - 1);

    let mut dir = kernel_page_dir().lock();
    for phys in (start..end).step_by(4096) {
        let virt = VirtualAddress::new(translator.phys_to_virt(phys));
        let mut flags = PageFlags::empty();
        flags.set_writable(true);
        flags.set_write_through(true);
        flags.set_cache_disable(true);
        flags.set_no_execute(true);
        dir.map(virt, PhysicalAddress::new(phys), flags);
    }

    x86_64::instructions::tlb::flush_all();
}
