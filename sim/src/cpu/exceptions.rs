//! Exception model: NVIC, SysTick and SCB registers plus exception entry/return.

use super::{Cpu, Halt, LR, PC, SP};
use crate::bus::Bus;
use serde::{Deserialize, Serialize};

pub const EXC_SVC: u32 = 11;
pub const EXC_PENDSV: u32 = 14;
pub const EXC_SYSTICK: u32 = 15;

/// System control space state (0xE000_E000..0xE000_EFFF).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scs {
    pub irq_enabled: u32,
    pub irq_pending: u32,
    /// Current level of each peripheral interrupt line (level-sensitive sources).
    pub irq_level: u32,
    pub irq_prio: [u8; 32],
    pub systick_pending: bool,
    pub pendsv_pending: bool,
    pub shpr2: u32,
    pub shpr3: u32,
    pub scr: u32,
    // SysTick
    pub st_csr: u32,
    pub st_rvr: u32,
    /// Cycle at which the counter was last reloaded (counter value derived from it).
    pub st_base: u64,
    pub st_countflag: bool,
    /// Set by AIRCR.SYSRESETREQ; the machine performs the reset.
    pub reset_requested: bool,
    /// Cycle at which SysTick was frozen (the core clock stops in deep sleep).
    pub st_frozen_at: Option<u64>,
}

impl Scs {
    #[inline]
    pub fn any_pending(&self) -> bool {
        self.irq_pending & self.irq_enabled != 0 || self.systick_pending || self.pendsv_pending
    }

    /// Whether WFI should wake up (ignores PRIMASK).
    #[inline]
    pub fn wakeup_pending(&self) -> bool {
        self.any_pending()
    }

    /// Sets or clears a level-sensitive interrupt line. A high line keeps the IRQ pending.
    pub fn set_line(&mut self, irq: u32, level: bool) {
        let bit = 1u32 << irq;
        if level {
            if self.irq_level & bit == 0 {
                self.irq_pending |= bit;
            }
            self.irq_level |= bit;
        } else {
            self.irq_level &= !bit;
        }
    }

    pub fn priority(&self, exc: u32) -> i32 {
        match exc {
            2 => -2,
            3 => -1,
            EXC_SVC => ((self.shpr2 >> 24) & 0xC0) as i32,
            EXC_PENDSV => ((self.shpr3 >> 16) & 0xC0) as i32,
            EXC_SYSTICK => ((self.shpr3 >> 24) & 0xC0) as i32,
            n if n >= 16 => (self.irq_prio[(n - 16) as usize] & 0xC0) as i32,
            _ => 0,
        }
    }

    pub fn read(&mut self, off: u32, cycles: u64) -> u32 {
        match off {
            0x010 => {
                let v = self.st_csr | (self.st_countflag as u32) << 16;
                self.st_countflag = false;
                v
            }
            0x014 => self.st_rvr,
            0x018 => self.systick_current(cycles),
            0x01C => 0x8000_0000 | 11999,
            0x100 => self.irq_enabled,
            0x180 => self.irq_enabled,
            0x200 | 0x280 => self.irq_pending,
            0x400..=0x41C => {
                let i = (off - 0x400) as usize;
                u32::from_le_bytes([self.irq_prio[i], self.irq_prio[i + 1], self.irq_prio[i + 2], self.irq_prio[i + 3]])
            }
            0xD00 => 0x410C_C200,
            0xD04 => (self.pendsv_pending as u32) << 28 | (self.systick_pending as u32) << 26,
            0xD0C => 0xFA05_0000,
            0xD10 => self.scr,
            0xD14 => 0x208,
            0xD1C => self.shpr2,
            0xD20 => self.shpr3,
            _ => 0,
        }
    }

    pub fn write(&mut self, off: u32, v: u32, cycles: u64) {
        match off {
            0x010 => {
                let was = self.st_csr & 1;
                self.st_csr = v & 7;
                if was == 0 && v & 1 != 0 {
                    self.st_base = cycles;
                }
            }
            0x014 => self.st_rvr = v & 0xFF_FFFF,
            0x018 => {
                self.st_base = cycles;
                self.st_countflag = false;
            }
            0x100 => self.irq_enabled |= v,
            0x180 => self.irq_enabled &= !v,
            0x200 => self.irq_pending |= v,
            0x280 => self.irq_pending = (self.irq_pending & !v) | (self.irq_level & v),
            0x400..=0x41C => {
                let i = (off - 0x400) as usize;
                self.irq_prio[i..i + 4].copy_from_slice(&v.to_le_bytes());
            }
            0xD04 => {
                if v & (1 << 28) != 0 {
                    self.pendsv_pending = true;
                }
                if v & (1 << 27) != 0 {
                    self.pendsv_pending = false;
                }
                if v & (1 << 26) != 0 {
                    self.systick_pending = true;
                }
                if v & (1 << 25) != 0 {
                    self.systick_pending = false;
                }
            }
            0xD0C => {
                if v >> 16 == 0x05FA && v & 4 != 0 {
                    self.reset_requested = true;
                }
            }
            0xD10 => self.scr = v & 0x16,
            0xD1C => self.shpr2 = v,
            0xD20 => self.shpr3 = v,
            _ => {}
        }
    }

    fn systick_period(&self) -> u64 {
        self.st_rvr as u64 + 1
    }

    pub fn systick_current(&self, cycles: u64) -> u32 {
        if self.st_csr & 1 == 0 || self.st_rvr == 0 {
            return 0;
        }
        let cycles = self.st_frozen_at.unwrap_or(cycles);
        let elapsed = cycles.saturating_sub(self.st_base) % self.systick_period();
        (self.st_rvr as u64 - elapsed.min(self.st_rvr as u64)) as u32
    }

    /// Cycle of the next SysTick wrap, if counting.
    pub fn systick_next(&self) -> Option<u64> {
        if self.st_csr & 1 == 0 || self.st_rvr == 0 || self.st_frozen_at.is_some() {
            return None;
        }
        Some(self.st_base + self.systick_period())
    }

    pub fn systick_freeze(&mut self, cycles: u64) {
        if self.st_frozen_at.is_none() {
            self.st_frozen_at = Some(cycles);
        }
    }

    pub fn systick_thaw(&mut self, cycles: u64) {
        if let Some(at) = self.st_frozen_at.take() {
            self.st_base += cycles - at;
        }
    }

    /// Processes SysTick wraps up to `cycles`.
    pub fn systick_service(&mut self, cycles: u64) {
        if let Some(mut at) = self.systick_next() {
            let period = self.systick_period();
            while at <= cycles {
                self.st_base = at;
                self.st_countflag = true;
                if self.st_csr & 2 != 0 {
                    self.systick_pending = true;
                }
                at += period;
            }
        }
    }
}

impl Cpu {
    fn execution_priority(&self, bus: &Bus) -> i32 {
        let mut prio = 256;
        for &e in &self.active {
            prio = prio.min(bus.scs.priority(e));
        }
        if self.primask {
            prio = prio.min(0);
        }
        prio
    }

    /// Highest-priority pending exception that may preempt the current execution.
    pub(super) fn preempting_exception(&self, bus: &Bus) -> Option<u32> {
        let scs = &bus.scs;
        let mut best: Option<(i32, u32)> = None;
        let mut consider = |exc: u32| {
            let p = scs.priority(exc);
            if best.is_none_or(|(bp, _)| p < bp) {
                best = Some((p, exc));
            }
        };
        let mut pend = scs.irq_pending & scs.irq_enabled;
        while pend != 0 {
            let i = pend.trailing_zeros();
            consider(16 + i);
            pend &= pend - 1;
        }
        if scs.systick_pending {
            consider(EXC_SYSTICK);
        }
        if scs.pendsv_pending {
            consider(EXC_PENDSV);
        }
        let (p, exc) = best?;
        (p < self.execution_priority(bus)).then_some(exc)
    }

    pub(super) fn exception_entry(&mut self, bus: &mut Bus, exc: u32, return_addr: u32) {
        if bus.trace.irq && (16..16 + 13).contains(&exc) {
            bus.trace_line(format!("exception {exc} (start logic {}) from {return_addr:#06x}", exc - 16));
        }
        match exc {
            EXC_SYSTICK => bus.scs.systick_pending = false,
            EXC_PENDSV => bus.scs.pendsv_pending = false,
            n if n >= 16 => bus.scs.irq_pending &= !(1 << (n - 16)),
            _ => {}
        }
        let mut sp = self.r[SP];
        let realign = sp & 4 != 0;
        sp = sp.wrapping_sub(0x20) & !7;
        let xpsr = self.xpsr() | (realign as u32) << 9;
        let frame = [self.r[0], self.r[1], self.r[2], self.r[3], self.r[12], self.r[LR], return_addr, xpsr];
        for (i, v) in frame.iter().enumerate() {
            bus.write32(sp + 4 * i as u32, *v);
        }
        self.r[SP] = sp;
        let exc_return = if self.ipsr != 0 {
            0xFFFF_FFF1
        } else if self.spsel {
            // Thread mode on PSP: switch to MSP for the handler.
            std::mem::swap(&mut self.r[SP], &mut self.other_sp);
            0xFFFF_FFFD
        } else {
            0xFFFF_FFF9
        };
        self.r[LR] = exc_return;
        self.ipsr = exc;
        self.active.push(exc);
        self.r[PC] = bus.read32(exc * 4) & !1;
    }

    pub(super) fn exception_return(&mut self, bus: &mut Bus, exc_return: u32) {
        let Some(done) = self.active.pop() else {
            self.halted = Some(Halt::Fault("exception return without active exception".into()));
            return;
        };
        // A level-sensitive line that is still asserted pends the IRQ again.
        if done >= 16 {
            let bit = 1 << (done - 16);
            bus.scs.irq_pending |= bus.scs.irq_level & bit;
        }
        let to_thread_psp = exc_return & 0xF == 0xD;
        if to_thread_psp {
            std::mem::swap(&mut self.r[SP], &mut self.other_sp);
        }
        let sp = self.r[SP];
        let mut frame = [0u32; 8];
        for (i, v) in frame.iter_mut().enumerate() {
            *v = bus.read32(sp + 4 * i as u32);
        }
        let xpsr = frame[7];
        self.r[SP] = sp.wrapping_add(0x20) | if xpsr & (1 << 9) != 0 { 4 } else { 0 };
        self.r[0] = frame[0];
        self.r[1] = frame[1];
        self.r[2] = frame[2];
        self.r[3] = frame[3];
        self.r[12] = frame[4];
        self.r[LR] = frame[5];
        self.r[PC] = frame[6] & !1;
        self.n = xpsr & 0x8000_0000 != 0;
        self.z = xpsr & 0x4000_0000 != 0;
        self.c = xpsr & 0x2000_0000 != 0;
        self.v = xpsr & 0x1000_0000 != 0;
        self.ipsr = if exc_return & 0xF == 0x1 { *self.active.last().unwrap_or(&0) } else { 0 };
    }
}
