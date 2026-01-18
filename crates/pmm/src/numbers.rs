//! Page and frame number types for memory management.
//!
//! This module provides newtypes for physical frame numbers and virtual page numbers,
//! which are used throughout the memory management subsystem.

use crate::{
    address::{PhysicalAddress, VirtualAddress},
    arch,
};
use core::{
    fmt,
    iter::Step,
    ops::{Add, Sub},
};

/// Macro to define common page/frame number functionality.
///
/// This macro generates the basic structure and methods common to both frame
/// and page number types, reducing code duplication.
macro_rules! impl_page_number_common {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name(usize);

        impl $name {
            /// Creates a new page/frame number.
            #[inline]
            pub const fn new(number: usize) -> Self {
                Self(number)
            }

            /// Returns the raw page/frame number.
            #[inline]
            pub const fn as_usize(self) -> usize {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Add<usize> for $name {
            type Output = Self;

            #[inline]
            fn add(self, rhs: usize) -> Self::Output {
                Self(self.0 + rhs)
            }
        }

        impl Sub<usize> for $name {
            type Output = Self;

            #[inline]
            fn sub(self, rhs: usize) -> Self::Output {
                Self(self.0 - rhs)
            }
        }

        impl Sub<$name> for $name {
            type Output = usize;

            #[inline]
            fn sub(self, rhs: $name) -> Self::Output {
                self.0 - rhs.0
            }
        }

        impl Step for $name {
            fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
                if start <= end {
                    let diff = end.0 - start.0;
                    (diff, Some(diff))
                } else {
                    (0, None)
                }
            }

            fn forward_checked(start: Self, count: usize) -> Option<Self> {
                start.0.checked_add(count).map(Self)
            }

            fn backward_checked(start: Self, count: usize) -> Option<Self> {
                start.0.checked_sub(count).map(Self)
            }
        }
    };
}

impl_page_number_common!(
    FrameNumber,
    "A physical memory frame number.\n\n\
     Represents a physical memory frame, which is the physical memory equivalent of a page.\n\
     Frame numbers are zero-indexed and correspond to PAGE_SIZE-aligned physical addresses."
);

impl FrameNumber {
    /// Returns the physical address at the start of this frame.
    #[inline]
    pub const fn start(self) -> PhysicalAddress {
        PhysicalAddress::new(self.0 * arch::PAGE_SIZE)
    }

    /// Returns the physical address at the end of this frame (start of next frame).
    #[inline]
    pub const fn end(self) -> PhysicalAddress {
        PhysicalAddress::new((self.0 + 1) * arch::PAGE_SIZE)
    }
}

impl From<PhysicalAddress> for FrameNumber {
    #[inline]
    fn from(addr: PhysicalAddress) -> Self {
        Self::new(addr.as_usize() / arch::PAGE_SIZE)
    }
}

impl_page_number_common!(
    PageNumber,
    "A virtual memory page number.\n\n\
     Represents a virtual memory page. Page numbers are zero-indexed and correspond to\n\
     PAGE_SIZE-aligned virtual addresses."
);

impl PageNumber {
    /// Returns the virtual address at the start of this page.
    #[inline]
    pub const fn start(self) -> VirtualAddress {
        VirtualAddress::new(self.0 * arch::PAGE_SIZE)
    }

    /// Returns the virtual address at the end of this page (start of next page).
    #[inline]
    pub const fn end(self) -> VirtualAddress {
        VirtualAddress::new((self.0 + 1) * arch::PAGE_SIZE)
    }
}

impl From<VirtualAddress> for PageNumber {
    #[inline]
    fn from(addr: VirtualAddress) -> Self {
        Self::new(addr.as_usize() / arch::PAGE_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod frame_number {
        use super::*;

        #[test]
        fn new_frame() {
            let frame = FrameNumber::new(42);
            assert_eq!(frame.as_usize(), 42);
        }

        #[test]
        fn start_address() {
            let frame = FrameNumber::new(1);
            assert_eq!(frame.start().as_usize(), arch::PAGE_SIZE);
        }

        #[test]
        fn end_address() {
            let frame = FrameNumber::new(1);
            assert_eq!(frame.end().as_usize(), 2 * arch::PAGE_SIZE);
        }

        #[test]
        fn from_physical_address() {
            // Use an offset smaller than PAGE_SIZE
            let addr = PhysicalAddress::new(arch::PAGE_SIZE * 3 + 10);
            let frame = FrameNumber::from(addr);
            assert_eq!(frame.as_usize(), 3);
        }

        #[test]
        fn from_aligned_address() {
            let addr = PhysicalAddress::new(arch::PAGE_SIZE * 5);
            let frame = FrameNumber::from(addr);
            assert_eq!(frame.as_usize(), 5);
        }

        #[test]
        fn add_offset() {
            let frame = FrameNumber::new(10);
            let result = frame + 5;
            assert_eq!(result.as_usize(), 15);
        }

        #[test]
        fn sub_offset() {
            let frame = FrameNumber::new(10);
            let result = frame - 3;
            assert_eq!(result.as_usize(), 7);
        }

        #[test]
        fn sub_frame() {
            let frame1 = FrameNumber::new(10);
            let frame2 = FrameNumber::new(3);
            let diff = frame1 - frame2;
            assert_eq!(diff, 7);
        }

        #[test]
        fn comparison() {
            let frame1 = FrameNumber::new(5);
            let frame2 = FrameNumber::new(10);
            let frame3 = FrameNumber::new(5);

            assert!(frame1 < frame2);
            assert!(frame2 > frame1);
            assert_eq!(frame1, frame3);
            assert_ne!(frame1, frame2);
        }

        #[test]
        fn round_trip() {
            let frame = FrameNumber::new(42);
            let addr = frame.start();
            let recovered = FrameNumber::from(addr);
            assert_eq!(frame, recovered);
        }
    }

    mod page_number {
        use super::*;

        #[test]
        fn new_page() {
            let page = PageNumber::new(42);
            assert_eq!(page.as_usize(), 42);
        }

        #[test]
        fn start_address() {
            let page = PageNumber::new(1);
            assert_eq!(page.start().as_usize(), arch::PAGE_SIZE);
        }

        #[test]
        fn end_address() {
            let page = PageNumber::new(1);
            assert_eq!(page.end().as_usize(), 2 * arch::PAGE_SIZE);
        }

        #[test]
        fn from_virtual_address() {
            // Use an offset smaller than PAGE_SIZE
            let addr = VirtualAddress::new(arch::PAGE_SIZE * 3 + 10);
            let page = PageNumber::from(addr);
            assert_eq!(page.as_usize(), 3);
        }

        #[test]
        fn from_aligned_address() {
            let addr = VirtualAddress::new(arch::PAGE_SIZE * 5);
            let page = PageNumber::from(addr);
            assert_eq!(page.as_usize(), 5);
        }

        #[test]
        fn add_offset() {
            let page = PageNumber::new(10);
            let result = page + 5;
            assert_eq!(result.as_usize(), 15);
        }

        #[test]
        fn sub_offset() {
            let page = PageNumber::new(10);
            let result = page - 3;
            assert_eq!(result.as_usize(), 7);
        }

        #[test]
        fn sub_page() {
            let page1 = PageNumber::new(10);
            let page2 = PageNumber::new(3);
            let diff = page1 - page2;
            assert_eq!(diff, 7);
        }

        #[test]
        fn comparison() {
            let page1 = PageNumber::new(5);
            let page2 = PageNumber::new(10);
            let page3 = PageNumber::new(5);

            assert!(page1 < page2);
            assert!(page2 > page1);
            assert_eq!(page1, page3);
            assert_ne!(page1, page2);
        }

        #[test]
        fn round_trip() {
            let page = PageNumber::new(42);
            let addr = page.start();
            let recovered = PageNumber::from(addr);
            assert_eq!(page, recovered);
        }
    }
}
