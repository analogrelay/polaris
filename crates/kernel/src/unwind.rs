use crate::{
    arch,
    image::LinkerSection,
    interrupts::{self, InterruptContext},
    mem::MemoryArea,
    modules::{Module, ModuleName},
};
use gimli::{
    BaseAddresses, CfaRule, EhFrame, EhFrameHdr, EndianSlice, NativeEndian, ParsedEhFrameHdr,
    UnwindContext, UnwindSection,
};
use symbolicator::SymbolTable;

fn load_symbol_table() -> Option<SymbolTable<'static>> {
    let module = Module::get(ModuleName::DEBUG_SYMBOLS)?;
    let data = unsafe { core::slice::from_raw_parts(module.base as *const u8, module.size) };
    SymbolTable::from_bytes(data).ok()
}

pub fn unwind_stack(state: arch::UnwindState) {
    let eh_frame = unsafe { LinkerSection::EhFrame.as_bytes() };
    let eh_frame_hdr = unsafe { LinkerSection::EhFrameHdr.as_bytes() };
    let text_base = LinkerSection::Text.as_u64();
    if eh_frame.is_empty() {
        log::warn!("no .eh_frame section found for stack unwinding");
    } else {
        let mut bases = BaseAddresses::default()
            .set_text(text_base)
            .set_eh_frame(eh_frame.as_ptr() as u64);
        let eh_frame = EhFrame::new(&eh_frame, NativeEndian);
        let eh_frame_hdr = if eh_frame_hdr.is_empty() {
            None
        } else {
            bases = bases.set_eh_frame_hdr(eh_frame_hdr.as_ptr() as u64);
            Some(gimli::EhFrameHdr::new(eh_frame_hdr, NativeEndian))
        };
        let unwinder = StackUnwinder::for_current_frame(state, bases, eh_frame, eh_frame_hdr);
        let symbol_table = load_symbol_table();
        for frame in unwinder {
            match frame {
                StackFrame::Interrupt(context) => {
                    log::error!(
                        " at cpu interrupt, vector={}, error_code={:?}",
                        context.vector(),
                        context.error_code(),
                    );
                }
                StackFrame::Standard {
                    instruction_pointer,
                } => {
                    if let Some(symbol_table) = &symbol_table {
                        if let Some(symbol) = symbol_table.lookup(instruction_pointer) {
                            log::error!(
                                " at {:#018x} {} ({}:{})",
                                instruction_pointer,
                                symbol.function_name,
                                symbol.source_file,
                                symbol.line,
                            );
                            continue;
                        }
                    }
                    log::error!(" at {:#018x} <unknown>", instruction_pointer);
                }
            }
        }
    }
}

pub fn handle_panic(info: &core::panic::PanicInfo, state: arch::UnwindState) -> ! {
    log::error!("PICNIC: {}", info.message());
    if let Some(location) = info.location() {
        log::error!(" at {}", location)
    }

    unwind_stack(state);

    log::error!("CPU parked");
    arch::park();
}

#[inline(never)]
pub fn test() -> ! {
    #[inline(never)]
    fn panic_time() {
        log::debug!("about to panic");
        unsafe {
            core::arch::asm!("int3");
        }
    }

    #[inline(never)]
    fn nested() {
        log::debug!("in nested function");
        panic_time();
    }

    nested();
    arch::park();
}

pub enum StackFrame {
    Standard { instruction_pointer: u64 },
    Interrupt(InterruptContext),
}

pub struct StackUnwinder {
    state: arch::UnwindState,
    bases: BaseAddresses,
    ctx: UnwindContext<usize>,
    eh_frame: EhFrame<EndianSlice<'static, NativeEndian>>,
    eh_frame_hdr: Option<ParsedEhFrameHdr<EndianSlice<'static, gimli::LittleEndian>>>,
    last_was_interrupt: bool,
}

impl StackUnwinder {
    /// Creates a new unwinder for the current stack frame.
    ///
    /// This is never inlined so that we can skip this frame when unwinding.
    /// This allows the [`next`] method to return the caller's frame.
    #[inline(never)]
    pub fn for_current_frame(
        state: arch::UnwindState,
        bases: BaseAddresses,
        eh_frame: EhFrame<EndianSlice<'static, NativeEndian>>,
        eh_frame_hdr: Option<EhFrameHdr<EndianSlice<'static, NativeEndian>>>,
    ) -> Self {
        let eh_frame_hdr = eh_frame_hdr
            .map(|hdr| hdr.parse(&bases, (usize::BITS / 8) as u8).ok())
            .flatten();
        Self {
            state,
            bases,
            ctx: UnwindContext::new(),
            eh_frame,
            eh_frame_hdr,
            last_was_interrupt: false,
        }
    }
}

impl Iterator for StackUnwinder {
    type Item = StackFrame;

    fn next(&mut self) -> Option<Self::Item> {
        if self.last_was_interrupt {
            // The last thing we returned was the synthetic interrupt frame.
            // So the state is pointing at the interrupted context and we can just return that.
            self.last_was_interrupt = false;
            return Some(StackFrame::Standard {
                instruction_pointer: self.state.instruction_pointer(),
            });
        }

        let ip = self.state.instruction_pointer();
        if let Some(LinkerSection::InterruptHandlers) = LinkerSection::containing(ip.into()) {
            // Try to pop an interrupt context
            if let Some(context) = interrupts::take_current_interrupt_context() {
                // Use the interrupt context to make the next frame
                // Return an synthetic frame for the interrupt
                self.state = arch::UnwindState::from_interrupt(context.clone());
                self.last_was_interrupt = true;
                return Some(StackFrame::Interrupt(context));
            }

            // no interrupt context found, halt unwinding
            log::warn!("no interrupt context found during unwinding at {:#x}", ip,);
            return None;
        }
        let Some(fde) = get_fde(&self.eh_frame, &self.eh_frame_hdr, &self.bases, ip) else {
            log::warn!("no FDE found for address {:#x} during unwinding", ip,);
            return None;
        };
        let Ok(unwind_row) = fde.unwind_info_for_address(
            &self.eh_frame,
            &self.bases,
            &mut self.ctx,
            self.state.instruction_pointer(),
        ) else {
            log::warn!(
                "no unwind info found for address {:#x} during unwinding",
                self.state.instruction_pointer()
            );
            return None;
        };
        let cfa = match unwind_row.cfa() {
            CfaRule::RegisterAndOffset { register, offset } => self.state.cfa(*register, *offset),
            x => {
                log::warn!("unsupported CFA rule encountered during unwinding: {:?}", x);
                return None;
            }
        };
        let state = self.state.next_frame(cfa, &unwind_row);
        if let Some(state) = state {
            self.state = state;
            Some(StackFrame::Standard {
                instruction_pointer: self.state.instruction_pointer(),
            })
        } else {
            None
        }
    }
}

fn get_fde(
    eh_frame: &EhFrame<EndianSlice<'static, NativeEndian>>,
    eh_frame_hdr: &Option<ParsedEhFrameHdr<EndianSlice<'static, NativeEndian>>>,
    bases: &BaseAddresses,
    address: u64,
) -> Option<gimli::FrameDescriptionEntry<EndianSlice<'static, NativeEndian>, usize>> {
    // Try the EH frame header first for faster lookups
    if let Some(eh_frame_hdr) = eh_frame_hdr {
        if let Some(table) = eh_frame_hdr.table() {
            let ptr = match table.lookup(address, bases) {
                Ok(ptr) => ptr,
                Err(e) => {
                    log::warn!(
                        "error looking up FDE in .eh_frame_hdr for address {:#x}: {:?}",
                        address,
                        e
                    );
                    return None;
                }
            };
            let offset = match table.pointer_to_offset(ptr) {
                Ok(offset) => offset,
                Err(e) => {
                    log::warn!(
                        "error converting pointer to offset in .eh_frame_hdr for address {:#x}: {:?}",
                        address,
                        e
                    );
                    return None;
                }
            };
            let fde = match eh_frame.fde_from_offset(bases, offset, EhFrame::cie_from_offset) {
                Ok(fde) => fde,
                Err(e) => {
                    log::warn!(
                        "error loading FDE from .eh_frame for address {:#x}: {:?}",
                        address,
                        e
                    );
                    return None;
                }
            };
            return Some(fde);
        }
    }

    if let Ok(fde) = eh_frame.fde_for_address(bases, address, EhFrame::cie_from_offset) {
        return Some(fde);
    }

    None
}

#[inline(always)]
pub fn capture_unwind_state() -> arch::UnwindState {
    arch::UnwindState::capture()
}
