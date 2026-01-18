//! Address types for physical and virtual memory management.
//!
//! This module provides architecture-independent wrappers around physical and virtual
//! addresses, with methods to manipulate them for page table operations.

use core::fmt;
use core::ops::{Add, Sub};

use crate::{FrameNumber, arch};

#[cfg(any(test, feature = "software-emulation"))]
use crate::arch::EmulatedMemory;

/// Address translator for converting between physical and virtual addresses.
///
/// This enum supports two modes:
/// - Hardware: Uses a direct-map offset for translation (kernel mode)
/// - Emulated: Uses an emulated memory buffer for translation (testing mode)
pub enum AddressTranslator {
    /// Hardware translation using a direct-map offset.
    Hardware { direct_map_offset: usize },
    /// Emulated translation using a simulated memory region.
    #[cfg(any(test, feature = "software-emulation"))]
    Emulated(EmulatedMemory),
}

impl AddressTranslator {
    /// Creates a new hardware translator with the given direct-map offset.
    pub const fn hardware(direct_map_offset: usize) -> Self {
        Self::Hardware { direct_map_offset }
    }

    /// Creates a new emulated translator with the given memory size.
    #[cfg(any(test, feature = "software-emulation"))]
    pub fn emulated(size: usize) -> Self {
        Self::Emulated(EmulatedMemory::new(size))
    }

    /// Sets the global address translator.
    ///
    /// This function must be called exactly once during initialization.
    ///
    /// # Panics
    ///
    /// Panics if the translator has already been set.
    pub fn set_current(translator: AddressTranslator) {
        #[cfg(not(any(test, feature = "software-emulation")))]
        {
            if ADDRESS_TRANSLATOR.get().is_some() {
                panic!("address translator already set");
            }
            ADDRESS_TRANSLATOR.call_once(|| translator);
        }

        #[cfg(any(test, feature = "software-emulation"))]
        {
            ADDRESS_TRANSLATOR.with(|t| {
                if t.get().is_some() {
                    panic!("address translator already set");
                }
                t.call_once(|| translator);
            });
        }
    }

    /// Returns a reference to the current global address translator.
    ///
    /// # Panics
    ///
    /// Panics if the translator has not been set yet.
    pub fn current() -> &'static AddressTranslator {
        #[cfg(not(any(test, feature = "software-emulation")))]
        {
            ADDRESS_TRANSLATOR.get().expect(
                "address translator not set; call AddressTranslator::set_current during initialization",
            )
        }

        #[cfg(any(test, feature = "software-emulation"))]
        {
            ADDRESS_TRANSLATOR.with(|t| {
                // SAFETY: We leak the reference to make it 'static. This is safe because:
                // 1. In test mode, each thread has its own ADDRESS_TRANSLATOR
                // 2. Once set, it's never modified (spin::Once guarantees this)
                // 3. The thread-local lives for the entire duration of the thread
                unsafe { &*(t.get().expect(
                    "address translator not set; call AddressTranslator::set_current during initialization",
                ) as *const AddressTranslator) }
            })
        }
    }

    /// Returns a reference to the current global address translator if it has been set.
    #[cfg(test)]
    pub fn try_current() -> Option<&'static AddressTranslator> {
        #[cfg(not(any(test, feature = "software-emulation")))]
        {
            ADDRESS_TRANSLATOR.get()
        }

        #[cfg(any(test, feature = "software-emulation"))]
        {
            ADDRESS_TRANSLATOR.with(|t| {
                t.get().map(|translator| {
                    // SAFETY: Same reasoning as current() - we leak the reference for 'static lifetime
                    unsafe { &*(translator as *const AddressTranslator) }
                })
            })
        }
    }

    /// Translates a physical address to a virtual address.
    pub fn phys_to_virt(&self, phys: usize) -> usize {
        match self {
            Self::Hardware { direct_map_offset } => phys.wrapping_add(*direct_map_offset),
            #[cfg(any(test, feature = "software-emulation"))]
            Self::Emulated(mem) => mem.translate(phys) as usize,
        }
    }

    /// Translates a virtual address to a physical address.
    pub fn virt_to_phys(&self, virt: usize) -> usize {
        match self {
            Self::Hardware { direct_map_offset } => virt.wrapping_sub(*direct_map_offset),
            #[cfg(any(test, feature = "software-emulation"))]
            Self::Emulated(mem) => mem.ptr_to_phys(virt as *const u8),
        }
    }

    /// Translates a physical address to a typed pointer.
    pub fn phys_to_ptr<T>(&self, phys: usize) -> *mut T {
        self.phys_to_virt(phys) as *mut T
    }

    /// Translates a pointer to a physical address.
    pub fn ptr_to_phys<T>(&self, ptr: *const T) -> usize {
        self.virt_to_phys(ptr as usize)
    }

    /// Allocates memory from the emulated space (test mode only).
    ///
    /// Returns the physical address of the allocated block, or None if
    /// there's not enough space.
    #[cfg(any(test, feature = "software-emulation"))]
    pub fn allocate(&self, size: usize, align: usize) -> Option<usize> {
        match self {
            Self::Hardware { .. } => {
                panic!("cannot allocate from hardware translator")
            }
            Self::Emulated(mem) => mem.allocate(size, align),
        }
    }
}

/// Global address translator.
///
/// This is initialized once during kernel initialization (with Hardware variant).
/// In test/software-emulation mode, this is thread-local to allow each test to have its own
/// emulated memory space.
#[cfg(not(any(test, feature = "software-emulation")))]
static ADDRESS_TRANSLATOR: spin::Once<AddressTranslator> = spin::Once::new();

#[cfg(any(test, feature = "software-emulation"))]
std::thread_local! {
    static ADDRESS_TRANSLATOR: spin::Once<AddressTranslator> = spin::Once::new();
}

/// Macro to define common address type functionality.
///
/// This macro generates the basic structure and methods common to both physical
/// and virtual address types, reducing code duplication.
macro_rules! impl_address_common {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name(usize);

        impl $name {
            /// Creates a new address without validation.
            ///
            /// # Safety
            ///
            /// The caller must ensure that the address is valid for the current architecture.
            #[inline]
            pub const unsafe fn new_unchecked(addr: usize) -> Self {
                Self(addr)
            }

            /// Creates an address from a pointer.
            #[inline]
            pub fn from_ptr<T>(ptr: *const T) -> Self {
                // SAFETY: In emulated mode, we bypass validation because host pointers
                // aren't canonical for the guest architecture.
                #[cfg(any(test, feature = "software-emulation"))]
                if let Some(translator) = AddressTranslator::try_current() {
                    if matches!(translator, AddressTranslator::Emulated(_)) {
                        return unsafe { Self::new_unchecked(ptr as usize) };
                    }
                }

                Self::new(ptr as usize)
            }

            /// Returns the raw address value.
            #[inline]
            pub const fn as_usize(self) -> usize {
                self.0
            }

            /// Checks if the address is aligned to the given alignment.
            ///
            /// # Panics
            ///
            /// Panics if `align` is not a power of two.
            #[inline]
            pub const fn is_aligned(self, align: usize) -> bool {
                assert!(align.is_power_of_two(), "alignment must be a power of two");
                self.0 & (align - 1) == 0
            }

            /// Aligns the address down to the given alignment.
            ///
            /// # Panics
            ///
            /// Panics if `align` is not a power of two.
            #[inline]
            pub const fn align_down(self, align: usize) -> Self {
                assert!(align.is_power_of_two(), "alignment must be a power of two");
                Self(self.0 & !(align - 1))
            }

            /// Aligns the address up to the given alignment.
            ///
            /// # Panics
            ///
            /// Panics if `align` is not a power of two.
            #[inline]
            pub const fn align_up(self, align: usize) -> Self {
                assert!(align.is_power_of_two(), "alignment must be a power of two");
                Self((self.0 + align - 1) & !(align - 1))
            }
        }

        impl fmt::Pointer for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:p}", self.0 as *const u8)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:#x})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:#x}", self.0)
            }
        }

        #[cfg(target_pointer_width = "64")]
        impl From<u64> for $name {
            #[inline]
            fn from(addr: u64) -> Self {
                Self::new(addr as usize)
            }
        }

        #[cfg(target_pointer_width = "32")]
        impl From<u32> for $name {
            #[inline]
            fn from(addr: u32) -> Self {
                Self::new(addr as usize)
            }
        }

        impl From<usize> for $name {
            #[inline]
            fn from(addr: usize) -> Self {
                Self::new(addr)
            }
        }

        impl Add<usize> for $name {
            type Output = Self;

            #[inline]
            fn add(self, rhs: usize) -> Self::Output {
                Self::new(self.0 + rhs)
            }
        }

        impl Sub<usize> for $name {
            type Output = Self;

            #[inline]
            fn sub(self, rhs: usize) -> Self::Output {
                Self::new(self.0 - rhs)
            }
        }

        impl Sub<$name> for $name {
            type Output = usize;

            #[inline]
            fn sub(self, rhs: $name) -> Self::Output {
                self.0 - rhs.0
            }
        }
    };
}

impl_address_common!(
    PhysicalAddress,
    "A physical memory address.\n\n\
     This is a newtype wrapper around the architecture-dependent representation of a\n\
     physical address. It provides methods for address manipulation and alignment checks."
);

impl PhysicalAddress {
    /// Creates a new physical address.
    ///
    /// # Panics
    ///
    /// Panics if the address exceeds the architecture's maximum physical address width.
    #[inline]
    pub const fn new(addr: usize) -> Self {
        assert!(
            crate::arch::validate_physical(addr),
            "physical address exceeds maximum width"
        );
        Self(addr)
    }

    /// Converts a direct-mapped virtual address back to a physical address.
    ///
    /// This assumes the virtual address is within the kernel's direct mapping region.
    ///
    /// # Panics
    ///
    /// Panics if the address translator has not been set via [`AddressTranslator::set_current`].
    #[inline]
    pub fn from_direct_mapped(virt: VirtualAddress) -> Self {
        let translator = AddressTranslator::current();
        Self::new(translator.virt_to_phys(virt.as_usize()))
    }

    /// Returns the corresponding frame number for this physical address.
    #[inline]
    pub fn frame_number(self) -> FrameNumber {
        FrameNumber::from(self)
    }
}

impl_address_common!(
    VirtualAddress,
    "A virtual memory address.\n\n\
     This is a newtype wrapper around the architecture-dependent representation of a\n\
     virtual address. It provides methods for address manipulation, alignment checks,\n\
     and extracting page table indices."
);

impl VirtualAddress {
    /// Creates a new virtual address.
    ///
    /// # Panics
    ///
    /// Panics if the address is not canonical for the architecture.
    #[inline]
    pub const fn new(addr: usize) -> Self {
        assert!(
            crate::arch::validate_virtual(addr),
            "address is not canonical"
        );
        Self(addr)
    }

    /// Creates a virtual address from a physical address using the direct map offset.
    ///
    /// This assumes the kernel has set up a direct mapping of all physical memory
    /// at a fixed offset in the virtual address space.
    ///
    /// # Panics
    ///
    /// Panics if the address translator has not been set via [`AddressTranslator::set_current`].
    #[inline]
    pub fn direct_mapped(phys: PhysicalAddress) -> Self {
        let translator = AddressTranslator::current();
        let virt = translator.phys_to_virt(phys.as_usize());

        // In emulated mode, phys_to_virt returns a host pointer which isn't canonical
        // for the guest architecture. Bypass the validity check in that case.
        #[cfg(any(test, feature = "software-emulation"))]
        if matches!(translator, AddressTranslator::Emulated(_)) {
            return Self(virt);
        }

        Self::new(virt)
    }

    /// Returns true if this virtual address is in the direct-mapped region.
    ///
    /// A virtual address is considered direct-mapped if it is greater than or equal
    /// to the direct map offset. Returns false if the address translator has not been set
    /// or if using emulated memory.
    #[inline]
    pub fn is_direct_mapped(self) -> bool {
        #[cfg(not(any(test, feature = "software-emulation")))]
        {
            ADDRESS_TRANSLATOR
                .get()
                .and_then(|translator| match translator {
                    AddressTranslator::Hardware { direct_map_offset } => {
                        Some(self.0 >= *direct_map_offset)
                    }
                })
                .unwrap_or(false)
        }

        #[cfg(any(test, feature = "software-emulation"))]
        {
            ADDRESS_TRANSLATOR.with(|t| {
                t.get()
                    .and_then(|translator| match translator {
                        AddressTranslator::Hardware { direct_map_offset } => {
                            Some(self.0 >= *direct_map_offset)
                        }
                        AddressTranslator::Emulated(_) => Some(true),
                    })
                    .unwrap_or(false)
            })
        }
    }

    /// Converts the address to a pointer.
    #[inline]
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// Converts the address to a mutable pointer.
    #[inline]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    /// Returns the page offset within a page (bits 0-11 for 4 KiB pages).
    ///
    /// # Parameters
    ///
    /// * `page_size` - The page size in bytes. Must be a power of two.
    ///
    /// # Panics
    ///
    /// Panics if `page_size` is not a power of two.
    #[inline]
    pub const fn page_offset(self) -> usize {
        self.0 & (arch::PAGE_SIZE - 1)
    }

    /// Returns the page table index at the specified level.
    ///
    /// Page table levels are numbered from 0 (the lowest level, closest to the page)
    /// upward. The exact meaning of each level is architecture-dependent:
    ///
    /// # Parameters
    ///
    /// * `level` - The page table level (0 = lowest/page table, higher = more significant)
    ///
    /// # Returns
    ///
    /// The index into the page table at the specified level (0-511 for 9-bit indices).
    ///
    /// # Panics
    ///
    /// Panics if `page_size` is not a power of two or if `level` is too high for the address space.
    #[inline]
    pub const fn page_index(self, level: usize) -> usize {
        arch::page_index(self.0, level)
    }

    /// Gets the corresponding page number for this virtual address.
    #[inline]
    pub fn page_number(self) -> crate::PageNumber {
        crate::PageNumber::from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PhysicalAddress tests
    mod physical_address {
        use super::*;

        #[test]
        fn new_valid_address() {
            let addr = PhysicalAddress::new(0x0100);
            assert_eq!(addr.as_usize(), 0x0100);
        }

        #[test]
        fn new_zero_address() {
            let addr = PhysicalAddress::new(0);
            assert_eq!(addr.as_usize(), 0);
        }

        #[test]
        fn new_max_valid_address() {
            // Use architecture-specific maximum
            let max_addr = (1usize << arch::MAX_PHYSICAL_BITS) - 1;
            let addr = PhysicalAddress::new(max_addr);
            assert_eq!(addr.as_usize(), max_addr as usize);
        }

        #[test]
        #[should_panic(expected = "physical address exceeds maximum width")]
        fn new_exceeds_max() {
            // Beyond architecture's maximum
            PhysicalAddress::new(1usize << arch::MAX_PHYSICAL_BITS);
        }

        #[test]
        fn alignment_check() {
            let addr = PhysicalAddress::new(arch::PAGE_SIZE * 4);
            assert!(addr.is_aligned(arch::PAGE_SIZE));
            assert!(addr.is_aligned(arch::PAGE_SIZE / 4));
            assert!(addr.is_aligned(1));
            assert!(!addr.is_aligned(arch::PAGE_SIZE * 8));
        }

        #[test]
        fn align_down() {
            let addr = PhysicalAddress::new(0x0124);
            assert_eq!(
                addr.align_down(arch::PAGE_SIZE),
                PhysicalAddress::new(0x0120)
            );
            assert_eq!(
                addr.align_down(arch::PAGE_SIZE / 4),
                PhysicalAddress::new(0x0124)
            ); // 0x124 is already 4-byte aligned
            assert_eq!(addr.align_down(2), PhysicalAddress::new(0x0124)); // Already 2-byte aligned
        }

        #[test]
        fn align_up() {
            let addr = PhysicalAddress::new(0x0124);
            assert_eq!(addr.align_up(arch::PAGE_SIZE), PhysicalAddress::new(0x0130));
            assert_eq!(
                addr.align_up(arch::PAGE_SIZE / 4),
                PhysicalAddress::new(0x0124)
            );
            assert_eq!(
                addr.align_up(arch::PAGE_SIZE / 16),
                PhysicalAddress::new(0x0124)
            );
        }

        #[test]
        fn align_already_aligned() {
            let addr = PhysicalAddress::new(arch::PAGE_SIZE * 2);
            assert_eq!(addr.align_down(arch::PAGE_SIZE), addr);
            assert_eq!(addr.align_up(arch::PAGE_SIZE), addr);
        }

        #[test]
        fn add_operator() {
            let addr = PhysicalAddress::new(0x0100);
            let result = addr + 0x50;
            assert_eq!(result.as_usize(), 0x0150);
        }

        #[test]
        fn sub_offset_operator() {
            let addr = PhysicalAddress::new(0x0150);
            let result = addr - 0x50;
            assert_eq!(result.as_usize(), 0x0100);
        }

        #[test]
        fn sub_address_operator() {
            let addr1 = PhysicalAddress::new(0x0150);
            let addr2 = PhysicalAddress::new(0x0100);
            let diff = addr1 - addr2;
            assert_eq!(diff, 0x50);
        }

        #[test]
        fn comparison_operators() {
            let addr1 = PhysicalAddress::new(0x0100);
            let addr2 = PhysicalAddress::new(0x0200);
            let addr3 = PhysicalAddress::new(0x0100);

            assert!(addr1 < addr2);
            assert!(addr2 > addr1);
            assert_eq!(addr1, addr3);
            assert_ne!(addr1, addr2);
        }

        #[test]
        fn as_usize() {
            let addr = PhysicalAddress::new(0x1234);
            assert_eq!(addr.as_usize(), 0x1234);
        }

        #[test]
        fn debug_format() {
            let addr = PhysicalAddress::new(0x0100);
            let debug_str = format!("{:?}", addr);
            assert!(debug_str.contains("PhysicalAddress"));
            assert!(debug_str.contains("0x100") || debug_str.contains("0x0100"));
        }

        #[test]
        fn display_format() {
            let addr = PhysicalAddress::new(0x0100);
            let display_str = format!("{}", addr);
            assert!(display_str.contains("0x100") || display_str.contains("0x0100"));
        }
    }

    // VirtualAddress tests
    mod virtual_address {
        use super::*;

        #[test]
        fn new_valid_lower_half() {
            // For 16-bit: 0x0000-0x7FFF is lower half
            let addr = VirtualAddress::new(0x7FFF);
            assert_eq!(addr.as_usize(), 0x7FFF);
        }

        #[test]
        fn new_valid_upper_half() {
            // For 16-bit: 0x8000 with sign extension = 0xFFFF_FFFF_FFFF_8000
            let addr = VirtualAddress::new(0xFFFF_FFFF_FFFF_8000);
            assert_eq!(addr.as_usize(), 0xFFFF_FFFF_FFFF_8000);
        }

        #[test]
        fn new_zero_address() {
            let addr = VirtualAddress::new(0);
            assert_eq!(addr.as_usize(), 0);
        }

        #[test]
        #[should_panic(expected = "address is not canonical")]
        fn new_non_canonical_low() {
            // For 16-bit: 0x8000 without sign extension is non-canonical
            VirtualAddress::new(0x8000);
        }

        #[test]
        #[should_panic(expected = "address is not canonical")]
        fn new_non_canonical_high() {
            // For 16-bit: 0x7FFF with extra bits set is non-canonical
            VirtualAddress::new(0xFFFF_FFFF_FFFF_7FFF);
        }

        #[test]
        fn alignment() {
            let addr = VirtualAddress::new(arch::PAGE_SIZE * 2);
            assert!(addr.is_aligned(arch::PAGE_SIZE));
            assert!(!addr.is_aligned(arch::PAGE_SIZE * 4));
        }

        #[test]
        fn align_down() {
            let addr = VirtualAddress::new(0x0124);
            assert_eq!(
                addr.align_down(arch::PAGE_SIZE),
                VirtualAddress::new(0x0120)
            );
        }

        #[test]
        fn align_up() {
            let addr = VirtualAddress::new(0x0124);
            assert_eq!(addr.align_up(arch::PAGE_SIZE), VirtualAddress::new(0x0130));
        }

        #[test]
        fn page_offset() {
            let addr = VirtualAddress::new(0x0124);
            assert_eq!(addr.page_offset(), 0x4);
        }

        #[test]
        fn page_offset_at_boundary() {
            let addr = VirtualAddress::new(arch::PAGE_SIZE);
            assert_eq!(addr.page_offset(), 0);
        }

        #[test]
        fn page_index_4k_pages() {
            // Software emulation: 16-byte pages, 4-bit indices
            // Address: 0x1234
            // Bits 0-3: page offset = 0x4
            // Bits 4-7: level 0 = 0x3
            // Bits 8-11: level 1 = 0x2
            // Bits 12-15: level 2 = 0x1
            let addr = VirtualAddress::new(0x1234);

            assert_eq!(addr.page_offset(), 0x4);
            assert_eq!(addr.page_index(0), 0x3);
            assert_eq!(addr.page_index(1), 0x2);
            assert_eq!(addr.page_index(2), 0x1);
        }

        #[test]
        fn page_index_different_levels() {
            // Software emulation: set all bits at level 0
            let addr = VirtualAddress::new(0x00F0); // bits 4-7 all set

            assert_eq!(addr.page_index(0), 0xF); // All 4 bits set at level 0
            assert_eq!(addr.page_index(1), 0); // Level 1 should be 0
        }

        #[test]
        fn add_operator() {
            let addr = VirtualAddress::new(0x0100);
            let result = addr + 0x50;
            assert_eq!(result.as_usize(), 0x0150);
        }

        #[test]
        fn sub_offset_operator() {
            let addr = VirtualAddress::new(0x0150);
            let result = addr - 0x50;
            assert_eq!(result.as_usize(), 0x0100);
        }

        #[test]
        fn sub_address_operator() {
            let addr1 = VirtualAddress::new(0x0150);
            let addr2 = VirtualAddress::new(0x0100);
            let diff = addr1 - addr2;
            assert_eq!(diff, 0x50);
        }

        #[test]
        fn pointer_conversion() {
            let addr = VirtualAddress::new(0x0100);
            let ptr: *const u8 = addr.as_ptr();
            assert_eq!(ptr as usize, 0x0100);

            let mut_ptr: *mut u8 = addr.as_mut_ptr();
            assert_eq!(mut_ptr as usize, 0x0100);
        }

        #[test]
        fn comparison_operators() {
            let addr1 = VirtualAddress::new(0x0100);
            let addr2 = VirtualAddress::new(0x0200);
            let addr3 = VirtualAddress::new(0x0100);

            assert!(addr1 < addr2);
            assert!(addr2 > addr1);
            assert_eq!(addr1, addr3);
            assert_ne!(addr1, addr2);
        }

        #[test]
        fn upper_half_addresses() {
            // Upper half: 0x8000 sign-extended
            let addr = VirtualAddress::new(0xFFFF_FFFF_FFFF_8000);
            assert_eq!(addr.page_offset(), 0);

            let addr_with_offset = VirtualAddress::new(0xFFFF_FFFF_FFFF_8004);
            assert_eq!(addr_with_offset.page_offset(), 0x4);
        }

        #[test]
        fn as_usize() {
            let addr = VirtualAddress::new(0x1234);
            assert_eq!(addr.as_usize(), 0x1234);
        }

        #[test]
        fn debug_format() {
            let addr = VirtualAddress::new(0x0100);
            let debug_str = format!("{:?}", addr);
            assert!(debug_str.contains("VirtualAddress"));
            assert!(debug_str.contains("0x100") || debug_str.contains("0x0100"));
        }

        #[test]
        fn display_format() {
            let addr = VirtualAddress::new(0x0100);
            let display_str = format!("{}", addr);
            assert!(display_str.contains("0x100") || display_str.contains("0x0100"));
        }
    }

    // Direct mapping tests
    mod direct_mapping {
        use super::*;

        fn setup_offset() {
            // With thread-local storage, we just need to ensure it's set for this thread
            if AddressTranslator::try_current().is_none() {
                // For 16-bit: use 0xFFFF_FFFF_FFFF_8000 as direct mapping base
                AddressTranslator::set_current(AddressTranslator::hardware(0xFFFF_FFFF_FFFF_8000));
            }
        }

        #[test]
        fn physical_to_virtual_conversion() {
            setup_offset();
            let phys = PhysicalAddress::new(0x0100);
            let virt = VirtualAddress::direct_mapped(phys);
            assert_eq!(virt.as_usize(), 0xFFFF_FFFF_FFFF_8100);
        }

        #[test]
        fn virtual_to_physical_conversion() {
            setup_offset();
            let virt = VirtualAddress::new(0xFFFF_FFFF_FFFF_8100);
            let phys = PhysicalAddress::from_direct_mapped(virt);
            assert_eq!(phys.as_usize(), 0x0100);
        }

        #[test]
        fn round_trip_conversion() {
            setup_offset();
            let original_phys = PhysicalAddress::new(0x1234);
            let virt = VirtualAddress::direct_mapped(original_phys);
            let recovered_phys = PhysicalAddress::from_direct_mapped(virt);
            assert_eq!(original_phys, recovered_phys);
        }

        #[test]
        fn is_direct_mapped_true() {
            setup_offset();
            let virt = VirtualAddress::new(0xFFFF_FFFF_FFFF_8100);
            assert!(virt.is_direct_mapped());
        }

        #[test]
        fn is_direct_mapped_false() {
            setup_offset();
            let virt = VirtualAddress::new(0x0100);
            assert!(!virt.is_direct_mapped());
        }

        #[test]
        fn is_direct_mapped_at_boundary() {
            setup_offset();
            // Exactly at the offset boundary should be direct mapped
            let virt = VirtualAddress::new(0xFFFF_FFFF_FFFF_8000);
            assert!(virt.is_direct_mapped());
        }

        #[test]
        #[should_panic(expected = "address translator already set")]
        fn panics_on_double_set() {
            AddressTranslator::set_current(AddressTranslator::hardware(0xFFFF_FFFF_FFFF_8000));
            AddressTranslator::set_current(AddressTranslator::hardware(0xFFFF_FFFF_FFFF_9000)); // Should panic
        }
    }
}
