//! Address space management.
//!
//! This module provides the [`AddressSpace`] type, which combines an architecture-specific
//! page directory (hardware mapping) with a list of [`VirtualMemoryArea`] records (software
//! description of virtual regions). VMAs are the authoritative source of truth; page-table
//! state is derived from them via [`alloc_area`](AddressSpace::alloc_area) and
//! [`free_area`](AddressSpace::free_area).
//!
//! ## VMA list invariants
//!
//! The `areas` list always satisfies:
//! 1. **Sorted** — entries are ordered by `start` (ascending, strictly).
//! 2. **No adjacent duplicates** — two areas whose ranges touch (`left.end == right.start`)
//!    are merged if and only if they have the same `flags` and `kind`.
//! 3. **No overlaps** — `areas[i].end <= areas[i+1].start` for all i.
//!
//! Gaps are permitted. Together these invariants keep the list compact and suitable for
//! O(log n) binary-search lookups.

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::Mutex;

use crate::{
    PhysicalAddress, VirtualAddress,
    arch::{self, PageFlags},
    page_directory::PageDirectory,
    physical_memory_manager::{AllocError, PhysicalMemoryManager},
};

/// Describes what backs a [`VirtualMemoryArea`].
///
/// Designed to be extended with file-backed and device-mapped regions later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmaKind {
    /// Anonymous memory — no file backing, no special device semantics.
    Anonymous,
}

/// A Virtual Memory Area: the software description of one contiguous virtual region.
///
/// VMAs are authoritative. The page directory is kept in sync with the VMA list by
/// [`alloc_area`](AddressSpace::alloc_area) and [`free_area`](AddressSpace::free_area).
#[derive(Debug, Clone, Copy)]
pub struct VirtualMemoryArea {
    /// First page-aligned address of the region (inclusive).
    pub start: VirtualAddress,
    /// First address beyond the region (exclusive), page-aligned.
    pub end: VirtualAddress,
    /// Permissions / flags for pages in this region.
    pub flags: PageFlags,
    /// What backs this region.
    pub kind: VmaKind,
}

impl VirtualMemoryArea {
    /// Returns `true` if `other` can be merged into `self` — i.e. they have identical
    /// flags and kind. Callers are responsible for checking adjacency.
    fn can_merge_with(&self, other: &Self) -> bool {
        self.flags == other.flags && self.kind == other.kind
    }
}

/// An owned virtual address space.
///
/// Combines:
/// - A [`PageDirectory`] (hardware page tables loaded into the MMU).
/// - A sorted, non-overlapping list of [`VirtualMemoryArea`] records.
/// - A shared reference to the [`PhysicalMemoryManager`] used to allocate/free frames.
pub struct AddressSpace {
    page_dir: PageDirectory,
    /// Sorted by `start`, non-overlapping. See module-level invariants.
    areas: Vec<VirtualMemoryArea>,
    pmm: Arc<Mutex<PhysicalMemoryManager>>,
    /// First address to try when scanning for a free virtual range (used by [`alloc`]).
    alloc_hint: VirtualAddress,
}

impl AddressSpace {
    /// Creates an `AddressSpace` from an existing `PageDirectory` and a shared PMM.
    ///
    /// Intentionally `pub(crate)` — callers go through [`super::VirtualMemoryManager`].
    pub(crate) fn new(page_dir: PageDirectory, pmm: Arc<Mutex<PhysicalMemoryManager>>) -> Self {
        Self {
            page_dir,
            areas: Vec::new(),
            pmm,
            alloc_hint: VirtualAddress::new(arch::PAGE_SIZE),
        }
    }

    // ── Virtual region management ────────────────────────────────────────────────

    /// Allocates a virtual region: records a VMA and maps physical pages.
    ///
    /// Pre-allocates all order-0 frames from the PMM before touching the page
    /// directory. If the PMM cannot satisfy the request, already-allocated frames are
    /// returned and `Err` is returned — the address space is unchanged.
    ///
    /// If the new region is adjacent to an existing VMA that has the same flags and
    /// kind, they are merged into a single VMA (maintaining invariant 2).
    ///
    /// # Panics
    /// Panics if `start` is not page-aligned, or (in debug builds) if the new region
    /// overlaps an existing VMA.
    pub fn alloc_area(
        &mut self,
        start: VirtualAddress,
        size: usize,
        flags: PageFlags,
        kind: VmaKind,
    ) -> Result<(), AllocError> {
        assert!(
            start.is_aligned(arch::PAGE_SIZE),
            "alloc_area: start must be page-aligned"
        );
        let page_size = arch::PAGE_SIZE;
        let size_rounded = (size + page_size - 1) & !(page_size - 1);
        let end = start + size_rounded;
        let num_pages = size_rounded / page_size;

        // Find the insertion point: first index where areas[idx].start >= start.
        // Do this before allocating any physical frames so that a programming error
        // (double-mapping the same virtual range) is caught without consuming resources.
        let idx = self.areas.partition_point(|a| a.start < start);

        let overlaps = (idx > 0 && self.areas[idx - 1].end > start)
            || (idx < self.areas.len() && self.areas[idx].start < end);
        if overlaps {
            return Err(AllocError::RegionAlreadyMapped);
        }

        // Pre-allocate all frames under the lock; roll back on OOM.
        let frames: Vec<PhysicalAddress> = {
            let mut pmm = self.pmm.lock();
            let mut frames = Vec::with_capacity(num_pages);
            for _ in 0..num_pages {
                match pmm.allocate(0) {
                    Ok(phys) => frames.push(phys),
                    Err(e) => {
                        for f in &frames {
                            pmm.deallocate(*f, 0);
                        }
                        return Err(e);
                    }
                }
            }
            frames
        };

        // Check whether to merge with the left and/or right neighbor.
        let new_vma = VirtualMemoryArea { start, end, flags, kind };
        let merge_left = idx > 0
            && self.areas[idx - 1].end == start
            && self.areas[idx - 1].can_merge_with(&new_vma);
        let merge_right = idx < self.areas.len()
            && self.areas[idx].start == end
            && self.areas[idx].can_merge_with(&new_vma);

        match (merge_left, merge_right) {
            (true, true) => {
                // Absorb the new region and the right neighbor into the left neighbor.
                let right_end = self.areas[idx].end;
                self.areas.remove(idx);
                self.areas[idx - 1].end = right_end;
            }
            (true, false) => {
                self.areas[idx - 1].end = end;
            }
            (false, true) => {
                self.areas[idx].start = start;
            }
            (false, false) => {
                self.areas.insert(idx, new_vma);
            }
        }

        // Map pages (PMM lock already released).
        for (i, phys) in frames.iter().enumerate() {
            self.page_dir.map(start + i * page_size, *phys, flags);
        }

        #[cfg(any(test, debug_assertions))]
        self.assert_invariants();

        Ok(())
    }

    /// Frees `[start, start + size)`: removes or splits overlapping VMAs, unmaps
    /// pages, and returns physical frames to the PMM.
    ///
    /// VMAs that partially overlap the freed range are **split** — the surviving
    /// fragments retain the original VMA's flags and kind. Pages that were not
    /// mapped are skipped silently.
    ///
    /// # Panics
    /// Panics if `start` is not page-aligned.
    pub fn free_area(&mut self, start: VirtualAddress, size: usize) {
        assert!(
            start.is_aligned(arch::PAGE_SIZE),
            "free_area: start must be page-aligned"
        );
        let page_size = arch::PAGE_SIZE;
        let size_rounded = (size + page_size - 1) & !(page_size - 1);
        let end = start + size_rounded;

        // Binary-search for the subrange of VMAs that overlap [start, end).
        // An area overlaps iff area.end > start AND area.start < end.
        let first = self.areas.partition_point(|a| a.end <= start);
        let last = self.areas.partition_point(|a| a.start < end);
        // areas[first..last] are all areas that overlap the freed range.

        // Collect at most two surviving fragments from the edges of the overlapping range.
        let mut fragments: Vec<VirtualMemoryArea> = Vec::with_capacity(2);
        if first < last {
            let first_area = self.areas[first];
            let last_area = self.areas[last - 1];

            // Left fragment: the part of the first overlapping area that lies before `start`.
            if first_area.start < start {
                fragments.push(VirtualMemoryArea {
                    start: first_area.start,
                    end: start,
                    flags: first_area.flags,
                    kind: first_area.kind,
                });
            }
            // Right fragment: the part of the last overlapping area that lies after `end`.
            if last_area.end > end {
                fragments.push(VirtualMemoryArea {
                    start: end,
                    end: last_area.end,
                    flags: last_area.flags,
                    kind: last_area.kind,
                });
            }
        }

        // Replace the overlapping subrange with the surviving fragments.
        // Sorted order is maintained: both fragments, if present, sit within the
        // same range as the removed areas.
        self.areas.splice(first..last, fragments);

        // Unmap pages and return frames to the PMM.
        let mut pmm = self.pmm.lock();
        let mut offset = 0usize;
        while offset < size_rounded {
            let virt = start + offset;
            if let Some(phys) = self.page_dir.unmap(virt) {
                pmm.deallocate(phys, 0);
            }
            offset += page_size;
        }

        #[cfg(any(test, debug_assertions))]
        self.assert_invariants();
    }

    // ── VMA inspection ──────────────────────────────────────────────────────────

    /// Returns the recorded virtual memory areas for this address space.
    pub fn areas(&self) -> &[VirtualMemoryArea] {
        &self.areas
    }

    // ── Automatic virtual range allocation ──────────────────────────────────────

    /// Sets the hint used by [`alloc`](Self::alloc) to find the next free range.
    ///
    /// Defaults to `PAGE_SIZE` (skipping the null page). Call this to customise the
    /// starting address for a new address space — for example, to reserve the lower
    /// pages for a specific purpose.
    pub fn set_alloc_hint(&mut self, hint: VirtualAddress) {
        self.alloc_hint = hint;
    }

    /// Finds the first free virtual range of at least `size` bytes starting from the
    /// internal allocation hint, maps it, and returns the chosen start address.
    ///
    /// `size` is rounded up to the next page boundary. Delegates to [`alloc_area`](Self::alloc_area)
    /// once a suitable gap has been found, so all VMA invariants and physical-frame
    /// pre-allocation guarantees are preserved. The hint is advanced past the new
    /// allocation for the next call.
    ///
    /// # Errors
    /// - [`AllocError::OutOfVirtualAddressSpace`] — no gap large enough exists below
    ///   the canonical-address boundary.
    /// - Any error propagated from [`alloc_area`](Self::alloc_area) (e.g.
    ///   [`AllocError::OutOfMemory`] when the PMM is exhausted).
    pub fn alloc(
        &mut self,
        size: usize,
        flags: PageFlags,
        kind: VmaKind,
    ) -> Result<VirtualAddress, AllocError> {
        let page_size = arch::PAGE_SIZE;
        let size_rounded = (size + page_size - 1) & !(page_size - 1);

        let start = self
            .find_free_range(size_rounded)
            .ok_or(AllocError::OutOfVirtualAddressSpace)?;

        // Advance the hint past this allocation before delegating, so that even if
        // alloc_area merges VMAs the hint always moves forward.
        self.alloc_hint = start + size_rounded;

        self.alloc_area(start, size_rounded, flags, kind)?;

        Ok(start)
    }

    /// Scans the VMA list from [`alloc_hint`] for the first gap that fits `size` bytes.
    ///
    /// `size` must already be page-rounded. Returns `None` if no valid range exists
    /// below the non-canonical hole boundary (`1 << (MAX_VIRTUAL_BITS - 1)`).
    fn find_free_range(&self, size: usize) -> Option<VirtualAddress> {
        // Stay below the non-canonical hole (x86_64: bit 47; software: bit 15).
        let upper_limit = 1usize << (arch::MAX_VIRTUAL_BITS - 1);

        let mut candidate = self.alloc_hint.as_usize();

        // Find the first VMA whose end exceeds `candidate` — it may overlap the start.
        let idx = self.areas.partition_point(|a| a.end.as_usize() <= candidate);

        for area in &self.areas[idx..] {
            let area_start = area.start.as_usize();
            let area_end = area.end.as_usize();

            // Is there a gap before this VMA that fits `size`?
            match candidate.checked_add(size) {
                Some(end) if end <= area_start => return Some(VirtualAddress::new(candidate)),
                None => return None, // overflow
                _ => {}
            }

            // Move past this VMA.
            if area_end > candidate {
                candidate = area_end;
            }
        }

        // Past all VMAs — verify the range fits within the valid half.
        match candidate.checked_add(size) {
            Some(end) if end <= upper_limit => Some(VirtualAddress::new(candidate)),
            _ => None,
        }
    }

    // ── Activation ──────────────────────────────────────────────────────────────

    /// Activates this address space (loads the page directory into the MMU).
    ///
    /// # Safety
    /// The caller must ensure all memory that will be accessed after the switch is
    /// correctly mapped, including the kernel text, data, and stack.
    pub unsafe fn activate(&self) {
        // SAFETY: propagated from caller.
        unsafe { self.page_dir.activate() }
    }

    // ── Crate-internal accessors ─────────────────────────────────────────────────

    pub(crate) fn page_directory(&self) -> &PageDirectory {
        &self.page_dir
    }

    pub(crate) fn page_directory_mut(&mut self) -> &mut PageDirectory {
        &mut self.page_dir
    }

    // ── Invariant checking ───────────────────────────────────────────────────────

    /// Panics if any of the three VMA list invariants are violated.
    ///
    /// Only compiled in test and debug-assertions builds.
    #[cfg(any(test, debug_assertions))]
    fn assert_invariants(&self) {
        for window in self.areas.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                a.start < b.start,
                "invariant 1 violated: areas not sorted ({:?} >= {:?})",
                a.start,
                b.start
            );
            assert!(
                a.end <= b.start,
                "invariant 3 violated: areas overlap \
                 ([{:?},{:?}) and [{:?},{:?}))",
                a.start, a.end, b.start, b.end
            );
            if a.end == b.start {
                assert!(
                    !a.can_merge_with(b),
                    "invariant 2 violated: adjacent areas with identical flags+kind \
                     were not merged ([{:?},{:?}) and [{:?},{:?}))",
                    a.start, a.end, b.start, b.end
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddressTranslator, BootMemoryRegion, memmap::MemoryMap};

    fn setup() {
        let _ = std::panic::catch_unwind(|| {
            AddressTranslator::set_current(AddressTranslator::emulated(128 * 1024));
        });
    }

    struct Region {
        base: PhysicalAddress,
        size: usize,
    }
    impl BootMemoryRegion for Region {
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

    // NOTE: In tests, alloc_page_table() uses a bump allocator starting at phys 0.
    // PMM frames start at BASE_FRAME * PAGE_SIZE to avoid overlapping that region.
    const BASE_FRAME: usize = 256;

    fn make_pmm(num_frames: usize) -> Arc<Mutex<PhysicalMemoryManager>> {
        let base = BASE_FRAME * arch::PAGE_SIZE;
        let boot = [Region {
            base: PhysicalAddress::new(base),
            size: num_frames * arch::PAGE_SIZE,
        }];
        let mut pmm = PhysicalMemoryManager::new(MemoryMap::from_boot_map(&boot));
        for i in 0..num_frames {
            pmm.deallocate(PhysicalAddress::new(base + i * arch::PAGE_SIZE), 0);
        }
        Arc::new(Mutex::new(pmm))
    }

    fn new_space(pmm: Arc<Mutex<PhysicalMemoryManager>>) -> AddressSpace {
        AddressSpace::new(PageDirectory::new(), pmm)
    }

    fn present_flags() -> PageFlags {
        let mut f = PageFlags::empty();
        f.set_present(true);
        f
    }

    // ── Basic alloc / free ───────────────────────────────────────────────────────

    #[test]
    fn alloc_and_free_area() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(Arc::clone(&pmm));
        let page = arch::PAGE_SIZE;
        let virt = VirtualAddress::new(page);
        let flags = present_flags();

        let before = pmm.lock().free_frames();
        space
            .alloc_area(virt, page, flags, VmaKind::Anonymous)
            .expect("alloc_area failed");
        assert_eq!(space.areas().len(), 1);
        assert!(pmm.lock().free_frames() < before);

        let after_alloc = pmm.lock().free_frames();
        space.free_area(virt, page);
        assert_eq!(space.areas().len(), 0);
        assert_eq!(pmm.lock().free_frames(), after_alloc + 1);
    }

    #[test]
    fn alloc_area_records_vma_bounds() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let virt = VirtualAddress::new(page);
        let flags = present_flags();

        space
            .alloc_area(virt, 2 * page, flags, VmaKind::Anonymous)
            .unwrap();
        assert_eq!(space.areas()[0].start, virt);
        assert_eq!(space.areas()[0].end, virt + 2 * page);
        assert_eq!(space.areas()[0].kind, VmaKind::Anonymous);
    }

    #[test]
    fn free_area_returns_frames_to_pmm() {
        setup();
        let pmm = make_pmm(256);
        let mut space = new_space(Arc::clone(&pmm));
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        space
            .alloc_area(VirtualAddress::new(page), 2 * page, flags, VmaKind::Anonymous)
            .unwrap();

        let before = pmm.lock().free_frames();
        space.free_area(VirtualAddress::new(page), 2 * page);
        assert_eq!(pmm.lock().free_frames(), before + 2);
    }

    // ── Sorted order ─────────────────────────────────────────────────────────────

    #[test]
    fn alloc_out_of_order_maintains_sorted_order() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Alloc pages 3 and 1 (with a gap at page 2 to prevent merging since same flags).
        space
            .alloc_area(VirtualAddress::new(3 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();

        assert_eq!(space.areas().len(), 2);
        assert!(
            space.areas()[0].start < space.areas()[1].start,
            "areas not sorted"
        );
        assert_eq!(space.areas()[0].start, VirtualAddress::new(page));
        assert_eq!(space.areas()[1].start, VirtualAddress::new(3 * page));
    }

    // ── Merging ──────────────────────────────────────────────────────────────────

    #[test]
    fn alloc_adjacent_same_flags_merges() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Page 1 then page 2 (adjacent, same flags) → merged into one VMA.
        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(2 * page), page, flags, VmaKind::Anonymous)
            .unwrap();

        assert_eq!(space.areas().len(), 1, "adjacent same-flags areas must be merged");
        assert_eq!(space.areas()[0].start, VirtualAddress::new(page));
        assert_eq!(space.areas()[0].end, VirtualAddress::new(3 * page));
    }

    #[test]
    fn alloc_adjacent_right_then_left_merges() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Alloc page 2 first, then page 1 (adjacent to the left) → merged.
        space
            .alloc_area(VirtualAddress::new(2 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();

        assert_eq!(space.areas().len(), 1, "adjacent same-flags areas must be merged");
        assert_eq!(space.areas()[0].start, VirtualAddress::new(page));
        assert_eq!(space.areas()[0].end, VirtualAddress::new(3 * page));
    }

    #[test]
    fn alloc_bridges_gap_merges_triple() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Alloc page 1 and page 3 (gap at page 2).
        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(3 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        assert_eq!(space.areas().len(), 2);

        // Bridge with page 2 → all three merge.
        space
            .alloc_area(VirtualAddress::new(2 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        assert_eq!(space.areas().len(), 1, "bridging alloc must merge all three areas");
        assert_eq!(space.areas()[0].start, VirtualAddress::new(page));
        assert_eq!(space.areas()[0].end, VirtualAddress::new(4 * page));
    }

    #[test]
    fn alloc_adjacent_different_flags_no_merge() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;

        let flags_present = present_flags();
        let flags_empty = PageFlags::empty(); // present bit NOT set — different from flags_present

        // Adjacent but different flags → must NOT merge.
        space
            .alloc_area(VirtualAddress::new(page), page, flags_present, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(2 * page), page, flags_empty, VmaKind::Anonymous)
            .unwrap();

        assert_eq!(space.areas().len(), 2, "areas with different flags must not be merged");
    }

    // ── Free / split ─────────────────────────────────────────────────────────────

    #[test]
    fn free_area_mid_region_splits_vma() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let base = VirtualAddress::new(page);
        let flags = present_flags();

        space
            .alloc_area(base, 3 * page, flags, VmaKind::Anonymous)
            .unwrap();
        assert_eq!(space.areas().len(), 1);

        // Free the middle page.
        space.free_area(base + page, page);

        assert_eq!(space.areas().len(), 2);
        assert_eq!(space.areas()[0].start, base);
        assert_eq!(space.areas()[0].end, base + page);
        assert_eq!(space.areas()[1].start, base + 2 * page);
        assert_eq!(space.areas()[1].end, base + 3 * page);
    }

    #[test]
    fn free_area_spanning_multiple_vmas() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Alloc pages 1 and 3 (gap at page 2) so they are separate VMAs.
        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(3 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        assert_eq!(space.areas().len(), 2);

        // Free pages 1–3 (includes the gap) — both VMAs are removed.
        space.free_area(VirtualAddress::new(page), 3 * page);
        assert_eq!(space.areas().len(), 0);
    }

    #[test]
    fn free_area_partial_overlap_left() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        space
            .alloc_area(VirtualAddress::new(page), 2 * page, flags, VmaKind::Anonymous)
            .unwrap();

        // Free only the first page — right half survives.
        space.free_area(VirtualAddress::new(page), page);
        assert_eq!(space.areas().len(), 1);
        assert_eq!(space.areas()[0].start, VirtualAddress::new(2 * page));
        assert_eq!(space.areas()[0].end, VirtualAddress::new(3 * page));
    }

    #[test]
    fn free_area_partial_overlap_right() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        space
            .alloc_area(VirtualAddress::new(page), 2 * page, flags, VmaKind::Anonymous)
            .unwrap();

        // Free only the second page — left half survives.
        space.free_area(VirtualAddress::new(2 * page), page);
        assert_eq!(space.areas().len(), 1);
        assert_eq!(space.areas()[0].start, VirtualAddress::new(page));
        assert_eq!(space.areas()[0].end, VirtualAddress::new(2 * page));
    }

    // ── alloc (automatic range finder) ───────────────────────────────────────────

    #[test]
    fn alloc_returns_page_aligned_nonzero_address() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let flags = present_flags();

        let addr = space.alloc(1, flags, VmaKind::Anonymous).expect("alloc failed");
        assert_ne!(addr.as_usize(), 0, "must not return the null page");
        assert!(addr.is_aligned(arch::PAGE_SIZE), "must be page-aligned");
        assert_eq!(space.areas().len(), 1);
    }

    #[test]
    fn alloc_two_sequential_do_not_overlap() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let flags = present_flags();
        let page = arch::PAGE_SIZE;

        let a = space.alloc(page, flags, VmaKind::Anonymous).unwrap();
        let b = space.alloc(page, flags, VmaKind::Anonymous).unwrap();

        // The two allocations should be adjacent (and thus merged into one VMA).
        assert!(
            b.as_usize() >= a.as_usize() + page,
            "second allocation must not overlap the first"
        );
    }

    #[test]
    fn alloc_skips_existing_vma() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Pre-map page 1 manually so it sits right at the default hint.
        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();

        // alloc() must land somewhere past page 1.
        let addr = space.alloc(page, flags, VmaKind::Anonymous).unwrap();
        assert!(
            addr.as_usize() >= 2 * page,
            "alloc must skip the pre-existing VMA"
        );
    }

    #[test]
    fn alloc_fills_gap_between_vmas() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        // Pre-map pages 2 and 4 (gap at page 3), then set hint to page 1.
        space
            .alloc_area(VirtualAddress::new(2 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space
            .alloc_area(VirtualAddress::new(4 * page), page, flags, VmaKind::Anonymous)
            .unwrap();
        space.set_alloc_hint(VirtualAddress::new(page));

        // The first free range from page 1 is page 1 itself (gap before page 2).
        let addr = space.alloc(page, flags, VmaKind::Anonymous).unwrap();
        assert_eq!(addr, VirtualAddress::new(page), "should fill the gap at page 1");
    }

    #[test]
    fn free_area_no_overlap_is_noop() {
        setup();
        let pmm = make_pmm(64);
        let mut space = new_space(pmm);
        let page = arch::PAGE_SIZE;
        let flags = present_flags();

        space
            .alloc_area(VirtualAddress::new(page), page, flags, VmaKind::Anonymous)
            .unwrap();

        // Free a range that does not touch the VMA.
        space.free_area(VirtualAddress::new(4 * page), page);
        assert_eq!(space.areas().len(), 1);
        assert_eq!(space.areas()[0].start, VirtualAddress::new(page));
    }
}
