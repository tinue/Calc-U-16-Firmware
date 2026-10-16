//! SYSCON (clocks, power, start logic) and PMU.

use serde::{Deserialize, Serialize};

pub const IRC_HZ: f64 = 12_000_000.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Syscon {
    /// Register file indexed by offset / 4 (0x000..0x3FC).
    pub regs: Vec<u32>,
    /// STARTSRP0 latched start-logic status.
    pub start_status: u32,
    pub pmu: [u32; 5],
}

impl Default for Syscon {
    fn default() -> Self {
        let mut s = Syscon { regs: vec![0; 0x100], start_status: 0, pmu: [0; 5] };
        s.set(0x000, 2); // SYSMEMREMAP: user flash
        s.set(0x008, 0); // SYSPLLCTRL
        s.set(0x024, 0xA0); // WDTOSCCTRL reset value
        s.set(0x030, 1); // SYSRSTSTAT: power-on reset
        s.set(0x044, 1);
        s.set(0x074, 1);
        s.set(0x078, 1); // SYSAHBCLKDIV
        s.set(0x080, 0x85F); // SYSAHBCLKCTRL reset value
        s.set(0x098, 0);
        s.set(0x230, 0xFFFF);
        s.set(0x234, 0xEDF0);
        s.set(0x238, 0xEDF0);
        s
    }
}

impl Syscon {
    pub fn get(&self, off: u32) -> u32 {
        self.regs[(off / 4) as usize]
    }

    pub fn set(&mut self, off: u32, v: u32) {
        self.regs[(off / 4) as usize] = v;
    }

    pub fn read(&self, off: u32) -> u32 {
        match off {
            0x00C => 1, // SYSPLLSTAT: always locked
            0x20C => self.start_status,
            0x3F4 => 0x1A40_902B, // DEVICE_ID: LPC1114FBD48/302
            _ => self.get(off),
        }
    }

    /// Returns true when the write changes the core clock.
    pub fn write(&mut self, off: u32, v: u32) -> bool {
        match off {
            0x00C | 0x3F4 => false,
            0x030 => {
                // SYSRSTSTAT: write 1 to clear
                self.set(0x030, self.get(0x030) & !v);
                false
            }
            0x208 => {
                // STARTRSRP0CLR
                self.start_status &= !v;
                false
            }
            _ => {
                self.set(off, v);
                matches!(off, 0x008 | 0x024 | 0x040 | 0x070 | 0x074 | 0x078)
            }
        }
    }

    fn wdt_osc_hz(&self) -> f64 {
        let v = self.get(0x024);
        let freqsel = (v >> 5) & 15;
        let div = 2.0 * (1 + (v & 31)) as f64;
        const ANA: [f64; 16] = [
            0.0, 0.6e6, 1.05e6, 1.4e6, 1.75e6, 2.1e6, 2.4e6, 2.7e6, 3.0e6, 3.25e6, 3.5e6, 3.75e6, 4.0e6, 4.2e6, 4.4e6, 4.6e6,
        ];
        (ANA[freqsel as usize] / div).max(1.0)
    }

    /// Core (system AHB) clock in Hz.
    pub fn core_hz(&self) -> f64 {
        let pll_in = IRC_HZ; // SYSPLLCLKSEL: IRC or 12 MHz system oscillator
        let main = match self.get(0x070) & 3 {
            0 => IRC_HZ,
            1 => pll_in,
            2 => self.wdt_osc_hz(),
            _ => pll_in * ((self.get(0x008) & 31) + 1) as f64,
        };
        let div = self.get(0x078) & 0xFF;
        main / div.max(1) as f64
    }

    /// Records an edge on start-logic input `bit` (0..11 = PIO0_n, 12 = PIO1_0).
    /// Returns true if it is enabled and latched.
    pub fn start_edge(&mut self, bit: u32, rising: bool) -> bool {
        let polarity_rising = self.get(0x200) & (1 << bit) != 0;
        let enabled = self.get(0x204) & (1 << bit) != 0;
        if polarity_rising != rising || !enabled {
            return false;
        }
        // STARTSRP0 only reflects enabled start signals.
        self.start_status |= 1 << bit;
        true
    }

    /// Start-logic interrupt lines currently asserted (status & enable).
    pub fn start_lines(&self) -> u32 {
        self.start_status & self.get(0x204) & 0x1FFF
    }
}
