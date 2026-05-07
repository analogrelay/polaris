use core::{cell::UnsafeCell, mem::MaybeUninit};

use pmm::VirtualAddress;
use x86_64::{
    VirtAddr,
    registers::model_specific::{GsBase, KernelGsBase},
};

/// For now, a single CPU context is stored in a static.
/// When SMP is supported, we'll store a list of heap pointers to CPU contexts.
static mut CPU0_CONTEXT: CpuContext = CpuContext {
    user_rsp: VirtualAddress::ZERO,
    kernel_rsp: VirtualAddress::ZERO,
    this: VirtAddr::zero(),
};

/// Uniquely identifies a CPU in a multi-CPU system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuNumber(pub(super) usize);

/// Represents a CPU's current task context.
#[repr(C)] // We need a consistent repr so we can access fields by offset in assembly
pub struct CpuContext {
    user_rsp: VirtualAddress,
    kernel_rsp: VirtualAddress,

    // This is a pointer back to the CpuContextInner struct itself, used for self-reference.
    // SAFETY: We must ensure that the CpuContextInner is never moved or dropped.
    this: VirtAddr,
}

impl CpuContext {
    fn current() -> &'static Self {
        // SAFETY: After initialization with `init`, the GS base is set to the CPU context.
        let this: *const Self;
        unsafe {
            core::arch::asm! {
                "mov {}, gs:[16]",
                out(reg) this,
            }
            &*this
        }
    }
}

/// Initialize the current CPU.
///
/// # Arguments
/// * `cpu_number` - The number of the current CPU.
pub fn init(cpu_number: CpuNumber) {
    if cpu_number.0 != 0 {
        panic!("SMP not supported yet");
    }

    unsafe {
        // SAFETY: We're running on a single CPU, so the CPU context is static and never moved.
        let this = VirtAddr::from_ptr(&raw const CPU0_CONTEXT);
        CPU0_CONTEXT.this = this;
        KernelGsBase::write(this);
        GsBase::write(VirtAddr::zero());
    }
}
