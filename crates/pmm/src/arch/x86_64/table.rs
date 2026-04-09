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
/// This is a transparent newtype over [`x86_64::structures::paging::PageTable`], so a pointer
/// to `PageTable` is always a pointer to the actual 4096-byte array of page table entries.
/// This property is required for correct interaction with the CPU's page-table walker and for
/// safely casting HHDM-mapped physical page table addresses to `*mut PageTable`.
#[repr(transparent)]
pub struct PageTable(x86_64::structures::paging::PageTable);

impl PageTable {
    /// Creates a new, empty page table.
    ///
    /// All entries are initialized to zero (not present).
    pub fn new() -> Self {
        Self(x86_64::structures::paging::PageTable::new())
    }

    /// Returns a reference to the entry at the given index.
    ///
    /// # Panics
    /// Panics if index >= 512.
    pub fn entry(&self, index: usize) -> PageEntry {
        assert!(index < ENTRY_COUNT, "page table index out of bounds");
        PageEntry::from(
            self.0[index].addr().as_u64() as usize | self.0[index].flags().bits() as usize,
        )
    }

    /// Returns a mutable reference to the entry at the given index.
    ///
    /// # Panics
    /// Panics if index >= 512.
    pub fn entry_mut(&mut self, index: usize) -> &mut PageEntry {
        assert!(index < ENTRY_COUNT, "page table index out of bounds");
        // SAFETY: PageEntry and x86_64::PageTableEntry are both 64-bit values with the same
        // physical-address | flags layout.
        unsafe { &mut *(&mut self.0[index] as *mut _ as *mut PageEntry) }
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
        let virt = self as *const Self as usize;
        PhysicalAddress::new(AddressTranslator::current().virt_to_phys(virt))
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
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}
