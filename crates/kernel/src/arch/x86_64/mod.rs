use pmm::VirtualAddress;
use x86_64::{
    VirtAddr,
    instructions::tables::load_tss,
    registers::segmentation::{CS, Segment},
    set_general_handler,
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        idt::{InterruptDescriptorTable, InterruptStackFrame},
        tss::TaskStateSegment,
    },
};

use crate::{
    arch,
    interrupts::{InterruptContext, InterruptKind, interrupt_was_received},
};

/// Returns true if the given address is in user space (lower half).
///
/// On x86_64, user space occupies the lower half of the virtual address space
/// (addresses below 0x8000_0000_0000_0000).
pub fn is_user_space(addr: usize) -> bool {
    addr < 0x8000_0000_0000_0000
}

const DOUBLE_FAULT_IST_INDEX: usize = 0;
static IDT: spin::Once<InterruptDescriptorTable> = spin::Once::new();
static TSS: spin::Once<TaskStateSegment> = spin::Once::new();
static GDT: spin::Once<(GlobalDescriptorTable, Selectors)> = spin::Once::new();

/// The architecture-specific entry point
///
/// This function is responsible for capturing the stack start address
/// and calling the main kernel entry point.
#[unsafe(no_mangle)]
pub extern "C" fn kenter() -> ! {
    let stack_start = unsafe {
        // SAFETY: The bootloader sets up the stack pointer before transferring control
        // to the kernel entry point. We can read the current stack pointer safely here.
        let rsp: u64;
        core::arch::asm!("mov {}, rsp", out(reg) rsp);
        rsp as usize
    };
    crate::kernel_main(stack_start)
}

fn idt() -> &'static InterruptDescriptorTable {
    IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        set_general_handler!(&mut idt, common_interrupt);

        // Override double fault handler, so that we can use a dedicated stack.
        unsafe {
            idt.double_fault
                .set_handler_fn({
                    extern "x86-interrupt" fn double_fault_handler(
                        stack_frame: InterruptStackFrame,
                        error_code: u64,
                    ) -> ! {
                        common_interrupt(stack_frame, 8, Some(error_code));

                        arch::park();
                    }
                    double_fault_handler
                })
                .set_stack_index(DOUBLE_FAULT_IST_INDEX as u16)
        };

        idt
    })
}

fn tss() -> &'static TaskStateSegment {
    TSS.call_once(|| {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let start = VirtAddr::from_ptr(&raw const STACK);
            let end = start + STACK_SIZE as u64;
            end
        };
        tss
    })
}

fn gdt() -> (&'static GlobalDescriptorTable, &'static Selectors) {
    let (gdt, selectors) = GDT.call_once(|| {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(tss()));
        let selectors = Selectors {
            code_selector,
            tss_selector,
        };
        (gdt, selectors)
    });
    (gdt, selectors)
}

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    let (gdt, selectors) = gdt();
    gdt.load();
    unsafe {
        CS::set_reg(selectors.code_selector);
        load_tss(selectors.tss_selector);
    }
    idt().load();
}

fn common_interrupt(stack_frame: InterruptStackFrame, vector: u8, error_code: Option<u64>) {
    let state = InterruptState {
        stack_frame,
        error_code,
    };
    let vector = InterruptVector::new(vector);
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

#[derive(Debug)]
pub struct InterruptState {
    stack_frame: InterruptStackFrame,
    error_code: Option<u64>,
}

impl InterruptState {
    pub fn new(stack_frame: InterruptStackFrame, error_code: Option<u64>) -> Self {
        Self {
            stack_frame,
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
