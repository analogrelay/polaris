//! Block-based physical memory allocator.
//!
//! This module provides a simple block allocator inspired by Linux's memblock allocator.
//! It maintains static arrays of memory regions to track both available and reserved
//! physical memory, allowing early kernel initialization without requiring dynamic allocation.

use core::alloc::{AllocError as CoreAllocError, Allocator, Layout};
use core::ptr::NonNull;

use crate::arch::PAGE_SIZE;
use crate::{PhysicalAddress, VirtualAddress};

/// Maximum number of memory regions that can be tracked.
const MAX_REGIONS: usize = 128;

/// Errors that can occur during memory allocation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// No suitable memory region available for allocation.
    OutOfMemory,
    /// The requested alignment is invalid (e.g., not a power of two).
    InvalidAlignment,
    /// The region arrays are full and cannot track more regions.
    RegionsFull,
    /// Memory region overlaps with existing region in an invalid way.
    RegionOverlap,
}

impl From<AllocError> for CoreAllocError {
    fn from(_: AllocError) -> Self {
        CoreAllocError
    }
}

/// A contiguous range of physical memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion {
    base: PhysicalAddress,
    size: usize,
}

impl MemoryRegion {
    /// Creates a new memory region.
    pub const fn new(base: PhysicalAddress, size: usize) -> Self {
        Self { base, size }
    }

    /// Returns the base address of this region.
    pub const fn base(&self) -> PhysicalAddress {
        self.base
    }

    /// Returns the size of this region in bytes.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns the end address (exclusive) of this region.
    pub const fn end(&self) -> PhysicalAddress {
        PhysicalAddress::new(self.base.as_usize() + self.size)
    }

    /// Returns true if this region overlaps with another region.
    pub const fn overlaps(&self, other: &MemoryRegion) -> bool {
        self.base.as_usize() < other.end().as_usize()
            && other.base.as_usize() < self.end().as_usize()
    }

    /// Returns true if this region is adjacent to another region.
    pub const fn adjacent(&self, other: &MemoryRegion) -> bool {
        self.end().as_usize() == other.base.as_usize()
            || other.end().as_usize() == self.base.as_usize()
    }

    /// Returns true if this region can be merged with another region.
    pub const fn mergeable(&self, other: &MemoryRegion) -> bool {
        self.overlaps(other) || self.adjacent(other)
    }

    /// Merges this region with another, returning a new region spanning both.
    /// The regions must be mergeable.
    pub const fn merge(&self, other: &MemoryRegion) -> MemoryRegion {
        let base = if self.base.as_usize() < other.base.as_usize() {
            self.base
        } else {
            other.base
        };
        let end = if self.end().as_usize() > other.end().as_usize() {
            self.end()
        } else {
            other.end()
        };
        MemoryRegion::new(base, end.as_usize() - base.as_usize())
    }

    /// Returns true if this region contains the given address range.
    pub const fn contains(&self, base: PhysicalAddress, size: usize) -> bool {
        base.as_usize() >= self.base.as_usize() && base.as_usize() + size <= self.end().as_usize()
    }
}

/// Fixed-size array of memory regions with static allocation.
#[derive(Debug)]
struct RegionArray {
    regions: [Option<MemoryRegion>; MAX_REGIONS],
    count: usize,
}

impl RegionArray {
    /// Creates a new empty region array.
    const fn new() -> Self {
        Self {
            regions: [None; MAX_REGIONS],
            count: 0,
        }
    }

    /// Returns the number of regions in the array.
    const fn len(&self) -> usize {
        self.count
    }

    /// Returns true if the array is empty.
    const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns true if the array is full.
    const fn is_full(&self) -> bool {
        self.count >= MAX_REGIONS
    }

    /// Returns an iterator over the regions.
    fn iter(&self) -> impl Iterator<Item = &MemoryRegion> {
        self.regions[..self.count].iter().filter_map(|r| r.as_ref())
    }

    /// Inserts a region at the specified index, shifting subsequent regions.
    fn insert(&mut self, index: usize, region: MemoryRegion) -> Result<(), AllocError> {
        if self.is_full() {
            return Err(AllocError::RegionsFull);
        }
        if index > self.count {
            return Err(AllocError::RegionsFull);
        }

        // Shift regions to make space
        for i in (index..self.count).rev() {
            self.regions[i + 1] = self.regions[i];
        }

        self.regions[index] = Some(region);
        self.count += 1;
        Ok(())
    }

    /// Removes the region at the specified index, shifting subsequent regions.
    fn remove(&mut self, index: usize) {
        if index >= self.count {
            return;
        }

        // Shift regions to fill the gap
        for i in index..self.count - 1 {
            self.regions[i] = self.regions[i + 1];
        }

        self.regions[self.count - 1] = None;
        self.count -= 1;
    }

    /// Adds a region to the array, maintaining sorted order by base address.
    /// Automatically merges with adjacent or overlapping regions.
    fn add(&mut self, region: MemoryRegion) -> Result<(), AllocError> {
        if region.size() == 0 {
            return Ok(());
        }

        // Find insertion point and check for merges
        let mut insert_pos = self.count;
        let mut merged_region = region;
        let mut merge_start = None;
        let mut merge_end = None;

        for (i, existing) in self.iter().enumerate() {
            if merged_region.base() > existing.end() {
                // New region comes after this one
                continue;
            } else if merged_region.end() < existing.base() {
                // New region comes before this one
                if insert_pos == self.count {
                    insert_pos = i;
                }
                break;
            } else {
                // Regions overlap or are adjacent - merge them
                merged_region = merged_region.merge(existing);
                if merge_start.is_none() {
                    merge_start = Some(i);
                }
                merge_end = Some(i);
            }
        }

        // Remove merged regions and insert the combined region
        if let Some(start) = merge_start {
            let end = merge_end.unwrap();
            for _ in start..=end {
                self.remove(start);
            }
            insert_pos = start;
        }

        self.insert(insert_pos, merged_region)
    }

    /// Removes a region from the array, potentially splitting existing regions.
    fn subtract(&mut self, region: MemoryRegion) -> Result<(), AllocError> {
        if region.size() == 0 {
            return Ok(());
        }

        let mut i = 0;
        while i < self.count {
            let existing = self.regions[i].unwrap();

            if !existing.overlaps(&region) {
                i += 1;
                continue;
            }

            // Region overlaps - need to handle it
            self.remove(i);

            // Add back the parts that don't overlap
            if existing.base() < region.base() {
                // Part before the removed region
                let before = MemoryRegion::new(
                    existing.base(),
                    region.base().as_usize() - existing.base().as_usize(),
                );
                self.insert(i, before)?;
                i += 1;
            }

            if existing.end() > region.end() {
                // Part after the removed region
                let after = MemoryRegion::new(
                    region.end(),
                    existing.end().as_usize() - region.end().as_usize(),
                );
                self.insert(i, after)?;
                i += 1;
            }
        }

        Ok(())
    }

    /// Calculates the total size of all regions.
    fn total_size(&self) -> usize {
        self.iter().map(|r| r.size()).sum()
    }
}

/// A block-based physical memory allocator with static accounting space.
///
/// This allocator maintains two lists of memory regions:
/// - Memory regions: all physical memory in the system
/// - Reserved regions: memory that is reserved or allocated
///
/// Free memory is implicitly defined as memory in the memory list but not in the reserved list.
///
/// # Thread Safety
///
/// The allocator uses `spin::Mutex` for interior mutability to allow the `Allocator` trait
/// (which requires `&self`) to modify internal state. This provides thread safety with minimal
/// overhead. Contention should be minimal until secondary processors are launched.
pub struct BlockAllocator {
    /// All memory regions in the system (usable + reserved).
    memory: spin::Mutex<RegionArray>,
    /// Regions that are reserved or allocated.
    reserved: spin::Mutex<RegionArray>,
}

impl BlockAllocator {
    /// Creates a new empty block allocator.
    pub const fn new() -> Self {
        Self {
            memory: spin::Mutex::new(RegionArray::new()),
            reserved: spin::Mutex::new(RegionArray::new()),
        }
    }

    /// Adds a usable memory region to the allocator.
    ///
    /// The region's base address and size will be aligned to page boundaries.
    /// Adjacent or overlapping regions will be automatically merged.
    pub fn add(&mut self, base: PhysicalAddress, size: usize) -> Result<(), AllocError> {
        // Align base up to page boundary
        let aligned_base =
            PhysicalAddress::new((base.as_usize() + PAGE_SIZE - 1) & !(PAGE_SIZE - 1));

        // Adjust size to account for alignment and round down to page boundary
        let end = base.as_usize() + size;
        if end <= aligned_base.as_usize() {
            // Region is too small after alignment
            return Ok(());
        }
        let aligned_size = (end - aligned_base.as_usize()) & !(PAGE_SIZE - 1);

        if aligned_size == 0 {
            return Ok(());
        }

        let region = MemoryRegion::new(aligned_base, aligned_size);
        self.memory.lock().add(region)
    }

    /// Reserves a memory region, marking it as unavailable for allocation.
    ///
    /// This is used to mark bootloader structures, kernel code/data, and other
    /// reserved regions that should not be allocated.
    pub fn reserve(&mut self, base: PhysicalAddress, size: usize) -> Result<(), AllocError> {
        if size == 0 {
            return Ok(());
        }

        // Align base down to page boundary
        let aligned_base = PhysicalAddress::new(base.as_usize() & !(PAGE_SIZE - 1));

        // Align size up to page boundary
        let end = base.as_usize() + size;
        let aligned_end = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let aligned_size = aligned_end - aligned_base.as_usize();

        let region = MemoryRegion::new(aligned_base, aligned_size);
        self.reserved.lock().add(region)
    }

    /// Allocates physical memory from the available regions, returning a direct-mapped virtual address.
    ///
    /// Uses a first-fit algorithm to find a suitable region. The returned virtual address
    /// is the direct-mapped address corresponding to the allocated physical memory.
    /// The address will be aligned to the specified alignment (which must be a power of two).
    ///
    /// # Panics
    ///
    /// Panics if the direct map offset has not been set via [`crate::set_direct_map_offset`].
    pub fn allocate_raw(
        &mut self,
        size: usize,
        align: usize,
    ) -> Result<VirtualAddress, AllocError> {
        if size == 0 {
            return Err(AllocError::OutOfMemory);
        }

        // Validate alignment is a power of two
        if align == 0 || (align & (align - 1)) != 0 {
            return Err(AllocError::InvalidAlignment);
        }

        // Align size up to page boundary
        let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        // Search for a suitable free region
        let memory = self.memory.lock();
        let mut reserved = self.reserved.lock();

        for mem_region in memory.iter() {
            // Check each potential allocation within this memory region
            let mut current = mem_region.base().as_usize();
            let end = mem_region.end().as_usize();

            while current + aligned_size <= end {
                // Align current address
                let aligned_current = (current + align - 1) & !(align - 1);

                if aligned_current + aligned_size > end {
                    break;
                }

                let candidate = PhysicalAddress::new(aligned_current);
                let candidate_region = MemoryRegion::new(candidate, aligned_size);

                // Check if this candidate overlaps with any reserved region
                let mut is_free = true;
                for reserved_region in reserved.iter() {
                    if candidate_region.overlaps(reserved_region) {
                        is_free = false;
                        // Jump past this reserved region
                        current = reserved_region.end().as_usize();
                        break;
                    }
                }

                if is_free {
                    // Found a suitable region - reserve it and return the direct-mapped virtual address
                    reserved.add(candidate_region)?;
                    return Ok(VirtualAddress::direct_mapped(candidate));
                }
            }
        }

        Err(AllocError::OutOfMemory)
    }

    /// Frees a previously allocated memory region.
    ///
    /// The region will be removed from the reserved list, making it available
    /// for future allocations. The address should be the direct-mapped virtual
    /// address returned by [`allocate`](Self::allocate).
    ///
    /// # Panics
    ///
    /// Panics if the direct map offset has not been set via [`crate::set_direct_map_offset`].
    pub fn free(&mut self, base: VirtualAddress, size: usize) -> Result<(), AllocError> {
        if size == 0 {
            return Ok(());
        }

        // Convert back to physical address
        let phys_base = PhysicalAddress::from_direct_mapped(base);

        // Align to page boundaries (same as reserve)
        let aligned_base = PhysicalAddress::new(phys_base.as_usize() & !(PAGE_SIZE - 1));
        let end = phys_base.as_usize() + size;
        let aligned_end = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let aligned_size = aligned_end - aligned_base.as_usize();

        let region = MemoryRegion::new(aligned_base, aligned_size);
        self.reserved.lock().subtract(region)
    }

    /// Returns the total amount of physical memory tracked by the allocator.
    pub fn total_memory(&self) -> usize {
        self.memory.lock().total_size()
    }

    /// Returns the total amount of reserved or allocated memory.
    pub fn reserved_memory(&self) -> usize {
        self.reserved.lock().total_size()
    }

    /// Returns the total amount of available (free) memory.
    pub fn available_memory(&self) -> usize {
        self.total_memory().saturating_sub(self.reserved_memory())
    }
}

impl Default for BlockAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Allocator for BlockAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, CoreAllocError> {
        let size = layout.size();
        let align = layout.align();

        // Validate alignment is a power of two
        if align == 0 || (align & (align - 1)) != 0 {
            log::error!("invalid alignment requested: {}", align);
            return Err(CoreAllocError);
        }

        // Align size up to page boundary
        let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if aligned_size == 0 {
            log::error!("zero-size allocation requested");
            return Err(CoreAllocError);
        }

        // Search for a suitable free region
        let memory = self.memory.lock();
        let mut reserved = self.reserved.lock();

        for mem_region in memory.iter() {
            let mut current = mem_region.base().as_usize();
            let end = mem_region.end().as_usize();

            while current + aligned_size <= end {
                let aligned_current = (current + align - 1) & !(align - 1);

                if aligned_current + aligned_size > end {
                    break;
                }

                let candidate = PhysicalAddress::new(aligned_current);
                let candidate_region = MemoryRegion::new(candidate, aligned_size);

                let mut is_free = true;
                for reserved_region in reserved.iter() {
                    if candidate_region.overlaps(reserved_region) {
                        is_free = false;
                        current = reserved_region.end().as_usize();
                        break;
                    }
                }

                if is_free {
                    reserved.add(candidate_region).map_err(|_| CoreAllocError)?;
                    let virt_addr = VirtualAddress::direct_mapped(candidate);
                    let ptr = virt_addr.as_mut_ptr::<u8>();
                    let slice = unsafe { core::slice::from_raw_parts_mut(ptr, size) };
                    return Ok(NonNull::from(slice));
                }
            }
        }

        log::error!("out of memory: failed to allocate {} bytes", size);
        Err(CoreAllocError)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let virt_addr = VirtualAddress::from_ptr(ptr.as_ptr());
        let size = layout.size();

        // Convert back to physical address
        let phys_base = PhysicalAddress::from_direct_mapped(virt_addr);

        // Align to page boundaries
        let aligned_base = PhysicalAddress::new(phys_base.as_usize() & !(PAGE_SIZE - 1));
        let end = phys_base.as_usize() + size;
        let aligned_end = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let aligned_size = aligned_end - aligned_base.as_usize();

        let region = MemoryRegion::new(aligned_base, aligned_size);
        let _ = self.reserved.lock().subtract(region);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sets up the direct map offset for testing.
    /// Uses a simple offset of 0xFFFF_FFFF_FFFF_8000 for 16-bit software emulation.
    fn setup_test_direct_map() {
        // With thread-local storage, we just need to ensure it's set for this thread
        if crate::AddressTranslator::try_current().is_none() {
            crate::AddressTranslator::set_current(crate::AddressTranslator::hardware(
                0xFFFF_FFFF_FFFF_8000,
            ));
        }
    }

    #[test]
    fn memory_region_operations() {
        let r1 = MemoryRegion::new(PhysicalAddress::new(0x0100), 0x0100);
        let r2 = MemoryRegion::new(PhysicalAddress::new(0x0200), 0x0100);
        let r3 = MemoryRegion::new(PhysicalAddress::new(0x0180), 0x0100);

        assert_eq!(r1.base().as_usize(), 0x0100);
        assert_eq!(r1.size(), 0x0100);
        assert_eq!(r1.end().as_usize(), 0x0200);

        assert!(!r1.overlaps(&r2));
        assert!(r1.adjacent(&r2));
        assert!(r1.mergeable(&r2));
        assert!(r1.overlaps(&r3));
        assert!(r1.mergeable(&r3));
    }

    #[test]
    fn memory_region_merge() {
        let r1 = MemoryRegion::new(PhysicalAddress::new(0x0100), 0x0100);
        let r2 = MemoryRegion::new(PhysicalAddress::new(0x0200), 0x0100);

        let merged = r1.merge(&r2);
        assert_eq!(merged.base().as_usize(), 0x0100);
        assert_eq!(merged.size(), 0x0200);
        assert_eq!(merged.end().as_usize(), 0x0300);
    }

    #[test]
    fn region_array_add_and_merge() {
        let mut array = RegionArray::new();

        array
            .add(MemoryRegion::new(PhysicalAddress::new(0x0200), 0x0100))
            .unwrap();
        assert_eq!(array.len(), 1);

        // Add adjacent region - should merge
        array
            .add(MemoryRegion::new(PhysicalAddress::new(0x0300), 0x0100))
            .unwrap();
        assert_eq!(array.len(), 1);
        assert_eq!(array.iter().next().unwrap().size(), 0x0200);

        // Add non-adjacent region
        array
            .add(MemoryRegion::new(PhysicalAddress::new(0x0500), 0x0100))
            .unwrap();
        assert_eq!(array.len(), 2);
    }

    #[test]
    fn region_array_subtract() {
        let mut array = RegionArray::new();
        array
            .add(MemoryRegion::new(PhysicalAddress::new(0x0100), 0x0300))
            .unwrap();

        // Remove middle section
        array
            .subtract(MemoryRegion::new(PhysicalAddress::new(0x0200), 0x0100))
            .unwrap();

        assert_eq!(array.len(), 2);
        let regions: Vec<_> = array.iter().collect();
        assert_eq!(regions[0].base().as_usize(), 0x0100);
        assert_eq!(regions[0].size(), 0x0100);
        assert_eq!(regions[1].base().as_usize(), 0x0300);
        assert_eq!(regions[1].size(), 0x0100);
    }

    #[test]
    fn allocator_starts_empty() {
        let allocator = BlockAllocator::new();
        assert_eq!(allocator.total_memory(), 0);
        assert_eq!(allocator.reserved_memory(), 0);
        assert_eq!(allocator.available_memory(), 0);
    }

    #[test]
    fn allocator_add_and_reserve() {
        let mut allocator = BlockAllocator::new();

        allocator.add(PhysicalAddress::new(0x0100), 0x1000).unwrap();
        assert_eq!(allocator.total_memory(), 0x1000);
        assert_eq!(allocator.available_memory(), 0x1000);

        allocator
            .reserve(PhysicalAddress::new(0x0200), 0x0100)
            .unwrap();
        assert_eq!(allocator.reserved_memory(), 0x0100);
        assert_eq!(allocator.available_memory(), 0x0f00);
    }

    #[test]
    fn allocator_alloc_and_free() {
        setup_test_direct_map();
        let mut allocator = BlockAllocator::new();

        allocator.add(PhysicalAddress::new(0x0100), 0x1000).unwrap();

        let addr = allocator.allocate_raw(0x0100, 0x0100).unwrap();
        // Check that the virtual address is in the direct map region
        assert!(addr.as_usize() >= 0xFFFF_FFFF_FFFF_8000);
        // Check page alignment
        assert_eq!(addr.as_usize() & 0xf, 0); // 16-byte alignment
        assert_eq!(allocator.reserved_memory(), 0x0100);

        allocator.free(addr, 0x0100).unwrap();
        assert_eq!(allocator.reserved_memory(), 0);
    }

    #[test]
    fn allocator_out_of_memory() {
        setup_test_direct_map();
        let mut allocator = BlockAllocator::new();

        allocator.add(PhysicalAddress::new(0x0100), 0x0200).unwrap();

        // Allocate all memory
        allocator.allocate_raw(0x0200, 0x0100).unwrap();

        // Try to allocate more - should fail
        assert_eq!(
            allocator.allocate_raw(0x0100, 0x0100),
            Err(AllocError::OutOfMemory)
        );
    }

    #[test]
    fn allocator_alignment() {
        setup_test_direct_map();
        let mut allocator = BlockAllocator::new();

        allocator.add(PhysicalAddress::new(0x0100), 0x1000).unwrap();

        let addr = allocator.allocate_raw(0x0100, 0x0400).unwrap();
        assert_eq!(addr.as_usize() & 0x03ff, 0); // Aligned to 1KB
    }
}
