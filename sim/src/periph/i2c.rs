//! LPC I2C controller (master mode) with a pluggable slave device.

use serde::{Deserialize, Serialize};

const AA: u32 = 0x04;
const SI: u32 = 0x08;
const STO: u32 = 0x10;
const STA: u32 = 0x20;
const I2EN: u32 = 0x40;

/// An I2C slave on the bus.
pub trait I2cDevice {
    fn address(&self) -> u8;
    /// START + address phase. Returns ACK.
    fn start(&mut self, read: bool) -> bool;
    fn write(&mut self, byte: u8) -> bool;
    fn read(&mut self) -> u8;
    fn stop(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    Idle,
    /// START sent, waiting for the address byte.
    Started,
    Tx,
    Rx,
    /// Address not acknowledged; waiting for STOP or repeated START.
    Nack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I2c {
    pub conset: u32,
    pub stat: u32,
    pub dat: u32,
    pub adr0: u32,
    pub sclh: u32,
    pub scll: u32,
    phase: Phase,
    /// Cycle at which the bus operation in progress completes.
    pub pending_at: Option<u64>,
    /// Transfer log for tracing: (read?, address, bytes).
    #[serde(skip)]
    pub log: Vec<String>,
}

impl Default for I2c {
    fn default() -> Self {
        I2c { conset: 0, stat: 0xF8, dat: 0, adr0: 0, sclh: 4, scll: 4, phase: Phase::Idle, pending_at: None, log: Vec::new() }
    }
}

impl I2c {
    pub fn irq_line(&self) -> bool {
        self.conset & SI != 0
    }

    fn byte_cycles(&self) -> u64 {
        9 * (self.sclh + self.scll).max(8) as u64
    }

    pub fn read(&self, off: u32) -> u32 {
        match off {
            0x00 => self.conset,
            0x04 => self.stat,
            0x08 => self.dat,
            0x0C => self.adr0,
            0x10 => self.sclh,
            0x14 => self.scll,
            _ => 0,
        }
    }

    pub fn write(&mut self, off: u32, v: u32, cycles: u64) {
        match off {
            0x00 => self.conset |= v & 0x7C,
            0x08 => self.dat = v & 0xFF,
            0x0C => self.adr0 = v,
            0x10 => self.sclh = v & 0xFFFF,
            0x14 => self.scll = v & 0xFFFF,
            0x18 => {
                // CONCLR: AAC, SIC, STAC, I2ENC
                self.conset &= !(v & 0x6C);
            }
            _ => return,
        }
        if self.conset & I2EN == 0 {
            self.phase = Phase::Idle;
            self.stat = 0xF8;
            self.pending_at = None;
            return;
        }
        if self.conset & SI == 0 && self.pending_at.is_none() && (self.conset & (STA | STO) != 0 || self.phase != Phase::Idle) {
            self.pending_at = Some(cycles + self.byte_cycles());
        }
    }

    /// Performs the bus operation requested by the CPU. `dev` is the only slave on the bus.
    pub fn service(&mut self, cycles: u64, dev: &mut dyn I2cDevice) {
        match self.pending_at {
            Some(t) if t <= cycles => self.pending_at = None,
            _ => return,
        }
        if self.conset & SI != 0 || self.conset & I2EN == 0 {
            return;
        }
        if self.conset & STO != 0 {
            if self.phase != Phase::Idle {
                dev.stop();
                self.log.push("P".into());
            }
            self.phase = Phase::Idle;
            self.conset &= !STO;
            self.stat = 0xF8;
            if self.conset & STA == 0 {
                return;
            }
        }
        if self.conset & STA != 0 && matches!(self.phase, Phase::Idle | Phase::Tx | Phase::Rx | Phase::Nack) {
            self.stat = if self.phase == Phase::Idle { 0x08 } else { 0x10 };
            self.phase = Phase::Started;
            self.log.push("S".into());
            self.conset |= SI;
            return;
        }
        match self.phase {
            Phase::Idle => return,
            Phase::Started => {
                let addr = (self.dat >> 1) as u8;
                let read = self.dat & 1 != 0;
                let ack = addr == dev.address() && dev.start(read);
                self.log.push(format!("{:02x}{}{}", addr, if read { "R" } else { "W" }, if ack { "" } else { "-nack" }));
                self.phase = match (ack, read) {
                    (false, _) => Phase::Nack,
                    (true, false) => Phase::Tx,
                    (true, true) => Phase::Rx,
                };
                self.stat = match (ack, read) {
                    (true, false) => 0x18,
                    (false, false) => 0x20,
                    (true, true) => 0x40,
                    (false, true) => 0x48,
                };
            }
            Phase::Tx => {
                let ack = dev.write(self.dat as u8);
                self.log.push(format!("<{:02x}", self.dat));
                self.stat = if ack { 0x28 } else { 0x30 };
            }
            Phase::Rx => {
                self.dat = dev.read() as u32;
                self.log.push(format!(">{:02x}", self.dat));
                self.stat = if self.conset & AA != 0 { 0x50 } else { 0x58 };
            }
            Phase::Nack => return,
        }
        self.conset |= SI;
    }

    pub fn reset(&mut self) {
        let log = std::mem::take(&mut self.log);
        *self = I2c::default();
        self.log = log;
    }
}
