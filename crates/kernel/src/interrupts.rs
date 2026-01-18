use pmm::VirtualAddress;

use crate::arch;

#[derive(Debug)]
pub struct InterruptContext {
    vector: arch::InterruptVector,
    state: arch::InterruptState,
    kind: InterruptKind,
}

impl InterruptContext {
    pub fn new(
        vector: arch::InterruptVector,
        state: arch::InterruptState,
        kind: InterruptKind,
    ) -> Self {
        Self {
            vector,
            state,
            kind,
        }
    }

    /// Returns the instruction pointer at the time of the interrupt.
    pub fn instruction_pointer(&self) -> VirtualAddress {
        self.state.instruction_pointer()
    }

    /// Returns the stack pointer at the time of the interrupt.
    pub fn stack_pointer(&self) -> VirtualAddress {
        self.state.stack_pointer()
    }

    /// Returns the error code associated with the interrupt, if any.
    pub fn error_code(&self) -> Option<u64> {
        self.state.error_code()
    }

    /// Returns the kind of interrupt.
    pub fn kind(&self) -> &InterruptKind {
        &self.kind
    }
}

#[derive(Debug)]
pub enum InterruptKind {
    Standard,
    PageFault {
        faulting_address: Option<VirtualAddress>,
    },
}

pub fn interrupt_was_received(context: InterruptContext) {
    log::trace!("interrupt received: {:?}", context);
    panic!("Unhandled interrupt");
}

#[macro_export]
macro_rules! interrupt_vectors {
    (
        $storage: ty,
        $(
            $name:ident = $value:expr,
        )*
    ) => {
        /// Represents an interrupt vector.
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        pub struct InterruptVector($storage);

        impl InterruptVector {
            $(
                pub const $name: Self = Self($value);
            )*

            /// Creates a new interrupt vector from a raw value.
            pub const fn new(value: $storage) -> Self {
                Self(value)
            }

            /// Returns the raw value of the interrupt vector.
            pub const fn value(&self) -> $storage {
                self.0
            }

            /// Returns the name of the interrupt vector, if known.
            pub fn name(&self) -> Option<&'static str> {
                match self.0 {
                    $(
                        $value => Some(stringify!($name)),
                    )*
                    _ => None,
                }
            }
        }

        impl core::fmt::Debug for InterruptVector {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                if let Some(name) = self.name() {
                    write!(f, "InterruptVector::{}({})", name, self.0)
                } else {
                    write!(f, "InterruptVector({})", self.0)
                }
            }
        }

        impl core::fmt::Display for InterruptVector {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                if let Some(name) = self.name() {
                    write!(f, "{}", name)
                } else {
                    write!(f, "{}", self.0)
                }
            }
        }
    }
}
