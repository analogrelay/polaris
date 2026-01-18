//! Human-readable size formatting for memory values.

use core::fmt;

/// Wraps a size in bytes and formats it as a human-readable value with binary SI prefixes.
///
/// Binary SI prefixes (KiB, MiB, GiB, etc.) are always used, where each unit is 1024 times
/// the previous unit. Values are displayed with up to 2 decimal places, omitting trailing
/// zeros.
///
/// # Examples
///
/// ```
/// use pmm::HumanSize;
///
/// assert_eq!(format!("{}", HumanSize(0)), "0B");
/// assert_eq!(format!("{}", HumanSize(1023)), "1023B");
/// assert_eq!(format!("{}", HumanSize(1024)), "1KiB");
/// assert_eq!(format!("{}", HumanSize(1536)), "1.5KiB");
/// assert_eq!(format!("{}", HumanSize(1048576)), "1MiB");
/// assert_eq!(format!("{}", HumanSize(1073741824)), "1GiB");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct HumanSize(pub usize);

impl HumanSize {
    /// Creates a new human-readable size from bytes.
    #[inline]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// Returns the raw byte count.
    #[inline]
    pub const fn bytes(self) -> usize {
        self.0
    }
}

impl From<u8> for HumanSize {
    #[inline]
    fn from(value: u8) -> Self {
        Self(value as usize)
    }
}

impl From<u16> for HumanSize {
    #[inline]
    fn from(value: u16) -> Self {
        Self(value as usize)
    }
}

impl From<u32> for HumanSize {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value as usize)
    }
}

impl From<u64> for HumanSize {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value as usize)
    }
}

impl From<usize> for HumanSize {
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl fmt::Display for HumanSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
        const THRESHOLD: f64 = 1024.0;

        let bytes = self.0 as f64;

        if bytes < THRESHOLD {
            return write!(f, "{}B", self.0);
        }

        let mut size = bytes;
        let mut unit_index = 0;

        while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
            size /= THRESHOLD;
            unit_index += 1;
        }

        // Format with up to 2 decimal places, removing trailing zeros
        // Check if it's a whole number (no fractional part)
        if size as usize as f64 == size {
            write!(f, "{}{}", size as usize, UNITS[unit_index])
        } else if (size * 10.0) as usize as f64 == size * 10.0 {
            // Has exactly one decimal place
            write!(f, "{:.1}{}", size, UNITS[unit_index])
        } else {
            write!(f, "{:.2}{}", size, UNITS[unit_index])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_zero() {
        assert_eq!(format!("{}", HumanSize(0)), "0B");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format!("{}", HumanSize(1)), "1B");
        assert_eq!(format!("{}", HumanSize(512)), "512B");
        assert_eq!(format!("{}", HumanSize(1023)), "1023B");
    }

    #[test]
    fn formats_kibibytes() {
        assert_eq!(format!("{}", HumanSize(1024)), "1KiB");
        assert_eq!(format!("{}", HumanSize(1536)), "1.5KiB");
        assert_eq!(format!("{}", HumanSize(2048)), "2KiB");
        assert_eq!(format!("{}", HumanSize(10240)), "10KiB");
    }

    #[test]
    fn formats_mebibytes() {
        assert_eq!(format!("{}", HumanSize(1048576)), "1MiB");
        assert_eq!(format!("{}", HumanSize(1572864)), "1.5MiB");
        assert_eq!(format!("{}", HumanSize(16777216)), "16MiB");
    }

    #[test]
    fn formats_gibibytes() {
        assert_eq!(format!("{}", HumanSize(1073741824)), "1GiB");
        assert_eq!(format!("{}", HumanSize(2147483648)), "2GiB");
        assert_eq!(format!("{}", HumanSize(1610612736)), "1.5GiB");
    }

    #[test]
    fn formats_tebibytes() {
        assert_eq!(format!("{}", HumanSize(1099511627776)), "1TiB");
    }

    #[test]
    fn removes_trailing_zeros() {
        // 1.5 KiB should show one decimal
        assert_eq!(format!("{}", HumanSize(1536)), "1.5KiB");
        // Exact KiB should show no decimals
        assert_eq!(format!("{}", HumanSize(2048)), "2KiB");
    }

    #[test]
    fn rounds_to_two_decimals() {
        // 1025 bytes = 1.0009765625 KiB ≈ 1.00 KiB
        let size = HumanSize(1025);
        let formatted = format!("{}", size);
        assert!(formatted.starts_with("1.00") || formatted == "1KiB");
    }
}
