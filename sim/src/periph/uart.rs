//! 16550-style UART with host-side byte pacing at the configured baud rate.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Uart {
    pub dll: u32,
    pub dlm: u32,
    pub ier: u32,
    pub lcr: u32,
    pub mcr: u32,
    pub scr: u32,
    pub fdr: u32,
    pub fcr: u32,
    /// Receive FIFO (as seen by the CPU).
    pub fifo: VecDeque<u8>,
    /// Bytes from the host not yet "on the wire".
    pub host_rx: VecDeque<u8>,
    /// Bytes transmitted by the CPU, collected for the host.
    pub tx: Vec<u8>,
    pub thre_irq: bool,
    pub overrun: bool,
    /// Cycle at which the next host byte arrives in the FIFO.
    pub next_rx: Option<u64>,
    /// Bytes dropped because the UART was not clocked (deep sleep).
    pub dropped: u64,
}

impl Uart {
    fn dlab(&self) -> bool {
        self.lcr & 0x80 != 0
    }

    pub fn irq_line(&self) -> bool {
        self.iir() & 1 == 0
    }

    fn iir(&self) -> u32 {
        let id = if self.ier & 4 != 0 && self.overrun {
            0x6
        } else if self.ier & 1 != 0 && !self.fifo.is_empty() {
            0x4
        } else if self.ier & 2 != 0 && self.thre_irq {
            0x2
        } else {
            return 0xC1;
        };
        0xC0 | id
    }

    /// Baud rate derived from the divisor latch and fractional divider (PCLK = `hz`).
    pub fn baud(&self, hz: f64) -> f64 {
        let div = ((self.dlm << 8) | self.dll).max(1) as f64;
        let (divadd, mulval) = (self.fdr & 15, (self.fdr >> 4) & 15);
        let frac = if mulval == 0 { 1.0 } else { 1.0 + divadd as f64 / mulval as f64 };
        hz / (16.0 * div * frac)
    }

    pub fn read(&mut self, off: u32) -> u32 {
        match off {
            0x00 if self.dlab() => self.dll,
            0x00 => self.fifo.pop_front().unwrap_or(0) as u32,
            0x04 if self.dlab() => self.dlm,
            0x04 => self.ier,
            0x08 => {
                let v = self.iir();
                if v & 0xF == 0x2 {
                    self.thre_irq = false;
                }
                v
            }
            0x0C => self.lcr,
            0x10 => self.mcr,
            0x14 => {
                let v = (!self.fifo.is_empty()) as u32 | (self.overrun as u32) << 1 | 0x60;
                self.overrun = false;
                v
            }
            0x1C => self.scr,
            0x28 => self.fdr,
            _ => 0,
        }
    }

    pub fn write(&mut self, off: u32, v: u32) {
        match off {
            0x00 if self.dlab() => self.dll = v & 0xFF,
            0x00 => {
                self.tx.push(v as u8);
                self.thre_irq = true;
            }
            0x04 if self.dlab() => self.dlm = v & 0xFF,
            0x04 => {
                self.ier = v & 0x307;
                if v & 2 != 0 {
                    self.thre_irq = true;
                }
            }
            0x08 => {
                self.fcr = v;
                if v & 2 != 0 {
                    self.fifo.clear();
                }
            }
            0x0C => self.lcr = v & 0xFF,
            0x10 => self.mcr = v,
            0x1C => self.scr = v & 0xFF,
            0x28 => self.fdr = v & 0xFF,
            _ => {}
        }
    }

    /// Moves one host byte into the receive FIFO.
    pub fn deliver(&mut self) {
        if let Some(b) = self.host_rx.pop_front() {
            if self.fifo.len() >= 16 {
                self.overrun = true;
            } else {
                self.fifo.push_back(b);
            }
        }
    }
}
