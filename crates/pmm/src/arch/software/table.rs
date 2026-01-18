//! Page table structure for software emulation.

use alloc::boxed::Box;

use crate::{PhysicalAddress, VirtualAddress};

use super::entry::PageEntry;

/// Number of entries in a software-emulated page table.
/// With 4-bit indexes, we have 16 entries per table.
const ENTRY_COUNT: usize = 16;

/// A page table for software emulation.
///
/// This is a scale model of x86_64 page tables:
/// - 16-bit virtual addresses (stored as u64 with sign-extension)
/// - 3 levels of page tables (level 2, level 1, level 0)
/// - 4-bit index at each level (16 entries per table)
/// - 4-bit page offset (16-byte pages)
///
/// Address layout:
/// - Bits 0-3: Page offset (16 bytes)
/// - Bits 4-7: Level 0 index (PT)
/// - Bits 8-11: Level 1 index (PD)
/// - Bits 12-15: Level 2 index (PDP/root)
pub struct PageTable {
    /// The entries in this page table.
    entries: Box<[PageEntry; ENTRY_COUNT]>,
}

impl PageTable {
    /// Creates a new, empty page table.
    ///
    /// All entries are initialized to zero (not present).
    pub fn new() -> Self {
        Self {
            entries: Box::new([PageEntry::default(); ENTRY_COUNT]),
        }
    }

    /// Returns a reference to the entry at the given index.
    ///
    /// # Panics
    /// Panics if index >= 16.
    pub fn entry(&self, index: usize) -> PageEntry {
        assert!(index < ENTRY_COUNT, "page table index out of bounds");
        self.entries[index]
    }

    /// Returns a mutable reference to the entry at the given index.
    ///
    /// # Panics
    /// Panics if index >= 16.
    pub fn entry_mut(&mut self, index: usize) -> &mut PageEntry {
        assert!(index < ENTRY_COUNT, "page table index out of bounds");
        &mut self.entries[index]
    }

    /// Returns the number of entries in this page table.
    pub const fn len(&self) -> usize {
        ENTRY_COUNT
    }

    /// Returns the physical address of this page table.
    ///
    /// This is the address that would be stored in a parent page table entry
    /// or used as the root table address.
    pub fn physical_address(&self) -> PhysicalAddress {
        // In software emulation, we treat the pointer to the entries array as the address.
        // We need to translate this virtual address to a physical address.
        let ptr = self.entries.as_ptr() as *const u8;
        let virt_addr = VirtualAddress::new(ptr as usize);
        let translator = crate::address::AddressTranslator::current();
        PhysicalAddress::new(translator.virt_to_phys(virt_addr.as_usize()))
    }

    /// Activates this page table by setting it as the current root table.
    ///
    /// In software emulation, this would typically update a thread-local or global
    /// state to track the current page table.
    ///
    /// # Safety
    /// This function is unsafe because loading an invalid page table can cause
    /// undefined behavior. The caller must ensure:
    /// - The page table correctly maps all memory that will be accessed
    /// - The kernel is properly mapped
    /// - The page table itself is mapped
    pub unsafe fn activate(&self) {
        // In software emulation, we don't actually change hardware state.
        // This is a no-op for now, but could update emulated state in the future.
        // For example, we might store the current page table in thread-local storage.
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}
