//! PCF8563 RTC on I2C (PIO0_4 SCL, PIO0_5 SDA, address 0x51).
//!
//! Only used to silence the RTC's interrupt output: its open-drain /INT shares
//! the ON line (PIO0_11) and stays low while an alarm/timer flag is set, which
//! the vendor firmware may have left behind.

use crate::regs::{self, syscon, Pin};

const BASE: u32 = 0x4000_0000;
const CONSET: u32 = BASE;
const STAT: u32 = BASE + 0x04;
const DAT: u32 = BASE + 0x08;
const SCLH: u32 = BASE + 0x10;
const SCLL: u32 = BASE + 0x14;
const CONCLR: u32 = BASE + 0x18;

const STA: u32 = 0x20;
const SI: u32 = 0x08;
const STO: u32 = 0x10;
const I2EN: u32 = 0x40;

const ADDR: u32 = 0x51;

fn init() {
    regs::modify(syscon::PRESETCTRL, |v| v | 2); // release I2C reset
    regs::modify(syscon::SYSAHBCLKCTRL, |v| v | syscon::CLK_I2C);
    regs::write(Pin(0, 4).iocon_addr(), 0x01); // SCL, standard I2C mode
    regs::write(Pin(0, 5).iocon_addr(), 0x01); // SDA
    regs::write(SCLH, 60); // 12 MHz / (60 + 60) = 100 kHz
    regs::write(SCLL, 60);
    regs::write(CONCLR, 0x6C);
    regs::write(CONSET, I2EN);
}

/// Waits for SI and returns the status code (0 on timeout).
fn wait() -> u32 {
    for _ in 0..50_000 {
        if regs::read(CONSET) & SI != 0 {
            return regs::read(STAT);
        }
    }
    0
}

/// Writes `data` starting at register `reg`. Returns the failing status code, if any.
fn write(reg: u8, data: &[u8]) -> Result<(), u32> {
    regs::write(CONSET, STA);
    let mut result = match wait() {
        0x08 => Ok(()),
        s => Err(s),
    };
    if result.is_ok() {
        regs::write(DAT, ADDR << 1);
        regs::write(CONCLR, STA | SI);
        for (i, &b) in [reg].iter().chain(data.iter()).enumerate() {
            let expect = if i == 0 { 0x18 } else { 0x28 };
            let s = wait();
            if s != expect {
                result = Err(s);
                break;
            }
            regs::write(DAT, b as u32);
            regs::write(CONCLR, SI);
        }
        if result.is_ok() {
            let s = wait();
            if s != 0x28 {
                result = Err(s);
            }
        }
    }
    regs::write(CONSET, STO);
    regs::write(CONCLR, SI | STA);
    result
}

/// Disables and clears all RTC interrupts (alarm, timer) and stops the timer.
pub fn silence_interrupts() -> Result<(), u32> {
    init();
    write(0x01, &[0x00])?; // control 2: TIE/AIE off, clear TF/AF
    write(0x09, &[0x80, 0x80, 0x80, 0x80])?; // alarms disabled
    write(0x0E, &[0x03])?; // timer off
    Ok(())
}
