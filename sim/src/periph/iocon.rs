//! IOCON pin configuration (LPC111x, 48-pin packages).

use super::Pin;

/// IOCON register offset for each pin (UM10398 table "I/O configuration registers").
pub const IOCON_MAP: &[(u32, Pin)] = &[
    (0x000, Pin(2, 6)),
    (0x008, Pin(2, 0)),
    (0x00C, Pin(0, 0)),
    (0x010, Pin(0, 1)),
    (0x014, Pin(1, 8)),
    (0x01C, Pin(0, 2)),
    (0x020, Pin(2, 7)),
    (0x024, Pin(2, 8)),
    (0x028, Pin(2, 1)),
    (0x02C, Pin(0, 3)),
    (0x030, Pin(0, 4)),
    (0x034, Pin(0, 5)),
    (0x038, Pin(1, 9)),
    (0x03C, Pin(3, 4)),
    (0x040, Pin(2, 4)),
    (0x044, Pin(2, 5)),
    (0x048, Pin(3, 5)),
    (0x04C, Pin(0, 6)),
    (0x050, Pin(0, 7)),
    (0x054, Pin(2, 9)),
    (0x058, Pin(2, 10)),
    (0x05C, Pin(2, 2)),
    (0x060, Pin(0, 8)),
    (0x064, Pin(0, 9)),
    (0x068, Pin(0, 10)),
    (0x06C, Pin(1, 10)),
    (0x070, Pin(2, 11)),
    (0x074, Pin(0, 11)),
    (0x078, Pin(1, 0)),
    (0x07C, Pin(1, 1)),
    (0x080, Pin(1, 2)),
    (0x084, Pin(3, 0)),
    (0x088, Pin(3, 1)),
    (0x08C, Pin(2, 3)),
    (0x090, Pin(1, 3)),
    (0x094, Pin(1, 4)),
    (0x098, Pin(1, 11)),
    (0x09C, Pin(3, 2)),
    (0x0A0, Pin(1, 5)),
    (0x0A4, Pin(1, 6)),
    (0x0A8, Pin(1, 7)),
    (0x0AC, Pin(3, 3)),
];

/// Timer match outputs selectable on pins: (pin, IOCON FUNC, timer index, match channel).
/// Timer index: 0 = CT16B0, 1 = CT16B1, 2 = CT32B0, 3 = CT32B1.
pub const MATCH_PINS: &[(Pin, u32, usize, usize)] = &[
    (Pin(0, 8), 2, 0, 0),
    (Pin(0, 9), 2, 0, 1),
    (Pin(0, 10), 3, 0, 2),
    (Pin(1, 9), 1, 1, 0),
    (Pin(1, 10), 2, 1, 1),
    (Pin(1, 6), 2, 2, 0),
    (Pin(1, 7), 2, 2, 1),
    (Pin(0, 1), 2, 2, 2),
    (Pin(0, 11), 3, 2, 3),
    (Pin(1, 1), 3, 3, 0),
    (Pin(1, 2), 3, 3, 1),
    (Pin(1, 3), 3, 3, 2),
    (Pin(1, 4), 2, 3, 3),
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Iocon {
    /// Register file indexed by offset / 4.
    pub regs: Vec<u32>,
}

impl Default for Iocon {
    fn default() -> Self {
        // Reset value: FUNC 0, pull-up enabled, hysteresis off.
        Iocon { regs: vec![0xD0; 0x100 / 4] }
    }
}

impl Iocon {
    pub fn offset_of(pin: Pin) -> Option<u32> {
        IOCON_MAP.iter().find(|(_, p)| *p == pin).map(|(o, _)| *o)
    }

    pub fn reg(&self, pin: Pin) -> u32 {
        Self::offset_of(pin).map_or(0xD0, |o| self.regs[(o / 4) as usize])
    }

    pub fn func(&self, pin: Pin) -> u32 {
        self.reg(pin) & 7
    }

    /// FUNC value that selects GPIO for this pin.
    pub fn gpio_func(pin: Pin) -> u32 {
        match pin {
            Pin(0, 0) | Pin(0, 10) | Pin(0, 11) | Pin(1, 0) | Pin(1, 1) | Pin(1, 2) | Pin(1, 3) => 1,
            _ => 0,
        }
    }

    pub fn is_gpio(&self, pin: Pin) -> bool {
        self.func(pin) == Self::gpio_func(pin)
    }

    /// Level the pin floats to when nothing drives it, from the MODE bits.
    pub fn pull_level(&self, pin: Pin) -> bool {
        (self.reg(pin) >> 3) & 3 != 1
    }

    pub fn describe(&self) -> String {
        IOCON_MAP
            .iter()
            .map(|(o, p)| format!("{p:<8} IOCON+{o:#05x} = {:#06x}", self.regs[(o / 4) as usize]))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
