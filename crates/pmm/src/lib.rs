#![cfg_attr(not(any(test, feature = "software-emulation")), no_std)]
#![feature(allocator_api)]
#![feature(slice_ptr_get)]
#![feature(step_trait)]

//! # Polaris Memory Manager (PMM)
//!
//! The Polaris Memory Manager (PMM) is a low-level memory management crate designed for
//! the Polaris operating system kernel. It provides:
//!
//! - Physical memory mapping and allocation.
//! - Virtual memory management.
//! - Support for multiple architectures, including x86_64, aarch64, riscv
//! - Software emulation for testing in non-kernel environments.

extern crate alloc;

mod address;
mod address_space;
mod arch;
mod block_allocator;
mod frame;
mod human_address;
mod human_size;
mod memmap;
mod numbers;
mod page_directory;
mod physical_memory_manager;

pub use address::{AddressTranslator, PhysicalAddress, VirtualAddress};
pub use address_space::AddressSpace;
pub use block_allocator::{AllocError, BlockAllocator, MemoryRegion};
pub use frame::{Frame, FrameFlag, FrameFlags, ORDER_NOT_BUDDY};
pub use human_address::HumanAddress;
pub use human_size::HumanSize;
pub use memmap::{BootMemoryRegion, FRAMES_PER_SECTION, MemoryMap, SECTION_SIZE};
pub use numbers::{FrameNumber, PageNumber};
pub use page_directory::PageDirectory;
pub use physical_memory_manager::PhysicalMemoryManager;

pub use arch::PAGE_SIZE;
