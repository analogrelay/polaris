//! Kernel image representation and linker sections.

use pmm::VirtualAddress;

/// Represents a linker section in the kernel image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkerSection {
    /// The `.text` section containing executable code.
    Text,
    /// The `.rodata` section containing read-only data.
    ReadOnlyData,
    /// The `.data` section containing initialized writable data.
    Data,
    /// The `.bss` section containing uninitialized writable data.
    Bss,
    /// The `.eh_frame` section containing exception handling frames.
    EhFrame,
    /// The `.eh_frame_hdr` section containing exception handling frame headers.
    EhFrameHdr,
    /// The `.interrupt_handlers` section containing interrupt handlers.
    InterruptHandlers,
}

impl LinkerSection {
    /// Returns a byte slice representing the specified linker section.
    ///
    /// # Safety
    ///
    /// The caller must ensure the returned slice is used for the intended purpose
    pub unsafe fn as_bytes(self) -> &'static [u8] {
        unsafe {
            let (start, end) = self.bounds();
            let size = end.as_usize() - start.as_usize();
            core::slice::from_raw_parts(start.as_ptr::<u8>(), size)
        }
    }

    /// Returns a raw pointer to the start of the specified linker section.
    ///
    /// # Safety
    ///
    /// The caller must ensure the pointer is used for the intended purpose
    pub unsafe fn as_ptr<T>(self) -> *const T {
        let (start, _) = self.bounds();
        start.as_ptr::<T>()
    }

    /// Returns the start address of the specified linker section as a `u64`.
    pub fn as_u64(self) -> u64 {
        self.as_usize() as u64
    }

    /// Returns the start address of the specified linker section as a `usize`.
    pub fn as_usize(self) -> usize {
        let (start, _) = self.bounds();
        start.as_usize()
    }

    /// Returns the linker section containing the specified virtual address, if any.
    pub fn containing(addr: VirtualAddress) -> Option<Self> {
        if LinkerSection::Bss.contains(addr) {
            Some(LinkerSection::Bss)
        } else if LinkerSection::Data.contains(addr) {
            Some(LinkerSection::Data)
        } else if LinkerSection::ReadOnlyData.contains(addr) {
            Some(LinkerSection::ReadOnlyData)
        } else if LinkerSection::Text.contains(addr) {
            Some(LinkerSection::Text)
        } else if LinkerSection::EhFrame.contains(addr) {
            Some(LinkerSection::EhFrame)
        } else if LinkerSection::EhFrameHdr.contains(addr) {
            Some(LinkerSection::EhFrameHdr)
        } else if LinkerSection::InterruptHandlers.contains(addr) {
            Some(LinkerSection::InterruptHandlers)
        } else {
            None
        }
    }

    /// Checks if the specified virtual address is within the linker section.
    pub fn contains(self, addr: VirtualAddress) -> bool {
        let (start, end) = self.bounds();
        addr >= start && addr < end
    }

    /// Returns the start and end virtual addresses of the specified linker section.
    pub fn bounds(self) -> (VirtualAddress, VirtualAddress) {
        // SAFETY: Linker symbols are valid throughout the kernel's lifetime
        unsafe {
            match self {
                LinkerSection::Text => {
                    let start = &__kernel_text_start as *const u8 as usize;
                    let end = &__kernel_text_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
                LinkerSection::ReadOnlyData => {
                    let start = &__kernel_rodata_start as *const u8 as usize;
                    let end = &__kernel_rodata_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
                LinkerSection::Data => {
                    let start = &__kernel_data_start as *const u8 as usize;
                    let end = &__kernel_data_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
                LinkerSection::Bss => {
                    let start = &__kernel_bss_start as *const u8 as usize;
                    let end = &__kernel_bss_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
                LinkerSection::EhFrame => {
                    let start = &__kernel_eh_frame_start as *const u8 as usize;
                    let end = &__kernel_eh_frame_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
                LinkerSection::EhFrameHdr => {
                    let start = &__kernel_eh_frame_hdr_start as *const u8 as usize;
                    let end = &__kernel_eh_frame_hdr_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
                LinkerSection::InterruptHandlers => {
                    let start = &__kernel_interrupt_handlers_start as *const u8 as usize;
                    let end = &__kernel_interrupt_handlers_end as *const u8 as usize;
                    (VirtualAddress::new(start), VirtualAddress::new(end))
                }
            }
        }
    }
}

// External symbols from the linker script
unsafe extern "C" {
    static __kernel_text_start: u8;
    static __kernel_text_end: u8;
    static __kernel_rodata_start: u8;
    static __kernel_rodata_end: u8;
    static __kernel_data_start: u8;
    static __kernel_data_end: u8;
    static __kernel_bss_start: u8;
    static __kernel_bss_end: u8;
    static __kernel_eh_frame_start: u8;
    static __kernel_eh_frame_end: u8;
    static __kernel_eh_frame_hdr_start: u8;
    static __kernel_eh_frame_hdr_end: u8;
    static __kernel_interrupt_handlers_start: u8;
    static __kernel_interrupt_handlers_end: u8;
}
