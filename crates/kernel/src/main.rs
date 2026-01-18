// cSpell:ignore kmain

#![cfg_attr(not(test), no_std)]
#![no_main]

#[cfg(not(test))]
#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    polaris_kernel::handle_panic(info)
}
