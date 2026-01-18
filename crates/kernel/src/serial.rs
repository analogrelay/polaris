#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::console::Console;

pub struct SerialWriter {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    port: uart_16550::SerialPort,
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    port: uart_16550::MmioSerialPort,
}

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.port.write_str(s)
    }
}

// cSpell:ignore uart
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn init(console: &Console) {
    let mut port = unsafe { uart_16550::SerialPort::new(0x3F8) };
    port.init();
    console.attach_serial(SerialWriter { port });
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn init(console: &Console, address: usize) {
    let mut port = unsafe { uart_16550::MmioSerialPort::new(address) };
    port.init();
    console.attach_serial(SerialWriter { port });
}
