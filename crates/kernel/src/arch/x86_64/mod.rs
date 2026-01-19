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

/// Magic number to identify a valid InterruptUnwindContext on the stack.
pub const INTERRUPT_UNWIND_MAGIC: usize = 0xDEAD_C0DE_CAFE_BABE;

/// Context saved when an interrupt occurs, for stack unwinding.
///
/// Placed on the stack in `common_interrupt` so the unwinder can find it
/// by scanning below the frame pointer. The magic number verifies we found
/// a valid context.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptUnwindContext {
    pub magic: usize,
    pub vector: u8,
    pub interrupted_rip: usize,
    pub interrupted_rbp: usize,
}

impl InterruptUnwindContext {
    /// Checks if this context has a valid magic number.
    pub fn is_valid(&self) -> bool {
        self.magic == INTERRUPT_UNWIND_MAGIC
    }
}

/// Attempts to find an InterruptUnwindContext on the stack given the frame pointer
/// of the common_interrupt function.
///
/// Scans a small range below RBP looking for the magic number.
pub fn find_interrupt_unwind_context(
    common_interrupt_rbp: usize,
) -> Option<InterruptUnwindContext> {
    // Scan middle-out from 0x128 below RBP, which is where the context is typically found.
    const EXPECTED_OFFSET: usize = 0x128;
    const SCAN_RANGE: usize = 512;
    const STEP: usize = core::mem::size_of::<usize>();
    const MAX_STEPS: usize = SCAN_RANGE / STEP;

    let start_addr = common_interrupt_rbp.wrapping_sub(EXPECTED_OFFSET);

    for i in 0..MAX_STEPS {
        // Alternate between scanning below and above the expected offset
        let offset = (i / 2) * STEP;
        let addr = if i % 2 == 0 {
            start_addr.wrapping_sub(offset)
        } else {
            start_addr.wrapping_add(offset)
        };

        let candidate = unsafe { &*(addr as *const InterruptUnwindContext) };
        if candidate.is_valid() {
            return Some(*candidate);
        }
    }

    None
}

// Linker symbols marking the bounds of the interrupt handlers section.
unsafe extern "C" {
    static __interrupt_handlers_start: u8;
    static __interrupt_handlers_end: u8;
}

/// Returns the address range of the interrupt handlers section.
pub fn interrupt_handlers_range() -> (usize, usize) {
    unsafe {
        (
            &__interrupt_handlers_start as *const _ as usize,
            &__interrupt_handlers_end as *const _ as usize,
        )
    }
}

/// Checks if an address is within the interrupt handlers section.
pub fn is_in_interrupt_handler(addr: usize) -> bool {
    let (start, end) = interrupt_handlers_range();
    addr >= start && addr < end
}

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

/// Common interrupt handler called by all interrupt stubs.
///
/// The unwinder detects this function by address and finds the
/// InterruptUnwindContext on the stack by scanning for the magic number.
#[inline(never)]
#[unsafe(link_section = ".interrupt_handlers")]
fn common_interrupt(stack_frame: InterruptStackFrame, vector: u8, error_code: Option<u64>) {
    // Place the unwind context as the first local variable.
    // The unwinder will find this by scanning below our RBP for the magic number.
    //
    // The x86-interrupt handler saved RBP in its prologue - that value is the
    // interrupted code's RBP, which we need to continue unwinding.
    let interrupted_rbp: usize;
    unsafe {
        core::arch::asm!("mov {}, [rbp]", out(reg) interrupted_rbp, options(nostack, readonly));
    }

    // Build and dispatch the interrupt context.
    let ip = stack_frame.instruction_pointer.as_u64() as usize;
    let state = InterruptState {
        stack_frame,
        error_code,
    };
    let interrupt_vector = InterruptVector::new(vector);
    let kind = match interrupt_vector {
        InterruptVector::PAGE_FAULT => {
            let faulting_address = x86_64::registers::control::Cr2::read()
                .ok()
                .map(|v| v.as_u64().into());
            InterruptKind::PageFault { faulting_address }
        }
        _ => InterruptKind::Standard,
    };

    // Use black_box to ensure the compiler doesn't optimize away this local.
    // The unwinder needs to find it on the stack.
    let _unwind_context = core::hint::black_box(InterruptUnwindContext {
        magic: INTERRUPT_UNWIND_MAGIC,
        vector,
        interrupted_rip: ip,
        interrupted_rbp,
    });
    log::trace!("created InterruptUnwindContext at: {:p}", &_unwind_context);
    interrupt_was_received(InterruptContext::new(interrupt_vector, state, kind));
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
