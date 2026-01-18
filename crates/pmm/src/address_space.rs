//! Address space management.
//!
//! This module provides architecture-independent types for managing virtual address spaces,
//! which may belong to the kernel, user processes, or other contexts.

use crate::arch::PageTable;

/// An address space is an architecture-independent representation of a virtual address space.
///
/// Each address space owns a page table that maps virtual addresses to physical addresses.
/// Address spaces can belong to the kernel, user processes, or other contexts.
pub struct AddressSpace {
    /// The architecture-dependent page table for this address space.
    page_table: PageTable,
}

impl AddressSpace {
    /// Creates a new address space with a default page table.
    pub fn new() -> Self {
        Self {
            page_table: PageTable::default(),
        }
    }

    /// Returns a reference to the page table for this address space.
    pub fn page_table(&self) -> &PageTable {
        &self.page_table
    }

    /// Returns a mutable reference to the page table for this address space.
    pub fn page_table_mut(&mut self) -> &mut PageTable {
        &mut self.page_table
    }
}
