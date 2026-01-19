use crate::{
    arch::{
        self,
        x86_64::{find_interrupt_unwind_context, is_in_interrupt_handler},
    },
    modules::{Module, ModuleName},
};
use symbolicator::SymbolTable;

/// Raw stack frame as laid out in memory by the compiler.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RawStackFrame {
    caller: usize,
    return_address: usize,
}

/// Represents a frame in the call stack during unwinding.
#[derive(Debug, Clone, Copy)]
enum StackFrame {
    /// A normal function call frame.
    Call { return_address: usize },
    /// A synthetic frame representing an interrupt boundary.
    Interrupt { vector: u8, interrupted_rip: usize },
}

fn is_valid_frame_pointer(ptr: usize) -> bool {
    if ptr == 0 {
        return false;
    }

    // Check alignment
    if ptr % core::mem::align_of::<usize>() != 0 {
        return false;
    }

    true
}

fn load_symbol_table() -> Option<SymbolTable<'static>> {
    let module = Module::get(ModuleName::DEBUG_SYMBOLS)?;
    let data = unsafe { core::slice::from_raw_parts(module.base as *const u8, module.size) };
    SymbolTable::from_bytes(data).ok()
}

/// Walks the stack and logs each frame with symbolicated information.
pub fn walk_stack(max_depth: usize) {
    let symtab = load_symbol_table();

    let mut rbp: usize;
    unsafe {
        core::arch::asm!("mov {}, rbp", out(reg) rbp);
    }

    let mut depth = 0;
    let mut prev_rbp = 0usize;

    while depth < max_depth {
        if !is_valid_frame_pointer(rbp) {
            return;
        }

        if rbp == prev_rbp {
            return;
        }

        let raw_frame = unsafe { *(rbp as *const RawStackFrame) };

        // Check if the return address is within the interrupt handlers section
        let (frame, next_rbp) = if is_in_interrupt_handler(raw_frame.return_address) {
            // We're returning into an interrupt handler - this means we crossed an interrupt boundary.
            // The caller RBP is common_interrupt's frame pointer. Scan its stack for the unwind context.
            if let Some(ctx) = find_interrupt_unwind_context(raw_frame.caller) {
                let frame = StackFrame::Interrupt {
                    vector: ctx.vector,
                    interrupted_rip: ctx.interrupted_rip,
                };
                // Continue walking from the interrupted code's RBP
                (frame, ctx.interrupted_rbp)
            } else {
                log::trace!(
                    "no InterruptUnwindContext found when unwinding from interrupt handler return address {:#x}",
                    raw_frame.return_address
                );
                // No valid context found, stop unwinding
                return;
            }
        } else {
            if raw_frame.return_address == 0 {
                return;
            }

            const KERNEL_START: usize = 0xffffffff80000000;
            const KERNEL_MAX: usize = 0xffffffff81000000;

            if raw_frame.return_address < KERNEL_START || raw_frame.return_address >= KERNEL_MAX {
                return;
            }

            let frame = StackFrame::Call {
                return_address: raw_frame.return_address,
            };
            (frame, raw_frame.caller)
        };

        // Log the frame
        match frame {
            StackFrame::Call { return_address } => {
                // Subtract 1 from return address to point inside the call instruction
                let call_site = return_address - 1;

                if let Some(info) = symtab.as_ref().and_then(|s| s.lookup(call_site as u64)) {
                    log::error!(
                        "  #{:02}: {:#x} - {} at {}:{}",
                        depth,
                        call_site,
                        info.function_name,
                        info.source_file,
                        info.line
                    );
                } else {
                    log::error!("  #{:02}: {:#x}", depth, call_site);
                }
            }
            StackFrame::Interrupt {
                vector,
                interrupted_rip,
            } => {
                log::error!("  #{:02}: <interrupt vector={}>", depth, vector,);
            }
        }

        prev_rbp = rbp;
        rbp = next_rbp;
        depth += 1;
    }
}

static PANICKING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

#[cfg(target_arch = "x86_64")]
pub fn handle_panic(info: &core::panic::PanicInfo) -> ! {
    if PANICKING.swap(true, core::sync::atomic::Ordering::SeqCst) {
        log::error!("RECURSIVE PANIC: {}", info.message());
        arch::park();
    }

    log::error!("PICNIC: {}", info.message());
    if let Some(location) = info.location() {
        log::error!(" at {}", location);
    }

    log::error!("Stack trace:");
    walk_stack(32);

    arch::park();
}

#[inline(never)]
pub fn test() -> ! {
    #[inline(never)]
    fn panic_time() {
        log::debug!("about to panic");

        // Trigger a breakpoint interrupt to cause a panic in an interrupt handler
        x86_64::instructions::interrupts::int3();
    }

    #[inline(never)]
    fn nested() {
        log::debug!("in nested function");
        panic_time();
    }

    nested();
    arch::park();
}
