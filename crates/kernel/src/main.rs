// cSpell:ignore kmain

#![cfg_attr(not(test), no_std)]
#![no_main]

#[cfg(not(test))]
#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    let state = polaris_kernel::capture_unwind_state();
    polaris_kernel::handle_panic(info, state)
}
