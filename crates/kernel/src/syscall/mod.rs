#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallNumber(usize);

impl SyscallNumber {
    pub const fn new(number: usize) -> Self {
        Self(number)
    }
}

impl core::fmt::Display for SyscallNumber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[inline(always)]
pub fn dispatch(number: SyscallNumber, args: &[u64; 6]) -> u64 {
    panic!("syscall {} not implemented", number)
}
