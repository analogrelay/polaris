use x86_64::{
    VirtAddr,
    instructions::tables::load_tss,
    registers::segmentation::{CS, Segment},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
};

mod interrupts;
mod unwind;

pub use interrupts::{InterruptState, InterruptVector};
pub use unwind::UnwindState;

/// Returns true if the given address is in user space (lower half).
///
/// On x86_64, user space occupies the lower half of the virtual address space
/// (addresses below 0x8000_0000_0000_0000).
pub fn is_user_space(addr: usize) -> bool {
    addr < 0x8000_0000_0000_0000
}

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

fn tss() -> &'static TaskStateSegment {
    TSS.call_once(|| {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[interrupts::DOUBLE_FAULT_IST_INDEX as usize] = {
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
    interrupts::idt().load();
}
