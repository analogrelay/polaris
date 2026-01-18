//! Page table entry flags for x86_64 architecture.

/// Page table entry flags for x86_64.
///
/// This wraps the x86_64 crate's page table entry flags, providing a minimal
/// interface for flag manipulation without higher-level abstractions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFlags(x86_64::structures::paging::PageTableFlags);

impl From<usize> for PageFlags {
    fn from(value: usize) -> Self {
        Self(x86_64::structures::paging::PageTableFlags::from_bits_truncate(value as u64))
    }
}

impl PageFlags {
    /// Creates empty page flags (page not present).
    pub const fn empty() -> Self {
        Self(x86_64::structures::paging::PageTableFlags::empty())
    }

    /// Returns the raw usize value of these flags.
    pub const fn as_usize(self) -> usize {
        self.0.bits() as usize
    }

    /// Returns whether the present bit is set.
    pub fn is_present(self) -> bool {
        self.0
            .contains(x86_64::structures::paging::PageTableFlags::PRESENT)
    }

    /// Sets or clears the present bit.
    pub fn set_present(&mut self, present: bool) {
        self.0
            .set(x86_64::structures::paging::PageTableFlags::PRESENT, present);
    }
}

impl Default for PageFlags {
    fn default() -> Self {
        Self::empty()
    }
}
