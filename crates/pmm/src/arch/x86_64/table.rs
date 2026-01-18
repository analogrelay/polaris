//! Page table structure for x86_64 architecture.

use alloc::boxed::Box;

use x86_64::{
    PhysAddr,
    registers::control::{Cr3, Cr3Flags},
    structures::paging::PhysFrame,
};

use crate::{PhysicalAddress, VirtualAddress, address::AddressTranslator};

use super::entry::PageEntry;

/// Number of entries in an x86_64 page table.
const ENTRY_COUNT: usize = 512;

/// A page table for x86_64 architecture.
///
/// This represents a single level in the page table hierarchy. On x86_64 with
/// 4-level paging, there are four levels: PML4 (level 3), PDPT (level 2),
/// PD (level 1), and PT (level 0).
pub struct PageTable {
    /// The underlying page table from the x86_64 crate.
    inner: Box<x86_64::structures::paging::PageTable>,
}

impl PageTable {
    /// Creates a new, empty page table.
    ///
    /// All entries are initialized to zero (not present).
    pub fn new() -> Self {
        Self {
            inner: Box::new(x86_64::structures::paging::PageTable::new()),
        }
    }

    /// Returns a reference to the entry at the given index.
    ///
    /// # Panics
    /// Panics if index >= 512.
    pub fn entry(&self, index: usize) -> PageEntry {
        assert!(index < ENTRY_COUNT, "page table index out of bounds");
        PageEntry::from(
            self.inner[index].addr().as_u64() as usize | self.inner[index].flags().bits() as usize,
        )
    }

    /// Returns a mutable reference to the entry at the given index.
    ///
    /// # Panics
    /// Panics if index >= 512.
    pub fn entry_mut(&mut self, index: usize) -> &mut PageEntry {
        assert!(index < ENTRY_COUNT, "page table index out of bounds");
        // SAFETY: We're reinterpreting the page table entry as our PageEntry type.
        // Both are 64-bit values with the same layout.
        unsafe { &mut *(&mut self.inner[index] as *mut _ as *mut PageEntry) }
    }

    /// Returns the number of entries in this page table.
    pub const fn len(&self) -> usize {
        ENTRY_COUNT
    }

    /// Returns the physical address of this page table.
    ///
    /// This is the address that would be stored in a parent page table entry
    /// or loaded into CR3.
    pub fn physical_address(&self) -> PhysicalAddress {
        // We need to translate the virtual address of this table to a physical address.
        // This requires the address translator.
        let ptr = &self.inner[0] as *const _ as *const u8;
        let virt_addr = VirtualAddress::new(ptr as usize);
        let translator = AddressTranslator::current();
        PhysicalAddress::new(translator.virt_to_phys(virt_addr.as_usize()))
    }

    /// Activates this page table by loading it into CR3.
    ///
    /// # Safety
    /// This function is unsafe because loading an invalid page table can cause
    /// undefined behavior, including memory corruption and system crashes.
    /// The caller must ensure:
    /// - The page table correctly maps all memory that will be accessed
    /// - The kernel is properly mapped
    /// - The page table itself is mapped
    pub unsafe fn activate(&self) {
        let phys_addr = PhysAddr::new(self.physical_address().as_usize() as u64);
        let frame = PhysFrame::containing_address(phys_addr);
        // SAFETY: Caller must ensure the page table is valid
        unsafe {
            Cr3::write(frame, Cr3Flags::empty());
        }
    }

    /// Returns a reference to the inner x86_64 page table.
    pub fn inner(&self) -> &x86_64::structures::paging::PageTable {
        &self.inner
    }

    /// Returns a mutable reference to the inner x86_64 page table.
    pub fn inner_mut(&mut self) -> &mut x86_64::structures::paging::PageTable {
        &mut self.inner
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}
