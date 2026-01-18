//! Human-readable address formatting for memory addresses.

use core::fmt;

/// Wraps an address and formats it as an uppercase hexadecimal value with `0x` prefix
/// and `_` digit separators every 4 digits.
///
/// # Examples
///
/// ```
/// use pmm::HumanAddress;
///
/// assert_eq!(format!("{}", HumanAddress(0x0)), "0x0");
/// assert_eq!(format!("{}", HumanAddress(0x1000)), "0x1000");
/// assert_eq!(format!("{}", HumanAddress(0x2311_324F)), "0x2311_324F");
/// assert_eq!(format!("{}", HumanAddress(0xDEAD_BEEF_CAFE)), "0xDEAD_BEEF_CAFE");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct HumanAddress(pub usize);

impl HumanAddress {
    /// Creates a new human-readable address.
    #[inline]
    pub const fn new(address: usize) -> Self {
        Self(address)
    }

    /// Returns the raw address value.
    #[inline]
    pub const fn address(self) -> usize {
        self.0
    }
}

impl From<u8> for HumanAddress {
    #[inline]
    fn from(value: u8) -> Self {
        Self(value as usize)
    }
}

impl From<u16> for HumanAddress {
    #[inline]
    fn from(value: u16) -> Self {
        Self(value as usize)
    }
}

impl From<u32> for HumanAddress {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value as usize)
    }
}

impl From<u64> for HumanAddress {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value as usize)
    }
}

impl From<usize> for HumanAddress {
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl fmt::Display for HumanAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return write!(f, "0x0");
        }

        write!(f, "0x")?;

        let value = self.0;

        // Count the number of hex digits needed
        let num_digits = if value == 0 {
            1
        } else {
            // Count bits, then convert to hex digits (4 bits per digit)
            let bits = usize::BITS - value.leading_zeros();
            ((bits + 3) / 4) as usize
        };

        // Calculate leading digits before the first separator
        let leading = if num_digits % 4 == 0 {
            4
        } else {
            num_digits % 4
        };

        // Write leading digits
        for i in 0..leading {
            let shift = (num_digits - 1 - i) * 4;
            let digit = ((value >> shift) & 0xF) as u8;
            let c = if digit < 10 {
                b'0' + digit
            } else {
                b'A' + (digit - 10)
            };
            write!(f, "{}", c as char)?;
        }

        // Write remaining digits in groups of 4
        let remaining = num_digits - leading;
        for group in 0..(remaining / 4) {
            write!(f, "_")?;
            for i in 0..4 {
                let digit_index = leading + group * 4 + i;
                let shift = (num_digits - 1 - digit_index) * 4;
                let digit = ((value >> shift) & 0xF) as u8;
                let c = if digit < 10 {
                    b'0' + digit
                } else {
                    b'A' + (digit - 10)
                };
                write!(f, "{}", c as char)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_zero() {
        assert_eq!(format!("{}", HumanAddress(0x0)), "0x0");
    }

    #[test]
    fn formats_small_values() {
        assert_eq!(format!("{}", HumanAddress(0x1)), "0x1");
        assert_eq!(format!("{}", HumanAddress(0xF)), "0xF");
        assert_eq!(format!("{}", HumanAddress(0xFF)), "0xFF");
        assert_eq!(format!("{}", HumanAddress(0xFFF)), "0xFFF");
    }

    #[test]
    fn formats_four_digit_values() {
        assert_eq!(format!("{}", HumanAddress(0x1000)), "0x1000");
        assert_eq!(format!("{}", HumanAddress(0xABCD)), "0xABCD");
        assert_eq!(format!("{}", HumanAddress(0xFFFF)), "0xFFFF");
    }

    #[test]
    fn formats_with_separators() {
        assert_eq!(format!("{}", HumanAddress(0x1_0000)), "0x1_0000");
        assert_eq!(format!("{}", HumanAddress(0x12_3456)), "0x12_3456");
        assert_eq!(format!("{}", HumanAddress(0x2311_324F)), "0x2311_324F");
        assert_eq!(format!("{}", HumanAddress(0xDEAD_BEEF)), "0xDEAD_BEEF");
    }

    #[test]
    fn formats_large_addresses() {
        assert_eq!(
            format!("{}", HumanAddress(0xDEAD_BEEF_CAFE)),
            "0xDEAD_BEEF_CAFE"
        );
        assert_eq!(
            format!("{}", HumanAddress(0x1_2345_6789_ABCD)),
            "0x1_2345_6789_ABCD"
        );
        assert_eq!(
            format!("{}", HumanAddress(0xFFFF_FFFF_FFFF_FFFF)),
            "0xFFFF_FFFF_FFFF_FFFF"
        );
    }

    #[test]
    fn formats_various_lengths() {
        assert_eq!(format!("{}", HumanAddress(0x1)), "0x1");
        assert_eq!(format!("{}", HumanAddress(0x12)), "0x12");
        assert_eq!(format!("{}", HumanAddress(0x123)), "0x123");
        assert_eq!(format!("{}", HumanAddress(0x1234)), "0x1234");
        assert_eq!(format!("{}", HumanAddress(0x1_2345)), "0x1_2345");
        assert_eq!(format!("{}", HumanAddress(0x12_3456)), "0x12_3456");
        assert_eq!(format!("{}", HumanAddress(0x123_4567)), "0x123_4567");
        assert_eq!(format!("{}", HumanAddress(0x1234_5678)), "0x1234_5678");
    }
}
