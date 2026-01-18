use crate::{
    arch,
    modules::{Module, ModuleName},
};
use symbolicator::SymbolTable;

#[derive(Debug, Clone, Copy)]
struct StackFrame {
    caller: *const StackFrame,
    return_address: usize,
}

fn is_valid_stack_frame(frame_ptr: *const StackFrame) -> bool {
    if frame_ptr.is_null() {
        return false;
    }

    // Check alignment
    if (frame_ptr as usize) % core::mem::align_of::<usize>() != 0 {
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

    let mut rbp: *const StackFrame;
    unsafe {
        core::arch::asm!("mov {}, rbp", out(reg) rbp);
    }

    let mut depth = 0;
    let mut prev_rbp = core::ptr::null();

    while depth < max_depth {
        if !is_valid_stack_frame(rbp) {
            return;
        }

        if rbp == prev_rbp {
            return;
        }

        let frame = unsafe { *rbp };

        if frame.return_address == 0 {
            return;
        }

        const KERNEL_START: usize = 0xffffffff80000000;
        const KERNEL_MAX: usize = 0xffffffff81000000;

        if frame.return_address < KERNEL_START || frame.return_address >= KERNEL_MAX {
            return;
        }

        // Subtract 1 from return address to point inside the call instruction
        // rather than the instruction after it.
        let call_site = frame.return_address - 1;

        if let Some(info) = symtab.as_ref().and_then(|s| s.lookup(call_site as u64)) {
            log::error!(
                "  #{}: {:#x} - {} at {}:{}",
                depth,
                call_site,
                info.function_name,
                info.source_file,
                info.line
            );
        } else {
            log::error!("  #{}: {:#x}", depth, call_site);
        }

        prev_rbp = rbp;
        rbp = frame.caller;
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
    fn panic_time() -> ! {
        log::debug!("about to panic");
        panic!("This is a test panic for stack unwinding");
    }

    #[inline(never)]
    fn nested() -> ! {
        log::debug!("in nested function");
        panic_time();
    }

    nested();
}
