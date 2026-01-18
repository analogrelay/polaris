//! Architecture-specific implementations for memory management.
//!
//! This module conditionally imports either hardware-specific implementations
//! or software emulation based on the target architecture and features.

// Use x86_64 hardware implementation when we're on x86_64 and not testing or emulating.
// NOTE: We DO include the module even during tests so that rust-analyzer can see it.
#[cfg(all(target_arch = "x86_64"))]
mod x86_64;
#[cfg(all(target_arch = "x86_64", not(test), not(feature = "software-emulation")))]
pub use x86_64::*;

// Use software emulation ONLY when:
// - Running tests, OR
// - software-emulation feature is explicitly enabled
#[cfg(any(test, feature = "software-emulation"))]
mod software;
#[cfg(any(test, feature = "software-emulation"))]
pub use software::*;

// Re-export page table primitives from the active architecture
pub use self::{PageEntry, PageFlags, PageTable};
