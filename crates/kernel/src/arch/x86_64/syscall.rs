#[repr(C)]
struct SyscallFrame {
    number: u64,
    args: [u64; 6],
}

#[no_mangle]
pub extern "C" fn syscall_dispatch(frame: &SyscallFrame) -> u64 {
    crate::syscall::dispatch(SyscallNumber::new(frame.number as usize), &frame.args)
}

#[naked]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Coming from userspace via `syscall`.
        // RCX = user RIP, R11 = user RFLAGS, RSP = user stack.
        "swapgs",
        "mov gs:[0], rsp", // save user RSP into CpuContext.user_rsp
        "mov rsp, gs:[8]", // switch to kernel RSP from CpuContext.kernel_rsp
        // Build SyscallFrame on the kernel stack.
        // Push in reverse order so lowest address = first field (number).
        "push r9",  // args[5]
        "push r8",  // args[4]
        "push r10", // args[3]  (not rcx — that's user RIP)
        "push rdx", // args[2]
        "push rsi", // args[1]
        "push rdi", // args[0]
        "push rax", // number
        // RSP now points at a valid SyscallFrame.
        "mov rdi, rsp",
        "call syscall_dispatch",
        // RAX holds the return value — leave it alone.
        "add rsp, 8*7", // discard SyscallFrame
        // Restore user state and return.
        // sysretq restores RIP from RCX and RFLAGS from R11.
        // Both were saved by the `syscall` instruction itself and
        // we haven't touched them, so they're still intact.
        "mov rsp, gs:[0]", // restore user RSP
        "swapgs",
        "sysretq",
    );
}
