//! x86_64 architecture-specific implementation.
//!
//! This module provides the hardware implementation for x86_64 architecture,
//! including address validation, page table configuration, and low-level
//! page table primitives.

mod entry;
mod flags;
mod table;

pub use entry::PageEntry;
pub use flags::PageFlags;
pub use table::PageTable;

/// Maximum number of bits in a physical address on x86_64.
/// This is typically 52 bits on modern CPUs, but we use 48 as a conservative default.
pub const MAX_PHYSICAL_BITS: usize = 48;

/// Maximum number of bits in a virtual address on x86_64 with 4-level paging.
pub const MAX_VIRTUAL_BITS: usize = 48;

/// Default page size in bytes (4 KiB).
pub const PAGE_SIZE: usize = 4096;

/// Number of page table levels in x86_64 (4-level paging).
/// This can be 5 with 5-level paging, but 4 is the standard.
pub const PAGE_TABLE_LEVELS: usize = 4;

/// Returns the page table index for a given virtual address at the specified level.
///
/// For x86_64, each level uses 9 bits, with level 0 being the page table (PT),
/// level 1 being the page directory (PD), level 2 being the page directory pointer
/// table (PDPT), and level 3 being the page map level 4 (PML4).
#[inline]
pub const fn page_index(address: usize, level: usize) -> usize {
    let bits_for_level = match level {
        0 | 1 | 2 | 3 => 9,
        _ => panic!("level out of range for x86_64 page table levels"),
    };
    let shift = 12 + (level * bits_for_level);
    ((address >> shift) & ((1 << bits_for_level) - 1)) as usize
}

/// Validates a physical address for x86_64.
///
/// Physical addresses must not exceed the maximum physical address width.
#[inline]
pub const fn validate_physical(addr: usize) -> bool {
    let max_addr = (1usize << MAX_PHYSICAL_BITS) - 1;
    addr <= max_addr
}

/// Validates a virtual address for x86_64.
///
/// Virtual addresses must be canonical (bits 47-63 must be sign-extended from bit 47).
#[inline]
pub const fn validate_virtual(addr: usize) -> bool {
    let canonical = if (addr & (1 << 47)) != 0 {
        addr | 0xFFFF_0000_0000_0000
    } else {
        addr & 0x0000_FFFF_FFFF_FFFF
    };
    canonical == addr
}

/// Canonicalizes a virtual address for x86_64.
///
/// This sign-extends bit 47 to bits 48-63.
#[inline]
pub const fn canonicalize_virtual(addr: usize) -> usize {
    if (addr & (1 << 47)) != 0 {
        addr | 0xFFFF_0000_0000_0000
    } else {
        addr & 0x0000_FFFF_FFFF_FFFF
    }
}
