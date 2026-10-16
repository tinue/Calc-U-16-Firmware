//! Keyboard bring-up diagnostics for the serial monitor.

use crate::keys;
use crate::regs::{self, Pin};
use crate::uart;
use cortex_m::asm::delay;

/// Pins `hunt` must not touch: LCD, UART, I2C, reset.
const RESERVED: [Pin; 10] = [
    Pin(0, 0),
    Pin(0, 2),
    Pin(0, 4),
    Pin(0, 5),
    Pin(1, 6),
    Pin(1, 7),
    Pin(1, 8),
    Pin(2, 0),
    Pin(2, 6),
    Pin(3, 3),
];

fn ports() -> [u32; 4] {
    core::array::from_fn(|p| regs::read(0x5000_3FFC + ((p as u32) << 16)))
}

fn write_pin(p: Pin) {
    uart::write(&[b'P', b'0' + p.0, b'_']);
    uart::write_dec(p.1 as u32);
}

fn write_ports(v: &[u32; 4]) {
    for (i, x) in v.iter().enumerate() {
        uart::write(&[b' ', b'P', b'0' + i as u8, b'=']);
        uart::write_hex(*x);
    }
}

/// Drives `p` low for ~50 µs and returns all port levels read meanwhile.
fn pulse_low(p: Pin) -> [u32; 4] {
    let iocon = regs::read(p.iocon_addr());
    let dir_addr = 0x5000_8000 + ((p.0 as u32) << 16);
    let dir = regs::read(dir_addr);
    p.configure(iocon & 0x18);
    p.output(false);
    delay(600);
    let v = ports();
    regs::write(dir_addr, dir);
    regs::write(p.iocon_addr(), iocon);
    v
}

/// Drives each documented column low and prints the readback of all ports.
pub fn probe() {
    let idle = ports();
    uart::write(b"idle ");
    write_ports(&idle);
    uart::write(b"\r\n");
    for (i, &c) in keys::COLUMNS.iter().enumerate() {
        let v = pulse_low(c);
        uart::write(b"C");
        uart::write_dec(i as u32);
        uart::write(b" ");
        write_pin(c);
        uart::write(if v[c.0 as usize] & (1 << c.1) == 0 {
            b" low "
        } else {
            b" HIGH"
        });
        write_ports(&v);
        uart::write(b"\r\n");
    }
}

/// Drives every unreserved pin low in turn and reports other pins that follow it.
pub fn hunt() {
    let mut hits = 0;
    for port in 0..4u8 {
        let n = if port == 3 { 6 } else { 12 };
        for pin in 0..n {
            let p = Pin(port, pin);
            if RESERVED.contains(&p) {
                continue;
            }
            let before = ports();
            let during = pulse_low(p);
            let after = ports();
            let mut dropped = [0u32; 4];
            for i in 0..4 {
                // Fell while driven and recovered afterwards; ignore the driven pin itself.
                dropped[i] = before[i] & !during[i] & after[i];
            }
            dropped[port as usize] &= !(1 << pin);
            if dropped.iter().any(|&d| d != 0) {
                hits += 1;
                write_pin(p);
                uart::write(b" pulls");
                write_ports(&dropped);
                uart::write(b"\r\n");
            }
        }
    }
    uart::write(b"hunt done, hits ");
    uart::write_dec(hits);
    uart::write(b"\r\n");
}
