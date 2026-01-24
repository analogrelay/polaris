use gimli::{RegisterRule, UnwindTableRow, X86_64};
use x86_64::structures::idt::InterruptStackFrame;

use crate::{image::LinkerSection, interrupts::InterruptContext};

pub struct UnwindState {
    rip: u64,
    rsp: u64,
}

impl UnwindState {
    #[inline(always)]
    pub fn capture() -> Self {
        let rip: u64;
        let rsp: u64;
        unsafe {
            core::arch::asm!(
                "lea {}, [rip]",
                "mov {}, rsp",
                out(reg) rip,
                out(reg) rsp
            );
        }
        Self { rip, rsp }
    }

    pub fn from_interrupt(context: InterruptContext) -> Self {
        Self {
            rip: context.instruction_pointer().as_usize() as u64,
            rsp: context.stack_pointer().as_usize() as u64,
        }
    }

    /// Returns the instruction pointer (RIP) of the current unwind state.
    pub fn instruction_pointer(&self) -> u64 {
        self.rip
    }

    /// Computes the Canonical Frame Address (CFA) based on the given register and offset.
    pub fn cfa(&self, register: gimli::Register, offset: i64) -> u64 {
        let value = match register {
            X86_64::RSP => self.rsp,
            x => panic!("unsupported register for CFA calculation: {:?}", x),
        };
        (value as i64 + offset) as u64
    }

    /// Returns the rule for the return address register.
    pub fn next_frame(&self, cfa: u64, unwind_row: &UnwindTableRow<usize>) -> Option<Self> {
        let rip_rule = unwind_row.register(X86_64::RA);
        let rip = match rip_rule {
            RegisterRule::Undefined => {
                log::warn!("return address is undefined during unwinding");
                return None;
            }
            RegisterRule::SameValue => self.rip,
            RegisterRule::Offset(offset) => {
                let addr = (cfa as i64 + offset) as *const u64;
                unsafe { core::ptr::read(addr) }
            }
            x => {
                log::warn!("unsupported return address rule during unwinding: {:?}", x);
                return None;
            }
        };

        if rip == 0 {
            return None;
        }

        Some(Self {
            // We subtract 1 from RIP to point to the call instruction itself, not the instruction after it.
            rip: rip - 1,
            rsp: cfa,
        })
    }
}

impl core::fmt::Debug for UnwindState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UnwindState")
            .field("rip", &format_args!("{:#x}", self.rip))
            .field("rsp", &format_args!("{:#x}", self.rsp))
            .finish()
    }
}
