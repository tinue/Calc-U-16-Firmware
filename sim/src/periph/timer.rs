//! CT16B0/1 and CT32B0/1 counter/timers, evaluated lazily from the cycle counter.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    pub bits32: bool,
    pub ir: u32,
    pub tcr: u32,
    pub pr: u32,
    pub mcr: u32,
    pub mr: [u32; 4],
    pub emr: u32,
    pub ctcr: u32,
    pub pwmc: u32,
    pub ccr: u32,
    /// TC value at `cyc_base`.
    tc_base: u32,
    cyc_base: u64,
    /// Cycle of the next match event worth processing.
    next: Option<u64>,
}

impl Timer {
    pub fn new(bits32: bool) -> Self {
        Timer {
            bits32,
            ir: 0,
            tcr: 0,
            pr: 0,
            mcr: 0,
            mr: [0; 4],
            emr: 0,
            ctcr: 0,
            pwmc: 0,
            ccr: 0,
            tc_base: 0,
            cyc_base: 0,
            next: None,
        }
    }

    fn modulus(&self) -> u64 {
        if self.bits32 { 1 << 32 } else { 1 << 16 }
    }

    fn running(&self) -> bool {
        self.tcr & 3 == 1
    }

    fn ticks(&self, cycles: u64) -> u64 {
        cycles.saturating_sub(self.cyc_base) / (self.pr as u64 + 1)
    }

    pub fn tc(&self, cycles: u64) -> u32 {
        if !self.running() {
            return self.tc_base;
        }
        ((self.tc_base as u64 + self.ticks(cycles)) % self.modulus()) as u32
    }

    /// Re-anchors the counter at `cycles` (before a configuration change).
    fn rebase(&mut self, cycles: u64) {
        self.tc_base = self.tc(cycles);
        self.cyc_base = cycles;
    }

    fn channel_active(&self, ch: usize) -> bool {
        (self.mcr >> (3 * ch)) & 7 != 0 || (self.emr >> (4 + 2 * ch)) & 3 != 0
    }

    /// Cycle of the next match on channel `ch` strictly after the anchor.
    fn match_cycle(&self, ch: usize) -> u64 {
        let m = self.modulus();
        let mr = self.mr[ch] as u64 % m;
        let mut d = (mr + m - self.tc_base as u64 % m) % m;
        if d == 0 {
            d = m;
        }
        self.cyc_base + d * (self.pr as u64 + 1)
    }

    fn recompute(&mut self) {
        self.next = if self.running() {
            (0..4).filter(|&ch| self.channel_active(ch)).map(|ch| self.match_cycle(ch)).min()
        } else {
            None
        };
    }

    pub fn next_event(&self) -> Option<u64> {
        self.next
    }

    pub fn em(&self, ch: usize) -> bool {
        self.emr & (1 << ch) != 0
    }

    /// Processes all matches up to `cycles`. Returns true if the EM outputs changed.
    pub fn service(&mut self, cycles: u64) -> bool {
        let em_before = self.emr & 0xF;
        while let Some(at) = self.next {
            if at > cycles {
                break;
            }
            let matched: Vec<usize> = (0..4).filter(|&ch| self.channel_active(ch) && self.match_cycle(ch) == at).collect();
            // Advance the anchor to the match point.
            self.tc_base = self.tc(at);
            self.cyc_base = at;
            let mut reset = false;
            for ch in matched {
                let mcr = (self.mcr >> (3 * ch)) & 7;
                if mcr & 1 != 0 {
                    self.ir |= 1 << ch;
                }
                match (self.emr >> (4 + 2 * ch)) & 3 {
                    1 => self.emr &= !(1 << ch),
                    2 => self.emr |= 1 << ch,
                    3 => self.emr ^= 1 << ch,
                    _ => {}
                }
                if mcr & 2 != 0 {
                    reset = true;
                }
                if mcr & 4 != 0 {
                    self.tcr &= !1;
                }
            }
            if reset {
                self.tc_base = 0;
                // TC passes through 0: channels matching 0 fire now.
                for ch in 0..4 {
                    if self.mr[ch] == 0 && self.channel_active(ch) {
                        if (self.mcr >> (3 * ch)) & 1 != 0 {
                            self.ir |= 1 << ch;
                        }
                        match (self.emr >> (4 + 2 * ch)) & 3 {
                            1 => self.emr &= !(1 << ch),
                            2 => self.emr |= 1 << ch,
                            3 => self.emr ^= 1 << ch,
                            _ => {}
                        }
                    }
                }
            }
            self.recompute();
        }
        self.emr & 0xF != em_before
    }

    pub fn irq_line(&self) -> bool {
        self.ir & 0x1F != 0
    }

    pub fn read(&self, off: u32, cycles: u64) -> u32 {
        match off {
            0x00 => self.ir,
            0x04 => self.tcr,
            0x08 => self.tc(cycles),
            0x0C => self.pr,
            0x10 => {
                if self.running() {
                    (cycles.saturating_sub(self.cyc_base) % (self.pr as u64 + 1)) as u32
                } else {
                    0
                }
            }
            0x14 => self.mcr,
            0x18..=0x24 => self.mr[((off - 0x18) / 4) as usize],
            0x28 => self.ccr,
            0x3C => self.emr,
            0x70 => self.ctcr,
            0x74 => self.pwmc,
            _ => 0,
        }
    }

    pub fn write(&mut self, off: u32, v: u32, cycles: u64) {
        let mask = if self.bits32 { u32::MAX } else { 0xFFFF };
        self.rebase(cycles);
        match off {
            0x00 => self.ir &= !v,
            0x04 => {
                self.tcr = v & 3;
                if v & 2 != 0 {
                    self.tc_base = 0;
                }
            }
            0x08 => self.tc_base = v & mask,
            0x0C => self.pr = v & mask,
            0x14 => self.mcr = v & 0xFFF,
            0x18..=0x24 => self.mr[((off - 0x18) / 4) as usize] = v & mask,
            0x28 => self.ccr = v,
            0x3C => self.emr = v & 0xFFF,
            0x70 => self.ctcr = v,
            0x74 => self.pwmc = v,
            _ => {}
        }
        self.recompute();
    }
}
