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
/// This type owns the root page table and provides operations for mapping and unmapping
/// virtual addresses to physical addresses. It handles walking the page table hierarchy
/// and allocating intermediate tables as needed.
pub struct PageDirectory {
    /// The root page table for this address space.
    root: PageTable,
}

impl PageDirectory {
    /// Creates a new page directory with an empty root page table.
    pub fn new() -> Self {
        Self {
            root: PageTable::new(),
        }
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

        let entry = self.walk_or_create(virt);
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
        let mut table = &mut self.root;
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

            // SAFETY: We're trusting that the page table entry contains a valid pointer
            // to a page table. This is safe as long as we only create entries that
            // point to valid page tables.
            table = unsafe { &mut *(next_table_virt_raw as *mut PageTable) };
        }

        let index = arch::page_index(virt_addr, 0);
        Some(table.entry_mut(index))
    }

    /// Walks the page table hierarchy, creating intermediate tables as needed.
    ///
    /// Returns a mutable reference to the final page table entry for the given
    /// virtual address.
    fn walk_or_create(&mut self, virt: VirtualAddress) -> &mut PageEntry {
        let mut table = &mut self.root;
        let virt_addr = virt.as_usize();

        // Walk through all levels except the last
        for level in (1..arch::PAGE_TABLE_LEVELS).rev() {
            let index = arch::page_index(virt_addr, level);
            let entry = table.entry_mut(index);

            if !entry.is_present() {
                // Allocate a new page table
                let new_table_ptr = alloc_page_table();
                let new_table_virt_raw = new_table_ptr as usize;

                let translator = AddressTranslator::current();
                let new_table_phys =
                    PhysicalAddress::new(translator.virt_to_phys(new_table_virt_raw));

                let mut flags = PageFlags::empty();
                flags.set_present(true);

                *entry = PageEntry::new(new_table_phys, flags);
            }

            let next_table_phys = entry.address().expect("entry should be present");
            let translator = AddressTranslator::current();
            let next_table_virt_raw = translator.phys_to_virt(next_table_phys.as_usize());

            // SAFETY: We're trusting that the page table entry contains a valid pointer
            // to a page table. This is safe because we either just created it above,
            // or it was created by a previous call to this function.
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
}
