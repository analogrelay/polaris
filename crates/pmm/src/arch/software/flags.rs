//! Page table entry flags for software emulation.

/// Page table entry flags for software emulation.
///
/// This provides a simplified flag implementation for testing. Flags are stored
/// as a raw u64 with specific bits representing different permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFlags(usize);

impl PageFlags {
    /// Present bit (bit 0).
    const PRESENT: usize = 1 << 0;

    /// Writable bit (bit 1).
    const WRITABLE: usize = 1 << 1;

    /// User-accessible bit (bit 2).
    const USER: usize = 1 << 2;

    /// No-execute bit (bit 3).
    const NO_EXECUTE: usize = 1 << 3;

    /// Creates empty page flags (page not present).
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Creates page flags from a raw usize value.
    pub const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    /// Returns the raw usize value of these flags.
    pub const fn to_raw(self) -> usize {
        self.0
    }

    /// Returns whether the present bit is set.
    pub fn is_present(self) -> bool {
        (self.0 & Self::PRESENT) != 0
    }

    /// Sets or clears the present bit.
    pub fn set_present(&mut self, present: bool) {
        if present {
            self.0 |= Self::PRESENT;
        } else {
            self.0 &= !Self::PRESENT;
        }
    }

    /// Returns whether the writable bit is set.
    pub fn is_writable(self) -> bool {
        (self.0 & Self::WRITABLE) != 0
    }

    /// Sets or clears the writable bit.
    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.0 |= Self::WRITABLE;
        } else {
            self.0 &= !Self::WRITABLE;
        }
    }

    /// Returns whether the user-accessible bit is set.
    pub fn is_user(self) -> bool {
        (self.0 & Self::USER) != 0
    }

    /// Sets or clears the user-accessible bit.
    pub fn set_user(&mut self, user: bool) {
        if user {
            self.0 |= Self::USER;
        } else {
            self.0 &= !Self::USER;
        }
    }

    /// Returns whether the no-execute bit is set.
    pub fn is_no_execute(self) -> bool {
        (self.0 & Self::NO_EXECUTE) != 0
    }

    /// Sets or clears the no-execute bit.
    pub fn set_no_execute(&mut self, no_execute: bool) {
        if no_execute {
            self.0 |= Self::NO_EXECUTE;
        } else {
            self.0 &= !Self::NO_EXECUTE;
        }
    }
}

impl Default for PageFlags {
    fn default() -> Self {
        Self::empty()
    }
}
