// cSpell:ignore kmain

#![cfg_attr(not(test), no_std)]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]
#![feature(allocator_api)]

extern crate alloc;

#[cfg(feature = "acpi")]
mod acpi;
mod arch;
mod console;
mod framebuffer;
mod image;
mod interrupts;
mod mem;
mod modules;
mod serial;
mod unwind;

use limine::BaseRevision;

pub use unwind::{capture_unwind_state, handle_panic};

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(4);

pub fn kernel_main(stack_start: usize) -> ! {
    assert!(BASE_REVISION.is_supported());

    // SAFETY: We're only calling this once, before any other CPUs are running.
    unsafe {
        mem::set_stack_bounds(stack_start);
    }

    let console = console::Console::init();
    serial::init(console);
    framebuffer::init(console);
    arch::init();

    mem::init_allocator();
    log::debug!("Block allocator initialized");

    let pmm = mem::init_pmm();
    mem::use_pmm(pmm);
    log::debug!("Physical Memory Manager initialized and in use");

    unwind::test();
}
