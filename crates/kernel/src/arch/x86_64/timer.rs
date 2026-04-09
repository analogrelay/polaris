// cSpell:ignore HPET hpet lapic femto

use core::hint::spin_loop;

use super::lapic;

// HPET MMIO register offsets (byte offsets; all registers are 64-bit).
const HPET_CAP_REG: usize = 0x00; // General Capabilities: bits [63:32] = counter period (fs)
const HPET_CFG_REG: usize = 0x10; // General Configuration: bit 0 = overall enable
const HPET_COUNTER: usize = 0xF0; // Main Counter Value

// LAPIC timer LVT bit fields.
const LAPIC_TIMER_VECTOR: u32 = 48; // IDT vector assigned to the LAPIC timer
const LVT_MASKED: u32 = 1 << 16; // Mask bit: timer won't fire interrupts when set
const LVT_PERIODIC: u32 = 1 << 17; // Mode bit: periodic (vs. one-shot)

// Timer divide configuration: 0x3 = divide by 16.
const TIMER_DIV_BY_16: u32 = 0x3;

// Maximum number of concurrent timer registrations.
const TIMER_SLOT_COUNT: usize = 8;

/// The calibrated LAPIC tick count for a 1ms interval.
static LAPIC_TICKS_PER_MS: spin::Once<u32> = spin::Once::new();

#[derive(Clone, Copy)]
enum TimerKind {
    Inactive,
    Periodic,
    OneShot,
}

#[derive(Clone, Copy)]
struct TimerSlot {
    kind: TimerKind,
    ticks_remaining: u64,
    period_ticks: u64,
    callback: fn(),
}

impl TimerSlot {
    const fn inactive() -> Self {
        Self {
            kind: TimerKind::Inactive,
            ticks_remaining: 0,
            period_ticks: 0,
            callback: || {},
        }
    }
}

static TIMER_REGISTRY: spin::Mutex<[TimerSlot; TIMER_SLOT_COUNT]> =
    spin::Mutex::new([TimerSlot::inactive(); TIMER_SLOT_COUNT]);

/// Initializes the timer subsystem.
///
/// Calibrates the LAPIC timer against the HPET and starts a 1ms periodic tick.
/// Must be called after `mem::init_allocator()` (address translator required) and after
/// `lapic::init()` (LAPIC MMIO must be accessible).
pub fn init() {
    let hpet_base = super::acpi::hpet_base_address();

    // Read HPET counter period from the capabilities register (bits [63:32], in femtoseconds).
    // SAFETY: hpet_base is the HHDM-mapped HPET MMIO region.
    let hpet_caps = unsafe { core::ptr::read_volatile((hpet_base + HPET_CAP_REG) as *const u64) };
    let hpet_period_fs = (hpet_caps >> 32) as u32;
    assert!(
        hpet_period_fs != 0 && hpet_period_fs < 100_000_000,
        "HPET period {hpet_period_fs} fs out of valid range (must be 0 < period < 100ns)"
    );

    // How many HPET ticks correspond to a 10ms window?
    // period_fs is femtoseconds per tick; 10ms = 10 * 10^12 fs.
    let hpet_ticks_per_10ms = (10_u64 * 1_000_000_000_000_u64) / hpet_period_fs as u64;

    // Enable the HPET main counter.
    // SAFETY: writing to the HPET configuration register.
    unsafe {
        let cfg = core::ptr::read_volatile((hpet_base + HPET_CFG_REG) as *const u64);
        core::ptr::write_volatile((hpet_base + HPET_CFG_REG) as *mut u64, cfg | 1);
    }

    // Set up the LAPIC timer for calibration: one-shot, masked, divide-by-16, max initial count.
    lapic::write_timer_divide(TIMER_DIV_BY_16);
    lapic::write_timer_lvt(LAPIC_TIMER_VECTOR | LVT_MASKED);
    lapic::write_timer_initial_count(0xFFFF_FFFF);

    // Record the HPET start time and wait for 10ms.
    // SAFETY: reading the HPET main counter register.
    let start =
        unsafe { core::ptr::read_volatile((hpet_base + HPET_COUNTER) as *const u64) };

    loop {
        // SAFETY: reading the HPET main counter register.
        let now = unsafe { core::ptr::read_volatile((hpet_base + HPET_COUNTER) as *const u64) };
        if now.wrapping_sub(start) >= hpet_ticks_per_10ms {
            break;
        }
        spin_loop();
    }

    let end_count = lapic::read_timer_current_count();

    // The LAPIC counter counts down from the initial value.
    let elapsed = 0xFFFF_FFFFu32.wrapping_sub(end_count);
    let ticks_per_ms = elapsed / 10;
    assert!(ticks_per_ms > 0, "LAPIC timer calibration produced zero ticks/ms");

    LAPIC_TICKS_PER_MS.call_once(|| ticks_per_ms);
    log::debug!("LAPIC timer: {ticks_per_ms} ticks/ms (divide-by-16)");

    // Switch LAPIC timer to periodic mode at 1ms intervals (unmasked).
    lapic::write_timer_lvt(LAPIC_TIMER_VECTOR | LVT_PERIODIC);
    lapic::write_timer_initial_count(ticks_per_ms);
}

/// Registers a periodic timer that calls `callback` every `period_ms` milliseconds.
///
/// # Panics
/// Panics if no timer slots are available.
pub fn set_periodic(period_ms: u64, callback: fn()) {
    let ticks = period_ms_to_ticks(period_ms);
    let mut registry = TIMER_REGISTRY.lock();
    let slot = registry
        .iter_mut()
        .find(|s| matches!(s.kind, TimerKind::Inactive))
        .expect("no timer slots available");
    *slot = TimerSlot {
        kind: TimerKind::Periodic,
        ticks_remaining: ticks,
        period_ticks: ticks,
        callback,
    };
}

/// Registers a one-shot timer that calls `callback` once after `delay_ms` milliseconds.
///
/// # Panics
/// Panics if no timer slots are available.
pub fn set_oneshot(delay_ms: u64, callback: fn()) {
    let ticks = period_ms_to_ticks(delay_ms);
    let mut registry = TIMER_REGISTRY.lock();
    let slot = registry
        .iter_mut()
        .find(|s| matches!(s.kind, TimerKind::Inactive))
        .expect("no timer slots available");
    *slot = TimerSlot {
        kind: TimerKind::OneShot,
        ticks_remaining: ticks,
        period_ticks: 0,
        callback,
    };
}

/// Called directly from the LAPIC timer interrupt handler (vector 48).
///
/// Advances all active timers by one tick, collects any expired callbacks, then fires them
/// with the registry lock released. Finally sends EOI to the LAPIC.
pub fn handle_tick() {
    let mut fired: [Option<fn()>; TIMER_SLOT_COUNT] = [None; TIMER_SLOT_COUNT];

    if let Some(mut registry) = TIMER_REGISTRY.try_lock() {
        for (slot, fired_slot) in registry.iter_mut().zip(fired.iter_mut()) {
            match slot.kind {
                TimerKind::Inactive => {}
                TimerKind::Periodic => {
                    slot.ticks_remaining -= 1;
                    if slot.ticks_remaining == 0 {
                        *fired_slot = Some(slot.callback);
                        slot.ticks_remaining = slot.period_ticks;
                    }
                }
                TimerKind::OneShot => {
                    slot.ticks_remaining -= 1;
                    if slot.ticks_remaining == 0 {
                        *fired_slot = Some(slot.callback);
                        slot.kind = TimerKind::Inactive;
                    }
                }
            }
        }
        // Lock is released here (registry drops).
    }

    // Fire callbacks with the lock released, so callbacks can call set_periodic/set_oneshot.
    for callback in fired.into_iter().flatten() {
        callback();
    }

    lapic::send_eoi();
}

/// Converts a millisecond delay to a tick count.
///
/// `handle_tick()` is called once per millisecond, so 1 tick == 1ms.
fn period_ms_to_ticks(ms: u64) -> u64 {
    ms
}
