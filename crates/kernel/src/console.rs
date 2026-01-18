//! Defines the early debug console that logs to the VGA framebuffer.

use core::{fmt::Write, sync::atomic::AtomicBool};

#[cfg(debug_assertions)]
use log::LevelFilter;
use spin::{Mutex, Once};

use crate::{framebuffer::FrameBufferWriter, serial::SerialWriter};

pub struct Console {
    has_output: AtomicBool,
    framebuffer: Mutex<Option<FrameBufferWriter>>,
    serial: Mutex<Option<SerialWriter>>,
}

static DEFAULT: Once<Console> = Once::new();

impl Console {
    pub fn init() -> &'static Self {
        let console = Self::default();
        console.install();
        console
    }

    pub fn default() -> &'static Self {
        DEFAULT.call_once(|| Console {
            has_output: AtomicBool::new(false),
            framebuffer: Mutex::new(None),
            serial: Mutex::new(None),
        })
    }

    pub fn install(&'static self) {
        log::set_logger(self).unwrap();

        #[cfg(debug_assertions)]
        log::set_max_level(LevelFilter::Trace);

        #[cfg(not(debug_assertions))]
        log::set_max_level(LevelFilter::Info);
    }

    pub fn has_output(&self) -> bool {
        self.has_output.load(core::sync::atomic::Ordering::SeqCst)
    }

    pub fn attach_serial(&self, serial: SerialWriter) {
        let mut guard = self.serial.lock();
        *guard = Some(serial);
        self.has_output
            .store(true, core::sync::atomic::Ordering::SeqCst);
    }

    pub fn attach_framebuffer(&self, fb: FrameBufferWriter) {
        let mut guard = self.framebuffer.lock();
        *guard = Some(fb);
        self.has_output
            .store(true, core::sync::atomic::Ordering::SeqCst);
    }
}

impl log::Log for Console {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Some(serial) = &mut *self.serial.lock() {
            write_log_entry_to(serial, record).unwrap();
        }
        if let Some(fb) = &mut *self.framebuffer.lock() {
            write_log_entry_to(fb, record).unwrap();
        }
    }

    fn flush(&self) {}
}

fn write_log_entry_to(
    writer: &mut impl core::fmt::Write,
    record: &log::Record,
) -> core::fmt::Result {
    #[cfg(any(debug_assertions, feature = "detailed-logging"))]
    return writeln!(
        writer,
        "[{} {}:{} {}] {}",
        record.level(),
        record.file().unwrap_or("unknown"),
        record.line().unwrap_or(0),
        record.target(),
        record.args()
    );
    #[cfg(not(any(debug_assertions, feature = "detailed-logging")))]
    return writeln!(writer, "[{:5}] {}", record.level(), record.args());
}
