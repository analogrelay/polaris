use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Special order value indicating the frame is allocated but not from the buddy allocator,
/// or that the frame has never been managed by the buddy allocator.
pub const ORDER_NOT_BUDDY: u8 = 0xFF;

/// Holds metadata for a physical memory frame.
///
/// Modeled after Linux's `struct page`, this data structure holds all the metadata for a
/// physical memory frame.
pub struct Frame {
    /// Flags identifying the state of this frame.
    pub flags: FrameFlags,
    /// The order of allocation for this frame (0-11 for buddy allocator blocks, 0xFF if not from buddy allocator).
    /// Only meaningful when the Allocated flag is set.
    order: AtomicU8,
}

impl Frame {
    /// Gets the allocation order for this frame.
    pub fn order(&self) -> u8 {
        self.order.load(Ordering::Acquire)
    }

    /// Sets the allocation order for this frame.
    pub fn set_order(&self, order: u8) {
        self.order.store(order, Ordering::Release);
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            flags: FrameFlags::new(),
            order: AtomicU8::new(ORDER_NOT_BUDDY),
        }
    }
}

pub enum FrameFlag {
    /// Frame is allocated.
    Allocated = 1 << 0,
    /// Frame is reserved and should not be allocated.
    Reserved = 1 << 1,
}

/// Atomic flags for a physical memory frame.
#[derive(Default)]
pub struct FrameFlags(AtomicU64);

impl FrameFlags {
    /// Creates a new `FrameFlags` instance with all flags cleared.
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Creates a new `FrameFlags` instance with the given initial flags.
    pub const fn from_bits(initial: u64) -> Self {
        Self(AtomicU64::new(initial))
    }

    /// Sets the given flag non-atomically (by holding a mutable reference).
    pub fn set(&mut self, flag: FrameFlag) {
        let mask = flag as u64;
        *self.0.get_mut() |= mask;
    }

    /// Clears the given flag non-atomically (by holding a mutable reference).
    pub fn clear(&mut self, flag: FrameFlag) {
        let mask = !(flag as u64);
        *self.0.get_mut() &= mask;
    }

    /// Tests if the given flag is set, non-atomically (by holding a mutable reference).
    pub fn test(&mut self, flag: FrameFlag) -> bool {
        let mask = flag as u64;
        let value = *self.0.get_mut();
        (value & mask) != 0
    }

    /// Tests the given flag and sets it non-atomically (by holding a mutable reference), returning the previous value.
    pub fn test_and_set(&mut self, flag: FrameFlag) -> bool {
        let mask = flag as u64;
        let old = *self.0.get_mut();
        *self.0.get_mut() |= mask;
        (old & mask) != 0
    }

    /// Sets the given flag atomically.
    pub fn atomic_set(&self, flag: FrameFlag) {
        let mask = flag as u64;
        self.0.fetch_or(mask, Ordering::AcqRel);
    }

    /// Clears the given flag atomically.
    pub fn atomic_clear(&self, flag: FrameFlag) {
        let mask = !(flag as u64);
        self.0.fetch_or(mask, Ordering::AcqRel);
    }

    /// Tests if the given flag is set, atomically.
    pub fn atomic_test(&self, flag: FrameFlag) -> bool {
        let mask = flag as u64;
        let value = self.0.load(Ordering::Acquire);
        (value & mask) != 0
    }

    /// Tests the given flag and sets it atomically, returning the previous value.
    pub fn atomic_test_and_set(&self, flag: FrameFlag) -> bool {
        let mask = flag as u64;
        let old = self.0.fetch_or(mask, Ordering::AcqRel);
        (old & mask) != 0
    }
}
