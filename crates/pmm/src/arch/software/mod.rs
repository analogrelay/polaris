//! Software emulation implementation for testing and development.
//!
//! This module provides a software-emulated architecture that can run on any host.
//! It's designed for testing and development without requiring actual hardware access.
//!
//! The software-emulated architecture is a "scale model" of x86_64:
//! - 16-bit addresses (vs 48-bit on x86_64)
//! - 3 levels of page tables (vs 4 on x86_64)
//! - 4-bit indexes (16 entries per table, vs 9-bit/512 entries on x86_64)
//! - 4-bit page offset (16-byte pages, vs 12-bit/4KB on x86_64)
//!
//! This provides realistic paging behavior while keeping memory usage minimal for testing.

mod entry;
mod flags;
mod table;

pub use entry::PageEntry;
pub use flags::PageFlags;
pub use table::PageTable;

/// Maximum number of bits in a physical address for software emulation.
pub const MAX_PHYSICAL_BITS: usize = 16;

/// Maximum number of bits in a virtual address for software emulation.
pub const MAX_VIRTUAL_BITS: usize = 16;

/// Page size in bytes (16 bytes = 2^4).
pub const PAGE_SIZE: usize = 16;

/// Number of page table levels (3 levels: level 2, 1, 0).
pub const PAGE_TABLE_LEVELS: usize = 3;

/// Returns the page table index for a given virtual address at the specified level.
///
/// For software emulation:
/// - Level 0: Bits 4-7 (page table)
/// - Level 1: Bits 8-11 (page directory)
/// - Level 2: Bits 12-15 (root/page directory pointer)
#[inline]
pub const fn page_index(address: usize, level: usize) -> usize {
    let bits_for_level = match level {
        0 | 1 | 2 => 4,
        _ => panic!("level out of range for software emulation (0-2)"),
    };
    let shift = 4 + (level * bits_for_level);
    ((address >> shift) & ((1 << bits_for_level) - 1)) as usize
}

/// Validates a physical address for software emulation.
///
/// Physical addresses must fit within 16 bits.
#[inline]
pub const fn validate_physical(addr: usize) -> bool {
    addr <= 0xFFFF
}

/// Validates a virtual address for software emulation.
///
/// Virtual addresses must be canonical (bits 16-63 must be sign-extended from bit 15).
#[inline]
pub const fn validate_virtual(addr: usize) -> bool {
    let canonical = if (addr & 0x8000) != 0 {
        addr | 0xFFFF_FFFF_FFFF_0000
    } else {
        addr & 0xFFFF
    };
    canonical == addr
}

/// Canonicalizes a virtual address for software emulation.
///
/// This sign-extends bit 15 to bits 16-63.
#[inline]
pub const fn canonicalize_virtual(addr: usize) -> usize {
    if (addr & 0x8000) != 0 {
        addr | 0xFFFF_FFFF_FFFF_0000
    } else {
        addr & 0xFFFF
    }
}

/// Emulated memory for software simulation.
///
/// This provides a simulated physical memory space for testing page table operations
/// without requiring actual hardware or virtual memory support from the host OS.
pub struct EmulatedMemory {
    /// The underlying memory buffer.
    memory: Vec<u8>,
    /// Next allocation offset (simple bump allocator).
    next_alloc: core::sync::atomic::AtomicUsize,
}

impl EmulatedMemory {
    /// Creates a new emulated memory region of the specified size.
    pub fn new(size: usize) -> Self {
        Self {
            memory: alloc::vec![0u8; size],
            next_alloc: core::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Allocates a block of memory from the emulated space.
    ///
    /// Returns the physical address of the allocated block, or None if
    /// there's not enough space.
    pub fn allocate(&self, size: usize, align: usize) -> Option<usize> {
        use core::sync::atomic::Ordering;

        loop {
            let current = self.next_alloc.load(Ordering::Relaxed);

            // Align the current offset
            let aligned = (current + align - 1) & !(align - 1);
            let end = aligned + size;

            if end > self.memory.len() {
                return None;
            }

            // Try to claim this allocation
            if self
                .next_alloc
                .compare_exchange(current, end, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Some(aligned as usize);
            }
        }
    }

    /// Translates a physical address to a virtual address (pointer into the buffer).
    pub fn translate(&self, phys: usize) -> *mut u8 {
        assert!(phys < self.memory.len(), "physical address out of bounds");
        unsafe { self.memory.as_ptr().add(phys) as *mut u8 }
    }

    /// Translates a virtual address (pointer) back to a physical address.
    pub fn ptr_to_phys(&self, ptr: *const u8) -> usize {
        let offset = unsafe { ptr.offset_from(self.memory.as_ptr()) };
        assert!(offset >= 0, "pointer not within emulated memory");
        assert!(
            (offset as usize) < self.memory.len(),
            "pointer not within emulated memory"
        );
        offset as usize
    }

    /// Returns the size of the emulated memory region.
    pub fn size(&self) -> usize {
        self.memory.len()
    }
}
