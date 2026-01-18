//! Sparse memory map for tracking physical frame metadata.
//!
//! This module provides a two-level sparse array structure for efficiently tracking
//! metadata about physical memory frames. Memory is divided into sections of 32,768
//! frames each. Sections containing only reserved memory don't allocate frame storage,
//! and sections with usable memory only allocate storage for the contiguous region
//! containing all usable frames.
//!
//! # Building a Memory Map
//!
//! To build a memory map, implement the [`BootMemoryRegion`] trait on your bootloader's
//! memory map entry type, then call [`MemoryMap::from_boot_map`]:
//!
//! ```ignore
//! struct MyBootEntry { /* ... */ }
//!
//! impl BootMemoryRegion for MyBootEntry {
//!     fn base(&self) -> PhysicalAddress { /* ... */ }
//!     fn size(&self) -> usize { /* ... */ }
//!     fn is_usable(&self) -> bool { /* ... */ }
//! }
//!
//! let boot_entries: &[MyBootEntry] = /* ... */;
//! let memory_map = MemoryMap::from_boot_map(boot_entries);
//! ```

use alloc::boxed::Box;

use crate::{Frame, FrameFlag, FrameNumber, HumanSize, PhysicalAddress, arch};

/// Number of frames per section (32 KiB worth of frame indices).
pub const FRAMES_PER_SECTION: usize = 32_768;

/// Size of a section in bytes.
pub const SECTION_SIZE: usize = FRAMES_PER_SECTION * arch::PAGE_SIZE;

/// Represents a single entry in a boot-time memory map.
///
/// Implement this trait on bootloader-specific memory map entry types
/// to allow pmm to build its internal memory map from the boot map.
pub trait BootMemoryRegion {
    /// Returns the base physical address of this region.
    fn base(&self) -> PhysicalAddress;

    /// Returns the size of this region in bytes.
    fn size(&self) -> usize;

    /// Returns whether this region contains usable memory.
    ///
    /// Usable memory can be freely used by the kernel for allocation.
    /// Non-usable memory (reserved, ACPI, device memory, etc.) should
    /// return `false`.
    fn is_usable(&self) -> bool;
}

/// A section of the memory map containing frames for a contiguous region.
///
/// Each section covers `FRAMES_PER_SECTION` frame indices. If the section contains
/// any usable memory, `frames` holds the frame metadata for the smallest contiguous
/// region that includes all usable memory in the section.
pub struct Section {
    /// The starting frame number within this section's frame array.
    /// Frame index `start_frame` maps to `frames[0]`.
    start_frame: FrameNumber,
    /// Frame metadata, or None if the entire section is reserved/holes.
    frames: Option<Box<[Frame]>>,
}

impl Section {
    /// Creates an empty section (all reserved/holes).
    const fn empty() -> Self {
        Self {
            start_frame: FrameNumber::new(0),
            frames: None,
        }
    }

    pub fn frame_range(&self) -> Option<(FrameNumber, FrameNumber)> {
        let frames = self.frames.as_ref()?;
        let start = self.start_frame;
        let end = FrameNumber::new(self.start_frame.as_usize() + frames.len());
        Some((start, end))
    }

    /// Creates a section with the given frame range.
    fn with_frames(start_frame: FrameNumber, frames: Box<[Frame]>) -> Self {
        Self {
            start_frame,
            frames: Some(frames),
        }
    }

    /// Returns a reference to the frame at the given frame number, if present.
    fn frame(&self, frame_number: FrameNumber) -> Option<&Frame> {
        let frames = self.frames.as_ref()?;
        let start = self.start_frame.as_usize();
        let end = start + frames.len();
        let idx = frame_number.as_usize();

        if idx >= start && idx < end {
            Some(&frames[idx - start])
        } else {
            None
        }
    }

    /// Returns a mutable reference to the frame at the given frame number, if present.
    fn frame_mut(&mut self, frame_number: FrameNumber) -> Option<&mut Frame> {
        let frames = self.frames.as_mut()?;
        let start = self.start_frame.as_usize();
        let end = start + frames.len();
        let idx = frame_number.as_usize();

        if idx >= start && idx < end {
            Some(&mut frames[idx - start])
        } else {
            None
        }
    }
}

/// Holds metadata for all physical memory frames using a sparse two-level structure.
///
/// Memory is divided into sections of `FRAMES_PER_SECTION` frames. Sections that
/// contain only reserved memory don't allocate storage, reducing memory overhead
/// for systems with large reserved regions.
pub struct MemoryMap {
    sections: Box<[Section]>,
}

impl MemoryMap {
    /// Constructs a memory map from a boot-time memory map.
    ///
    /// The boot map entries are used to determine which regions are usable
    /// and which are reserved. The resulting memory map uses a sparse
    /// two-level structure to minimize memory overhead.
    ///
    /// # Type Parameters
    ///
    /// * `R` - A type implementing [`BootMemoryRegion`]
    ///
    /// # Arguments
    ///
    /// * `boot_map` - A slice of boot memory map entries
    pub fn from_boot_map<R: BootMemoryRegion>(boot_map: &[R]) -> Self {
        if boot_map.is_empty() {
            return Self {
                sections: Box::new([]),
            };
        }

        // Find the maximum address to determine section count
        let max_address = boot_map
            .iter()
            .map(|r| r.base().as_usize() + r.size())
            .max()
            .unwrap_or(0);

        let section_count = Self::section_count_for_address(max_address);

        log::trace!(
            "building memory map with {} sections for {} total memory",
            section_count,
            HumanSize::from(max_address)
        );

        // Build each section
        let sections: Box<[Section]> = (0..section_count)
            .map(|idx| Self::build_section(idx, boot_map))
            .collect();

        Self { sections }
    }

    /// Returns a reference to the frame at the given frame number.
    pub fn frame(&self, frame_number: FrameNumber) -> Option<&Frame> {
        let section_idx = frame_number.as_usize() / FRAMES_PER_SECTION;
        self.sections.get(section_idx)?.frame(frame_number)
    }

    /// Returns a mutable reference to the frame at the given frame number.
    pub fn frame_mut(&mut self, frame_number: FrameNumber) -> Option<&mut Frame> {
        let section_idx = frame_number.as_usize() / FRAMES_PER_SECTION;
        self.sections.get_mut(section_idx)?.frame_mut(frame_number)
    }

    /// Returns a reference to the frame for the given physical address.
    pub fn frame_for(&self, address: PhysicalAddress) -> Option<&Frame> {
        self.frame(address.frame_number())
    }

    /// Returns a mutable reference to the frame for the given physical address.
    pub fn frame_for_mut(&mut self, address: PhysicalAddress) -> Option<&mut Frame> {
        self.frame_mut(address.frame_number())
    }

    /// Returns a slice of all sections in the memory map.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Returns the number of frames that have allocated storage.
    pub fn allocated_frame_count(&self) -> usize {
        self.sections
            .iter()
            .filter_map(|s| s.frames.as_ref())
            .map(|f| f.len())
            .sum()
    }

    /// Returns the number of sections needed to cover addresses up to `max_address`.
    fn section_count_for_address(max_address: usize) -> usize {
        let max_frame = (max_address / arch::PAGE_SIZE) as usize;
        (max_frame / FRAMES_PER_SECTION) + 1
    }

    /// Builds a single section from the boot map.
    fn build_section<R: BootMemoryRegion>(section_idx: usize, boot_map: &[R]) -> Section {
        let section_start_frame = section_idx * FRAMES_PER_SECTION;
        let section_end_frame = section_start_frame + FRAMES_PER_SECTION;

        // Find the range of usable memory within this section
        let mut min_usable_frame: Option<usize> = None;
        let mut max_usable_frame: Option<usize> = None;

        for region in boot_map {
            if !region.is_usable() {
                continue;
            }

            let region_start_frame = (region.base().as_usize() / arch::PAGE_SIZE) as usize;
            let region_end_frame =
                ((region.base().as_usize() + region.size()) / arch::PAGE_SIZE) as usize;

            // Check if this region overlaps with the section
            if region_end_frame <= section_start_frame || region_start_frame >= section_end_frame {
                continue;
            }

            // Clamp to section bounds
            let start = region_start_frame.max(section_start_frame);
            let end = region_end_frame.min(section_end_frame);

            min_usable_frame = Some(min_usable_frame.map_or(start, |m| m.min(start)));
            max_usable_frame = Some(max_usable_frame.map_or(end, |m| m.max(end)));
        }

        // If no usable memory in this section, return empty
        let (min_frame, max_frame) = match (min_usable_frame, max_usable_frame) {
            (Some(min), Some(max)) => (min, max),
            _ => return Section::empty(),
        };

        // Allocate frames for the contiguous region
        let frame_count = max_frame - min_frame;
        let frames: Box<[Frame]> = (min_frame..max_frame)
            .map(|frame_idx| {
                let mut frame = Frame::default();

                // Determine the type of this frame based on boot map regions
                let frame_addr = PhysicalAddress::new(frame_idx * arch::PAGE_SIZE);
                if !Self::is_frame_usable(frame_addr, boot_map) {
                    frame.flags.set(FrameFlag::Reserved);
                }

                frame
            })
            .collect();

        log::trace!(
            "section {}: allocated {} frames starting at frame {}",
            section_idx,
            frame_count,
            min_frame
        );

        Section::with_frames(FrameNumber::new(min_frame), frames)
    }

    /// Determines if a frame is usable based on the boot map.
    ///
    /// A frame is usable if any usable region contains it and no later
    /// non-usable region overrides it. Boot map entries are processed
    /// in order, with later entries taking precedence.
    fn is_frame_usable<R: BootMemoryRegion>(addr: PhysicalAddress, boot_map: &[R]) -> bool {
        let mut usable = false;

        for region in boot_map {
            let region_end = region.base().as_usize() + region.size();
            if addr.as_usize() >= region.base().as_usize() && addr.as_usize() < region_end {
                usable = region.is_usable();
            }
        }

        usable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test implementation of BootMemoryRegion.
    struct TestRegion {
        base: PhysicalAddress,
        size: usize,
        usable: bool,
    }

    impl TestRegion {
        fn usable(base: usize, size: usize) -> Self {
            Self {
                base: PhysicalAddress::new(base),
                size,
                usable: true,
            }
        }

        fn reserved(base: usize, size: usize) -> Self {
            Self {
                base: PhysicalAddress::new(base),
                size,
                usable: false,
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
            self.usable
        }
    }

    #[test]
    fn empty_boot_map_creates_empty_memory_map() {
        let boot_map: &[TestRegion] = &[];
        let map = MemoryMap::from_boot_map(boot_map);

        assert_eq!(map.sections().len(), 0);
        assert_eq!(map.allocated_frame_count(), 0);
    }

    #[test]
    fn single_usable_region() {
        let boot_map = [TestRegion::usable(0, arch::PAGE_SIZE * 100)];

        let map = MemoryMap::from_boot_map(&boot_map);

        assert_eq!(map.sections().len(), 1);
        assert_eq!(map.allocated_frame_count(), 100);

        // Check that frames are accessible
        assert!(map.frame(FrameNumber::new(0)).is_some());
        assert!(map.frame(FrameNumber::new(99)).is_some());
        assert!(map.frame(FrameNumber::new(100)).is_none());
    }

    #[test]
    fn reserved_region_not_allocated() {
        let boot_map = [TestRegion::reserved(0, arch::PAGE_SIZE * 100)];

        let map = MemoryMap::from_boot_map(&boot_map);

        // Reserved-only sections don't allocate frame storage
        assert_eq!(map.sections().len(), 1);
        assert_eq!(map.allocated_frame_count(), 0);

        // Frames in reserved regions return None
        assert!(map.frame(FrameNumber::new(0)).is_none());
    }

    #[test]
    fn mixed_regions_in_section() {
        let boot_map = [
            // Reserved region first
            TestRegion::reserved(0, arch::PAGE_SIZE * 200),
            // Then usable region in the middle
            TestRegion::usable(arch::PAGE_SIZE * 50, arch::PAGE_SIZE * 100),
        ];

        let map = MemoryMap::from_boot_map(&boot_map);

        // Should allocate frames 50-150 (100 frames)
        assert_eq!(map.allocated_frame_count(), 100);

        // Frames before usable region are not allocated (holes)
        assert!(map.frame(FrameNumber::new(0)).is_none());
        assert!(map.frame(FrameNumber::new(49)).is_none());

        // Usable frames are accessible
        let frame_50 = map.frame(FrameNumber::new(50));
        assert!(frame_50.is_some());
        assert!(!frame_50.unwrap().flags.atomic_test(FrameFlag::Reserved));

        assert!(map.frame(FrameNumber::new(149)).is_some());

        // Frames after usable region are not allocated (holes)
        assert!(map.frame(FrameNumber::new(150)).is_none());
    }

    #[test]
    fn reserved_holes_within_usable_region() {
        let boot_map = [
            // Large usable region
            TestRegion::usable(0, arch::PAGE_SIZE * 100),
            // Reserved hole in the middle
            TestRegion::reserved(arch::PAGE_SIZE * 40, arch::PAGE_SIZE * 20),
        ];

        let map = MemoryMap::from_boot_map(&boot_map);

        // All 100 frames should be allocated (holes within are still allocated)
        assert_eq!(map.allocated_frame_count(), 100);

        // Check usable frames
        let frame_0 = map.frame(FrameNumber::new(0));
        assert!(frame_0.is_some());
        assert!(!frame_0.unwrap().flags.atomic_test(FrameFlag::Reserved));

        // Check reserved hole
        let frame_50 = map.frame(FrameNumber::new(50));
        assert!(frame_50.is_some());
        assert!(frame_50.unwrap().flags.atomic_test(FrameFlag::Reserved));

        // Check usable after hole
        let frame_70 = map.frame(FrameNumber::new(70));
        assert!(frame_70.is_some());
        assert!(!frame_70.unwrap().flags.atomic_test(FrameFlag::Reserved));
    }

    #[test]
    #[cfg(not(test))]
    fn multiple_sections() {
        let second_section_start = FRAMES_PER_SECTION as usize * arch::PAGE_SIZE;

        let boot_map = [
            // Region in first section
            TestRegion::usable(0, arch::PAGE_SIZE * 100),
            // Region in second section
            TestRegion::usable(second_section_start, arch::PAGE_SIZE * 50),
        ];

        let map = MemoryMap::from_boot_map(&boot_map);

        assert_eq!(map.section_count(), 2);
        assert_eq!(map.allocated_frame_count(), 150);

        // Check frames in first section
        assert!(map.frame(FrameNumber::new(0)).is_some());
        assert!(map.frame(FrameNumber::new(99)).is_some());

        // Check frames in second section
        assert!(map.frame(FrameNumber::new(FRAMES_PER_SECTION)).is_some());
        assert!(
            map.frame(FrameNumber::new(FRAMES_PER_SECTION + 49))
                .is_some()
        );
    }

    #[test]
    #[cfg(not(test))]
    fn sparse_sections() {
        let third_section_start = FRAMES_PER_SECTION as usize * 2 * arch::PAGE_SIZE;

        let boot_map = [
            // Region in first section
            TestRegion::usable(0, arch::PAGE_SIZE * 100),
            // Skip second section entirely, put region in third section
            TestRegion::usable(third_section_start, arch::PAGE_SIZE * 50),
        ];

        let map = MemoryMap::from_boot_map(&boot_map);

        assert_eq!(map.section_count(), 3);
        // Second section should be empty (no allocation)
        assert_eq!(map.allocated_frame_count(), 150);

        // Frames in empty second section return None
        assert!(map.frame(FrameNumber::new(FRAMES_PER_SECTION)).is_none());
        assert!(
            map.frame(FrameNumber::new(FRAMES_PER_SECTION + 100))
                .is_none()
        );

        // Frames in third section work
        assert!(
            map.frame(FrameNumber::new(FRAMES_PER_SECTION * 2))
                .is_some()
        );
    }

    #[test]
    fn frame_mut_access() {
        let boot_map = [TestRegion::usable(0, arch::PAGE_SIZE * 10)];

        let mut map = MemoryMap::from_boot_map(&boot_map);

        // Modify a frame
        let frame = map.frame_mut(FrameNumber::new(5)).unwrap();
        frame.flags.set(FrameFlag::Allocated);

        // Verify modification persisted
        let frame = map.frame(FrameNumber::new(5)).unwrap();
        assert!(frame.flags.atomic_test(FrameFlag::Allocated));
    }

    #[test]
    fn leading_trailing_holes_not_allocated() {
        let boot_map = [
            // Reserved region at start
            TestRegion::reserved(0, arch::PAGE_SIZE * 1000),
            // Small usable region in the middle of the section
            TestRegion::usable(arch::PAGE_SIZE * 500, arch::PAGE_SIZE * 100),
        ];

        let map = MemoryMap::from_boot_map(&boot_map);

        // Should only allocate the 100 usable frames, not the leading 500 reserved frames
        assert_eq!(map.allocated_frame_count(), 100);

        // Leading holes not allocated
        assert!(map.frame(FrameNumber::new(0)).is_none());
        assert!(map.frame(FrameNumber::new(499)).is_none());

        // Usable region allocated
        assert!(map.frame(FrameNumber::new(500)).is_some());
        assert!(map.frame(FrameNumber::new(599)).is_some());

        // Trailing holes not allocated
        assert!(map.frame(FrameNumber::new(600)).is_none());
    }

    #[test]
    fn frame_for_physical_address() {
        let boot_map = [TestRegion::usable(0, arch::PAGE_SIZE * 10)];

        let map = MemoryMap::from_boot_map(&boot_map);

        // frame_for should work with physical addresses
        let addr = PhysicalAddress::new(arch::PAGE_SIZE * 5);
        assert!(map.frame_for(addr).is_some());

        // Address in hole
        let hole_addr = PhysicalAddress::new(arch::PAGE_SIZE * 100);
        assert!(map.frame_for(hole_addr).is_none());
    }
}
