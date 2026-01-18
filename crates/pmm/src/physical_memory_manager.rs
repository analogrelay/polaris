//! Physical memory manager using a buddy allocator.
//!
//! This module provides the main physical memory allocator for the kernel, based on Linux's
//! buddy allocator design. It manages all physical frames in the system using an 11-order
//! buddy system (orders 0-11), where order n represents blocks of 2^n contiguous frames.

use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use crate::{FrameFlag, FrameNumber, MemoryMap, PhysicalAddress, arch};

use crate::VirtualAddress;

/// Maximum order supported by the buddy allocator (order 11 = 2048 frames = 8MB).
const MAX_ORDER: usize = 11;

/// Number of free lists in the buddy allocator (orders 0 through MAX_ORDER inclusive).
const NUM_FREE_LISTS: usize = MAX_ORDER + 1;

/// Errors that can occur during physical memory allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// No suitable memory block available for the requested order.
    OutOfMemory,
    /// The requested order exceeds MAX_ORDER.
    OrderTooLarge,
    /// Invalid alignment order (must be >= allocation order).
    InvalidAlignment,
    /// Attempted to deallocate an invalid address or order.
    InvalidDeallocation,
}

/// Node in an intrusive linked list for free blocks.
///
/// This structure is written directly into the physical frames that are free,
/// using the direct-mapped virtual addresses.
#[repr(C)]
struct FreeBlock {
    next: *mut FreeBlock,
}

/// Free list for a specific order.
struct FreeList {
    head: AtomicPtr<FreeBlock>,
    count: AtomicUsize,
}

impl FreeList {
    /// Creates an empty free list.
    const fn new() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
            count: AtomicUsize::new(0),
        }
    }

    /// Pushes a block onto the free list.
    fn push(&self, block: *mut FreeBlock) {
        loop {
            let head = self.head.load(Ordering::Acquire);
            unsafe {
                (*block).next = head;
            }
            if self
                .head
                .compare_exchange(head, block, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                self.count.fetch_add(1, Ordering::Release);
                return;
            }
        }
    }

    /// Pops a block from the free list, returning None if empty.
    fn pop(&self) -> Option<NonNull<FreeBlock>> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            if head.is_null() {
                return None;
            }
            let next = unsafe { (*head).next };
            if self
                .head
                .compare_exchange(head, next, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                self.count.fetch_sub(1, Ordering::Release);
                return NonNull::new(head);
            }
        }
    }

    /// Returns the number of blocks in this free list.
    fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// Returns true if this free list is empty.
    fn is_empty(&self) -> bool {
        self.count() == 0
    }
}

/// Physical memory manager using a buddy allocator.
///
/// Manages all physical memory frames in the system using an 11-order buddy allocation
/// scheme. Order n contains blocks of 2^n contiguous frames:
/// - Order 0: single 4KB frames
/// - Order 1: 8KB blocks (2 frames)
/// - ...
/// - Order 11: 8MB blocks (2048 frames)
///
/// Memory is allocated by finding a free block of the requested order, splitting larger
/// blocks if necessary. Memory is deallocated by returning blocks to free lists and
/// coalescing with buddy blocks when possible.
pub struct PhysicalMemoryManager {
    memory_map: MemoryMap,
    free_lists: [FreeList; NUM_FREE_LISTS],
    total_frames: usize,
}

impl PhysicalMemoryManager {
    /// Creates a new physical memory manager.
    ///
    /// The allocator takes ownership of the memory map and initializes all free lists as empty.
    /// Memory must be added to the allocator by deallocating regions into it.
    pub fn new(memory_map: MemoryMap) -> Self {
        let total_frames = memory_map.allocated_frame_count();

        Self {
            memory_map,
            free_lists: [
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
                FreeList::new(),
            ],
            total_frames,
        }
    }

    /// Allocates 2^order contiguous frames.
    ///
    /// Uses the buddy allocator splitting algorithm: if the requested order is not available,
    /// finds the next higher order with available blocks, splits it, and adds the buddy back
    /// to the appropriate free list.
    pub fn allocate(&mut self, order: usize) -> Result<PhysicalAddress, AllocError> {
        if order > MAX_ORDER {
            return Err(AllocError::OrderTooLarge);
        }

        // Try to find a free block at this order or higher
        let alloc_order = self.find_free_order(order)?;

        // Pop the block from the free list
        let block = self.free_lists[alloc_order]
            .pop()
            .ok_or(AllocError::OutOfMemory)?;
        let addr = self.block_to_address(block);

        // Split the block down to the requested order
        self.split_block(addr, alloc_order, order);

        // Mark the block as allocated
        let frame_num = addr.frame_number();
        if let Some(frame) = self.memory_map.frame_mut(frame_num) {
            frame.flags.set(FrameFlag::Allocated);
            frame.set_order(order as u8);
        }

        Ok(addr)
    }

    /// Allocates 2^order contiguous frames aligned to 2^align_order boundaries.
    ///
    /// The alignment order must be >= the allocation order. Useful for large page support,
    /// DMA requirements, or other hardware constraints.
    pub fn allocate_aligned(
        &mut self,
        order: usize,
        align_order: usize,
    ) -> Result<PhysicalAddress, AllocError> {
        if order > MAX_ORDER {
            return Err(AllocError::OrderTooLarge);
        }
        if align_order < order {
            return Err(AllocError::InvalidAlignment);
        }

        // For aligned allocations, we allocate at the alignment order
        // This ensures the block is naturally aligned
        let addr = self.allocate(align_order)?;

        // If we allocated more than needed, split off the excess
        if align_order > order {
            // Free the upper buddy at each order down to the requested order
            for split_order in order..align_order {
                let buddy_size = (1 << split_order) * arch::PAGE_SIZE;
                let buddy_addr = PhysicalAddress::new(addr.as_usize() + buddy_size);
                self.deallocate(buddy_addr, split_order);
            }

            // Update the order of the allocated block
            let frame_num = addr.frame_number();
            if let Some(frame) = self.memory_map.frame_mut(frame_num) {
                frame.set_order(order as u8);
            }
        }

        Ok(addr)
    }

    /// Deallocates 2^order frames starting at the given address.
    ///
    /// Returns the frames to the free lists, attempting to coalesce with buddy blocks.
    /// Coalescing proceeds recursively up through orders until a buddy is allocated or
    /// MAX_ORDER is reached.
    ///
    /// This function is also used during initialization to add memory regions to the allocator.
    pub fn deallocate(&mut self, base: PhysicalAddress, order: usize) {
        if order > MAX_ORDER {
            return;
        }

        let mut current_order = order;
        let mut current_addr = base;

        // Mark the frame as free and set its order for coalescing
        let frame_num = current_addr.frame_number();
        if let Some(frame) = self.memory_map.frame_mut(frame_num) {
            frame.flags.clear(FrameFlag::Allocated);
            frame.set_order(order as u8);
        }

        // Try to coalesce with buddies
        while current_order < MAX_ORDER {
            let buddy_addr = self.buddy_address(current_addr, current_order);

            // Check if the buddy is free and at the same order
            if !self.is_buddy_free(buddy_addr, current_order) {
                break;
            }

            // Remove buddy from its free list
            self.remove_from_free_list(buddy_addr, current_order);

            // Merge with buddy - the merged block starts at the lower address
            current_addr = if current_addr.as_usize() < buddy_addr.as_usize() {
                current_addr
            } else {
                buddy_addr
            };
            current_order += 1;
        }

        // Add the (possibly coalesced) block to the free list
        self.add_to_free_list(current_addr, current_order);
    }

    /// Returns the total number of frames managed by this allocator.
    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Returns the number of free frames across all orders.
    pub fn free_frames(&self) -> usize {
        self.free_lists
            .iter()
            .enumerate()
            .map(|(order, list)| list.count() * (1 << order))
            .sum()
    }

    /// Returns the number of free blocks at a specific order.
    pub fn free_blocks_at_order(&self, order: usize) -> usize {
        if order > MAX_ORDER {
            return 0;
        }
        self.free_lists[order].count()
    }

    /// Returns the number of allocated frames.
    ///
    /// Note: This counts frames marked as allocated in the memory map, which requires
    /// iterating through all sections. For large memory maps, consider caching this value.
    pub fn allocated_frames(&self) -> usize {
        // This is a simplified count - in practice you'd want to track this
        // We iterate through sections and count allocated frames
        self.free_lists
            .iter()
            .enumerate()
            .map(|(order, list)| {
                // Each free block at order N represents 2^N frames that are NOT allocated
                // Total frames - free frames = allocated frames, but we can't easily compute this
                // without knowing the total. For now, return 0 if we haven't tracked allocations.
                let _ = (order, list);
                0
            })
            .sum::<usize>();
        // For now, we don't have a good way to count allocated frames without
        // iterating the entire memory map. Return total - free as an approximation.
        self.total_frames.saturating_sub(self.free_frames())
    }

    /// Returns a reference to the frame metadata for the given frame number.
    pub fn frame(&self, frame_number: FrameNumber) -> Option<&crate::Frame> {
        self.memory_map.frame(frame_number)
    }

    /// Returns a mutable reference to the frame metadata for the given frame number.
    pub fn frame_mut(&mut self, frame_number: FrameNumber) -> Option<&mut crate::Frame> {
        self.memory_map.frame_mut(frame_number)
    }

    // Private helper methods

    /// Translates a physical address to a writable pointer.
    ///
    /// In software emulation mode, this uses the emulated memory region.
    /// In kernel mode, this uses the direct-mapped virtual address.
    fn phys_to_ptr<T>(&self, addr: PhysicalAddress) -> *mut T {
        VirtualAddress::direct_mapped(addr).as_mut_ptr()
    }

    /// Converts a pointer back to a physical address.
    ///
    /// In software emulation mode, calculates the offset from the emulated memory base.
    /// In kernel mode, converts from the direct-mapped virtual address.
    fn ptr_to_phys<T>(&self, ptr: *const T) -> PhysicalAddress {
        let virt = VirtualAddress::from_ptr(ptr);
        PhysicalAddress::from_direct_mapped(virt)
    }

    /// Finds the lowest order with available blocks that can satisfy the request.
    fn find_free_order(&self, min_order: usize) -> Result<usize, AllocError> {
        for order in min_order..=MAX_ORDER {
            if !self.free_lists[order].is_empty() {
                return Ok(order);
            }
        }
        Err(AllocError::OutOfMemory)
    }

    /// Splits a block from `from_order` down to `to_order`, adding buddies to free lists.
    fn split_block(&mut self, addr: PhysicalAddress, from_order: usize, to_order: usize) {
        let current_addr = addr;
        for order in (to_order..from_order).rev() {
            let buddy_size = (1 << order) * arch::PAGE_SIZE;
            let buddy_addr = PhysicalAddress::new(current_addr.as_usize() + buddy_size);
            self.add_to_free_list(buddy_addr, order);
        }
    }

    /// Calculates the buddy address for a block at the given order.
    fn buddy_address(&self, addr: PhysicalAddress, order: usize) -> PhysicalAddress {
        let block_size = (1 << order) * arch::PAGE_SIZE;
        let buddy_offset = addr.as_usize() ^ block_size;
        PhysicalAddress::new(buddy_offset)
    }

    /// Checks if the buddy at the given address and order is free.
    fn is_buddy_free(&self, buddy_addr: PhysicalAddress, order: usize) -> bool {
        let frame_num = buddy_addr.frame_number();
        if let Some(frame) = self.memory_map.frame(frame_num) {
            let frame_order = frame.order();
            !frame.flags.atomic_test(FrameFlag::Allocated)
                && !frame.flags.atomic_test(FrameFlag::Reserved)
                && frame_order == order as u8
                && (frame_order as usize) <= MAX_ORDER
        } else {
            false
        }
    }

    /// Adds a block to the free list at the given order.
    fn add_to_free_list(&mut self, addr: PhysicalAddress, order: usize) {
        let block_ptr = self.phys_to_ptr::<FreeBlock>(addr);

        // Mark the frame with the order
        let frame_num = addr.frame_number();
        if let Some(frame) = self.memory_map.frame_mut(frame_num) {
            frame.set_order(order as u8);
        }

        self.free_lists[order].push(block_ptr);
    }

    /// Removes a block from the free list at the given order.
    ///
    /// This is a slow O(n) operation that walks the list. It's only used during coalescing,
    /// which is relatively rare compared to allocations.
    fn remove_from_free_list(&mut self, addr: PhysicalAddress, order: usize) {
        let target_ptr = self.phys_to_ptr::<FreeBlock>(addr);

        let list = &self.free_lists[order];

        loop {
            let head = list.head.load(Ordering::Acquire);
            if head.is_null() {
                return;
            }

            if head == target_ptr {
                // Target is at the head
                let next = unsafe { (*head).next };
                if list
                    .head
                    .compare_exchange(head, next, Ordering::Release, Ordering::Acquire)
                    .is_ok()
                {
                    list.count.fetch_sub(1, Ordering::Release);
                    return;
                }
            } else {
                // Walk the list to find the target
                let mut prev = head;
                loop {
                    let current = unsafe { (*prev).next };
                    if current.is_null() {
                        return;
                    }
                    if current == target_ptr {
                        let next = unsafe { (*current).next };
                        unsafe {
                            (*prev).next = next;
                        }
                        list.count.fetch_sub(1, Ordering::Release);
                        return;
                    }
                    prev = current;
                }
            }
        }
    }

    /// Converts a FreeBlock pointer to a physical address.
    fn block_to_address(&self, block: NonNull<FreeBlock>) -> PhysicalAddress {
        self.ptr_to_phys(block.as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BootMemoryRegion;

    /// Test implementation of BootMemoryRegion.
    struct TestRegion {
        base: PhysicalAddress,
        size: usize,
    }

    impl TestRegion {
        fn new(base: usize, size: usize) -> Self {
            Self {
                base: PhysicalAddress::new(base),
                size,
            }
        }
    }

    impl BootMemoryRegion for TestRegion {
        fn base(&self) -> PhysicalAddress {
            self.base
        }

        fn size(&self) -> usize {
            self.size
        }

        fn is_usable(&self) -> bool {
            true
        }
    }

    fn setup_test_memmap(num_frames: usize) -> MemoryMap {
        // Set up the address translator for this test
        if crate::AddressTranslator::try_current().is_none() {
            let total_size = num_frames * arch::PAGE_SIZE;
            crate::AddressTranslator::set_current(crate::AddressTranslator::emulated(total_size));
        }

        let boot_map = [TestRegion::new(0, num_frames as usize * arch::PAGE_SIZE)];
        MemoryMap::from_boot_map(&boot_map)
    }

    #[test]
    fn creates_empty_allocator() {
        let memmap = setup_test_memmap(4096); // 4096 frames
        let pmm = PhysicalMemoryManager::new(memmap);

        assert_eq!(pmm.free_frames(), 0);
    }

    #[test]
    fn deallocates_single_frame() {
        let memmap = setup_test_memmap(4096);
        let mut pmm = PhysicalMemoryManager::new(memmap);

        // Deallocate a single frame at order 0
        pmm.deallocate(PhysicalAddress::new(0), 0);

        assert_eq!(pmm.free_frames(), 1);
        assert_eq!(pmm.free_blocks_at_order(0), 1);
    }

    #[test]
    fn allocates_single_frame() {
        let memmap = setup_test_memmap(4096);
        let mut pmm = PhysicalMemoryManager::new(memmap);

        // Add a frame
        pmm.deallocate(PhysicalAddress::new(0), 0);

        // Allocate it
        let result = pmm.allocate(0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PhysicalAddress::new(0));
        assert_eq!(pmm.free_frames(), 0);
    }

    #[test]
    fn coalesces_buddies() {
        let memmap = setup_test_memmap(4096);
        let mut pmm = PhysicalMemoryManager::new(memmap);

        let frame_size = arch::PAGE_SIZE;

        // Deallocate two buddy frames
        pmm.deallocate(PhysicalAddress::new(0), 0);
        pmm.deallocate(PhysicalAddress::new(frame_size), 0);

        // They should coalesce into an order-1 block
        assert_eq!(pmm.free_blocks_at_order(0), 0);
        assert_eq!(pmm.free_blocks_at_order(1), 1);
        assert_eq!(pmm.free_frames(), 2);
    }

    #[test]
    fn splits_larger_blocks() {
        let memmap = setup_test_memmap(4096);
        let mut pmm = PhysicalMemoryManager::new(memmap);

        // Deallocate an order-2 block (4 frames)
        pmm.deallocate(PhysicalAddress::new(0), 2);

        // Allocate an order-0 block (1 frame)
        let result = pmm.allocate(0);
        assert!(result.is_ok());

        // Should have split: used 1 frame, have 3 left
        assert_eq!(pmm.free_frames(), 3);
    }
}
