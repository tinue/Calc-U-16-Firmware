//! 10 ms pacing from CT32B0, polled (no interrupts, no sleep).
//!
//! The first hardware test hung with a SysTick-interrupt + WFI loop; the vendor
//! firmware paces its main loop with CT32B0 as well.

use crate::regs::{self, syscon};

const BASE: u32 = 0x4001_4000;
const IR: u32 = BASE;
const TCR: u32 = BASE + 0x04;
const PR: u32 = BASE + 0x0C;
const MCR: u32 = BASE + 0x14;
const MR0: u32 = BASE + 0x18;

pub fn init() {
    regs::modify(syscon::SYSAHBCLKCTRL, |v| v | syscon::CLK_CT32B0);
    regs::write(TCR, 2); // hold in reset
    regs::write(PR, 0);
    regs::write(MR0, 120_000 - 1); // 10 ms at 12 MHz
    regs::write(MCR, 3); // flag + reset on MR0
    regs::write(IR, 0x1F);
    regs::write(TCR, 1);
}

/// Waits for the next 10 ms tick.
pub fn wait_tick() {
    while regs::read(IR) & 1 == 0 {}
    regs::write(IR, 1);
}
