//! UART on PIO1_6 (RXD) / PIO1_7 (TXD), 38400 8N1, wired to the CP2102 USB bridge.

use crate::regs::{self, syscon, uart, Pin};

pub fn init() {
    regs::modify(syscon::SYSAHBCLKCTRL, |v| v | syscon::CLK_UART);
    // RXD/TXD with pull-ups (as the vendor firmware): keeps RXD defined while the
    // USB bridge is unpowered.
    regs::write(Pin(1, 6).iocon_addr(), 0xD1);
    regs::write(Pin(1, 7).iocon_addr(), 0xD1);
    regs::write(syscon::UARTCLKDIV, 1);
    // 12 MHz / (16 × 13 × (1 + 1/2)) = 38462 baud (+0.16 %).
    regs::write(uart::LCR, 0x83);
    regs::write(uart::DATA, 13);
    regs::write(uart::DLM, 0);
    regs::write(uart::FDR, (2 << 4) | 1);
    regs::write(uart::LCR, 0x03);
    regs::write(uart::FCR, 0x07);
}

/// Sends bytes. Gives up on a byte if the transmitter never becomes ready, so a
/// misbehaving UART cannot stall the firmware.
pub fn write(s: &[u8]) {
    for &b in s {
        let mut spins = 0u32;
        while regs::read(uart::LSR) & 0x20 == 0 {
            spins += 1;
            if spins > 20_000 {
                return;
            }
        }
        regs::write(uart::DATA, b as u32);
    }
}

/// Writes `v` in decimal.
pub fn write_dec(mut v: u32) {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    write(&buf[i..]);
}

/// Writes `v` as 8 hex digits.
pub fn write_hex(v: u32) {
    let mut buf = [0u8; 8];
    for (i, b) in buf.iter_mut().enumerate() {
        let d = (v >> (28 - 4 * i)) as u8 & 15;
        *b = if d < 10 { b'0' + d } else { b'A' + d - 10 };
    }
    write(&buf);
}

pub fn read() -> Option<u8> {
    (regs::read(uart::LSR) & 1 != 0).then(|| regs::read(uart::DATA) as u8)
}

/// Collects a command line; returns it when CR or LF arrives.
pub struct LineReader {
    buf: [u8; 32],
    len: usize,
}

impl LineReader {
    pub const fn new() -> Self {
        LineReader {
            buf: [0; 32],
            len: 0,
        }
    }

    pub fn poll(&mut self, mut on_line: impl FnMut(&[u8])) {
        while let Some(b) = read() {
            match b {
                b'\r' | b'\n' => {
                    if self.len > 0 {
                        on_line(&self.buf[..self.len]);
                        self.len = 0;
                    }
                }
                _ if self.len < self.buf.len() => {
                    self.buf[self.len] = b;
                    self.len += 1;
                }
                _ => {}
            }
        }
    }
}
