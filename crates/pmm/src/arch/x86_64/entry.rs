//! Page table entry for x86_64 architecture.

use crate::PhysicalAddress;

use super::flags::PageFlags;

/// A single page table entry for x86_64.
///
/// On x86_64, page table entries are 64-bit values containing a physical address
/// and various flags. This type provides low-level manipulation of these entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PageEntry(usize);

impl PageEntry {
    /// Physical address mask for x86_64 page table entries.
    /// Bits 12-51 contain the physical address (assuming 52-bit physical addresses).
    const ADDRESS_MASK: usize = 0x000F_FFFF_FFFF_F000;

    /// Flag bits mask (bits 0-11 and 52-63).
    const FLAGS_MASK: usize = !Self::ADDRESS_MASK;

    /// Bit indicating this is a huge page (2MB or 1GB).
    const HUGE_PAGE_BIT: usize = 1 << 7;

    /// Creates a new page table entry.
    ///
    /// The physical address must be page-aligned (lowest 12 bits must be zero).
    pub fn new(address: PhysicalAddress, flags: PageFlags) -> Self {
        debug_assert!(
            address.as_usize() & 0xFFF == 0,
            "physical address must be page-aligned"
        );

        let addr_bits = address.as_usize() & Self::ADDRESS_MASK;
        let flag_bits = flags.as_usize() & Self::FLAGS_MASK;
        Self(addr_bits | flag_bits)
    }

    /// Returns the physical address stored in this entry.
    ///
    /// Returns None if the entry is not present.
    pub fn address(self) -> Option<PhysicalAddress> {
        if self.is_present() {
            Some(PhysicalAddress::new(self.0 & Self::ADDRESS_MASK))
        } else {
            None
        }
    }

    /// Returns the flags for this entry.
    pub fn flags(self) -> PageFlags {
        PageFlags::from(self.0 & Self::FLAGS_MASK)
    }

    /// Sets the flags for this entry, preserving the address.
    pub fn set_flags(&mut self, flags: PageFlags) {
        let addr_bits = self.0 & Self::ADDRESS_MASK;
        let flag_bits = flags.as_usize() & Self::FLAGS_MASK;
        self.0 = addr_bits | flag_bits;
    }

    /// Returns whether this entry is present (valid).
    pub fn is_present(self) -> bool {
        self.flags().is_present()
    }

    /// Returns whether this entry is a leaf entry (maps a page directly).
    ///
    /// For x86_64, this is determined by the huge page bit. If set at level 1 or 2,
    /// this entry maps a 2MB or 1GB page respectively. At level 0 (the lowest level),
    /// all present entries are leaf entries.
    pub fn is_leaf(self) -> bool {
        self.is_present() && (self.0 & Self::HUGE_PAGE_BIT != 0)
    }

    /// Clears this entry (sets it to zero).
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Returns the raw usize value of this entry.
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for PageEntry {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl Default for PageEntry {
    fn default() -> Self {
        Self(0)
    }
}
