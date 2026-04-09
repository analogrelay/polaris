use pmm::AddressTranslator;
use x86_64::{instructions::port::Port, registers::model_specific::Msr};

// LAPIC register offsets (byte offsets; each register is a u32 at 16-byte alignment).
const SVR: u32 = 0x0F0; // Spurious-Interrupt Vector Register
const EOI: u32 = 0x0B0; // End-of-Interrupt (write 0 to signal EOI)
const LVT_TIMER: u32 = 0x320; // LVT Timer entry
const TIMER_INIT: u32 = 0x380; // Timer Initial Count
const TIMER_CURR: u32 = 0x390; // Timer Current Count (read-only)
const TIMER_DIV: u32 = 0x3E0; // Timer Divide Configuration

const IA32_APIC_BASE_MSR: u32 = 0x1B;

/// Virtual base address of the LAPIC MMIO registers.
static LAPIC_BASE: spin::Once<usize> = spin::Once::new();

/// Initializes the LAPIC for the boot processor.
///
/// This must be called after `mem::init_allocator()` so that the address translator is set up.
///
/// # Panics
/// Panics if x2APIC mode is active (MMIO access is not available in x2APIC mode).
pub fn init() {
    // Read IA32_APIC_BASE MSR to find the LAPIC physical base address.
    // SAFETY: Reading a model-specific register.
    let msr_val = unsafe { Msr::new(IA32_APIC_BASE_MSR).read() };

    assert!(
        (msr_val >> 10) & 1 == 0,
        "x2APIC mode is active; xAPIC MMIO is not available"
    );

    // Bits [51:12] of the MSR hold the LAPIC physical base address.
    let phys_base = (msr_val & 0x000F_FFFF_FFFF_F000) as usize;

    // Map the LAPIC MMIO page into the active page tables.
    // Limine's HHDM does not cover device MMIO regions, so we must add the mapping manually.
    // SAFETY: called after `mem::use_pmm()`; the allocator is available for new page tables.
    unsafe { super::paging::map_mmio(phys_base, 4096) };

    let virt_base = AddressTranslator::current().phys_to_virt(phys_base);
    LAPIC_BASE.call_once(|| virt_base);
    log::debug!("LAPIC Base, {:x} phys, {:x} virt", phys_base, virt_base);

    // Disable legacy 8259 PIC by masking all interrupts on both chips.
    // SAFETY: Port I/O to well-known PIC data ports.
    unsafe {
        Port::<u8>::new(0x21).write(0xFF); // master PIC: mask all IRQs
        Port::<u8>::new(0xA1).write(0xFF); // slave PIC: mask all IRQs
    }

    // Enable the LAPIC via the Spurious-Interrupt Vector Register.
    // Bit 8 = APIC Software Enable; low byte = spurious vector (255, already in IDT).
    let svr = read(SVR);
    write(SVR, (svr & !0xFF) | 0x100 | 0xFF);
}

/// Signals end-of-interrupt to the LAPIC.
pub fn send_eoi() {
    write(EOI, 0);
}

pub(super) fn write_timer_lvt(val: u32) {
    write(LVT_TIMER, val);
}

pub(super) fn write_timer_initial_count(val: u32) {
    write(TIMER_INIT, val);
}

pub(super) fn read_timer_current_count() -> u32 {
    read(TIMER_CURR)
}

pub(super) fn write_timer_divide(val: u32) {
    write(TIMER_DIV, val);
}

/// Reads a LAPIC register at the given byte offset.
fn read(offset: u32) -> u32 {
    let base = *LAPIC_BASE.get().expect("LAPIC not initialized");
    // SAFETY: LAPIC base is valid MMIO mapped by HHDM; offset is a known LAPIC register.
    unsafe { core::ptr::read_volatile((base + offset as usize) as *const u32) }
}

/// Writes a value to a LAPIC register at the given byte offset.
fn write(offset: u32, val: u32) {
    let base = *LAPIC_BASE.get().expect("LAPIC not initialized");
    // SAFETY: LAPIC base is valid MMIO mapped by HHDM; offset is a known LAPIC register.
    unsafe { core::ptr::write_volatile((base + offset as usize) as *mut u32, val) }
}
