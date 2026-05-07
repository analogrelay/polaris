#![no_std]
#![no_main]

#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!("mov rax, 0", "syscall", "jmp _start",);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
