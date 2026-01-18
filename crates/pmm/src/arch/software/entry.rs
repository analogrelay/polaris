//! Page table entry for software emulation.

use crate::PhysicalAddress;

use super::flags::PageFlags;

/// A single page table entry for software emulation.
///
/// This is a scale model of x86_64 using 16-bit addresses stored in 64-bit values.
/// The entry format:
/// - Bits 0-3: Flags (4 bits reserved for flags)
/// - Bits 4-19: Physical address (16 bits, sign-extended to 64 bits)
/// - Bits 20-63: Reserved (must be sign-extension of bit 19)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PageEntry(usize);

impl PageEntry {
    /// Physical address mask (bits 4-19, aligned to 16-byte pages).
    /// Bits 4-7 are the page offset alignment (16 bytes = 2^4).
    /// Bits 8-19 are the actual address bits we care about.
    const ADDRESS_MASK: usize = 0xFFFF0;

    /// Flag bits mask (bits 0-3).
    const FLAGS_MASK: usize = 0xF;

    /// Huge page bit (bit 7 in the address field, which is bit 11 overall).
    const HUGE_PAGE_BIT: usize = 1 << 7;

    /// Creates a new page table entry.
    ///
    /// The physical address must be page-aligned (lowest 4 bits must be zero for 16-byte pages).
    pub fn new(address: PhysicalAddress, flags: PageFlags) -> Self {
        debug_assert!(
            address.as_usize() & 0xF == 0,
            "physical address must be page-aligned (16-byte alignment)"
        );

        // Canonicalize the address (sign-extend from bit 15)
        let addr = Self::canonicalize(address.as_usize());
        let addr_bits = addr & Self::ADDRESS_MASK;
        let flag_bits = flags.to_raw() & Self::FLAGS_MASK;
        Self(addr_bits | flag_bits)
    }

    /// Returns the physical address stored in this entry.
    ///
    /// Returns None if the entry is not present.
    pub fn address(self) -> Option<PhysicalAddress> {
        if self.is_present() {
            // Extract and de-canonicalize the address
            let addr = (self.0 & Self::ADDRESS_MASK) & 0xFFFF;
            Some(PhysicalAddress::new(addr))
        } else {
            None
        }
    }

    /// Returns the flags for this entry.
    pub fn flags(self) -> PageFlags {
        PageFlags::from_raw(self.0 & Self::FLAGS_MASK)
    }

    /// Sets the flags for this entry, preserving the address.
    pub fn set_flags(&mut self, flags: PageFlags) {
        let addr_bits = self.0 & Self::ADDRESS_MASK;
        let flag_bits = flags.to_raw() & Self::FLAGS_MASK;
        self.0 = addr_bits | flag_bits;
    }

    /// Returns whether this entry is present (valid).
    pub fn is_present(self) -> bool {
        self.flags().is_present()
    }

    /// Returns whether this entry is a leaf entry (maps a page directly).
    ///
    /// For software emulation, this is determined by the huge page bit.
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

    /// Creates an entry from a raw usize value.
    pub const fn from_usize(value: usize) -> Self {
        Self(value)
    }

    /// Canonicalizes a 16-bit address by sign-extending bit 15 to bits 16-63.
    const fn canonicalize(addr: usize) -> usize {
        let addr_16 = addr & 0xFFFF;
        if (addr_16 & 0x8000) != 0 {
            // Sign bit is set, extend with 1s
            addr_16 | 0xFFFF_FFFF_FFFF_0000
        } else {
            // Sign bit is clear, keep as is
            addr_16
        }
    }
}

impl Default for PageEntry {
    fn default() -> Self {
        Self(0)
    }
}
