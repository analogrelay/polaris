//! Contains interrupt handlers that route to the common_interrupt function in mod.rs

use super::common_interrupt;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// Exception handlers (vectors 0-31)

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(0, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn debug_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(1, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn nmi_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(2, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(3, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn overflow_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(4, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn bound_range_exceeded_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(5, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(6, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn device_not_available_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(7, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    common_interrupt(8, stack_frame, Some(error_code));
    panic!("returned from double fault handler");
}

// Vector 9 is reserved (coprocessor segment overrun, no longer used)
#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn coprocessor_segment_overrun_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(9, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn invalid_tss_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    common_interrupt(10, stack_frame, Some(error_code));
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn segment_not_present_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(11, stack_frame, Some(error_code));
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(12, stack_frame, Some(error_code));
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(13, stack_frame, Some(error_code));
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    common_interrupt(14, stack_frame, Some(error_code.bits() as u64));
}

// Vector 15 is reserved

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn x87_floating_point_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(16, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn alignment_check_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(17, stack_frame, Some(error_code));
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn machine_check_handler(stack_frame: InterruptStackFrame) -> ! {
    common_interrupt(18, stack_frame, None);
    panic!("returned from machine check handler");
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn simd_floating_point_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(19, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn virtualization_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(20, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn control_protection_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(21, stack_frame, Some(error_code));
}

// Vectors 22-27 are reserved

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn hypervisor_injection_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(28, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn vmm_communication_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(29, stack_frame, Some(error_code));
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn security_exception_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    common_interrupt(30, stack_frame, Some(error_code));
}

// Vector 31 is reserved

// IRQ handlers (vectors 32-255)
// Typically IRQs are mapped starting at vector 32

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq0_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(32, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq1_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(33, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq2_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(34, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq3_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(35, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq4_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(36, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq5_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(37, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq6_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(38, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq7_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(39, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq8_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(40, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq9_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(41, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq10_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(42, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq11_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(43, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq12_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(44, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq13_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(45, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq14_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(46, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq15_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(47, stack_frame, None);
}

// Additional IRQ handlers for APIC and other devices (vectors 48-255)
// These are commonly used for APIC timer, IPIs, etc.

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq16_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(48, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq17_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(49, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq18_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(50, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq19_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(51, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq20_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(52, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq21_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(53, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq22_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(54, stack_frame, None);
}

#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn irq23_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(55, stack_frame, None);
}

// Spurious interrupt handler (commonly vector 255)
#[unsafe(link_section = ".interrupt_handlers")]
extern "x86-interrupt" fn spurious_handler(stack_frame: InterruptStackFrame) {
    common_interrupt(255, stack_frame, None);
}

pub fn register_handlers(idt: &mut InterruptDescriptorTable, double_fault_ist_index: u16) {
    idt.divide_error.set_handler_fn(divide_error_handler);
    idt.debug.set_handler_fn(debug_handler);
    idt.non_maskable_interrupt.set_handler_fn(nmi_handler);
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.overflow.set_handler_fn(overflow_handler);
    idt.bound_range_exceeded
        .set_handler_fn(bound_range_exceeded_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.device_not_available
        .set_handler_fn(device_not_available_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(double_fault_ist_index);
    }
    idt[9].set_handler_fn(coprocessor_segment_overrun_handler);
    idt.invalid_tss.set_handler_fn(invalid_tss_handler);
    idt.segment_not_present
        .set_handler_fn(segment_not_present_handler);
    idt.stack_segment_fault
        .set_handler_fn(stack_segment_fault_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.x87_floating_point
        .set_handler_fn(x87_floating_point_handler);
    idt.alignment_check.set_handler_fn(alignment_check_handler);
    idt.machine_check.set_handler_fn(machine_check_handler);
    idt.simd_floating_point
        .set_handler_fn(simd_floating_point_handler);
    idt.virtualization.set_handler_fn(virtualization_handler);
    idt.cp_protection_exception
        .set_handler_fn(control_protection_handler);
    idt.hv_injection_exception
        .set_handler_fn(hypervisor_injection_handler);
    idt.vmm_communication_exception
        .set_handler_fn(vmm_communication_handler);
    idt.security_exception
        .set_handler_fn(security_exception_handler);
    idt[32].set_handler_fn(irq0_handler);
    idt[33].set_handler_fn(irq1_handler);
    idt[34].set_handler_fn(irq2_handler);
    idt[35].set_handler_fn(irq3_handler);
    idt[36].set_handler_fn(irq4_handler);
    idt[37].set_handler_fn(irq5_handler);
    idt[38].set_handler_fn(irq6_handler);
    idt[39].set_handler_fn(irq7_handler);
    idt[40].set_handler_fn(irq8_handler);
    idt[41].set_handler_fn(irq9_handler);
    idt[42].set_handler_fn(irq10_handler);
    idt[43].set_handler_fn(irq11_handler);
    idt[44].set_handler_fn(irq12_handler);
    idt[45].set_handler_fn(irq13_handler);
    idt[46].set_handler_fn(irq14_handler);
    idt[47].set_handler_fn(irq15_handler);
    idt[48].set_handler_fn(irq16_handler);
    idt[49].set_handler_fn(irq17_handler);
    idt[50].set_handler_fn(irq18_handler);
    idt[51].set_handler_fn(irq19_handler);
    idt[52].set_handler_fn(irq20_handler);
    idt[53].set_handler_fn(irq21_handler);
    idt[54].set_handler_fn(irq22_handler);
    idt[55].set_handler_fn(irq23_handler);
    idt[255].set_handler_fn(spurious_handler);
}
