use pmm::VirtualAddress;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, InterruptStackFrameValue,
};

use crate::interrupts::{InterruptContext, InterruptKind, interrupt_was_received};

mod handlers;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
static IDT: spin::Once<InterruptDescriptorTable> = spin::Once::new();

pub fn idt() -> &'static InterruptDescriptorTable {
    IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        handlers::register_handlers(&mut idt, DOUBLE_FAULT_IST_INDEX);
        idt
    })
}

/// Common interrupt handler called by all interrupt stubs.
///
/// The unwinder detects this function by address and finds the
/// InterruptUnwindContext on the stack by scanning for the magic number.
fn common_interrupt(vector: u8, stack_frame: InterruptStackFrame, error_code: Option<u64>) {
    let vector = InterruptVector::new(vector);
    // Build and dispatch the interrupt context.
    let state = InterruptState::new(stack_frame, error_code);
    let kind = match vector {
        InterruptVector::PAGE_FAULT => {
            let faulting_address = x86_64::registers::control::Cr2::read()
                .ok()
                .map(|v| v.as_u64().into());
            InterruptKind::PageFault { faulting_address }
        }
        _ => InterruptKind::Standard,
    };

    interrupt_was_received(InterruptContext::new(vector, state, kind));
}

crate::interrupt_vectors! {
    u8,
    DIVIDE_ERROR = 0,
    DEBUG = 1,
    NON_MASKABLE_INTERRUPT = 2,
    BREAKPOINT = 3,
    OVERFLOW = 4,
    BOUND_RANGE_EXCEEDED = 5,
    INVALID_OPCODE = 6,
    DEVICE_NOT_AVAILABLE = 7,
    DOUBLE_FAULT = 8,
    COPROCESSOR_SEGMENT_OVERRUN = 9,
    INVALID_TSS = 10,
    SEGMENT_NOT_PRESENT = 11,
    STACK_SEGMENT_FAULT = 12,
    GENERAL_PROTECTION_FAULT = 13,
    PAGE_FAULT = 14,
    X87_FLOATING_POINT_EXCEPTION = 16,
    ALIGNMENT_CHECK = 17,
    MACHINE_CHECK = 18,
    SIMD_FLOATING_POINT_EXCEPTION = 19,
    VIRTUALIZATION_EXCEPTION = 20,
    CP_PROTECTION_EXCEPTION = 21,
}

#[derive(Debug, Clone)]
pub struct InterruptState {
    stack_frame: InterruptStackFrameValue,
    error_code: Option<u64>,
}

impl InterruptState {
    pub fn new(stack_frame: InterruptStackFrame, error_code: Option<u64>) -> Self {
        Self {
            stack_frame: stack_frame.clone(),
            error_code,
        }
    }

    pub fn instruction_pointer(&self) -> VirtualAddress {
        VirtualAddress::new(self.stack_frame.instruction_pointer.as_u64() as usize)
    }

    pub fn stack_pointer(&self) -> VirtualAddress {
        VirtualAddress::new(self.stack_frame.stack_pointer.as_u64() as usize)
    }

    pub fn error_code(&self) -> Option<u64> {
        self.error_code
    }
}
