//! 10-bit ADC (software-started conversions only).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Adc {
    pub cr: u32,
    pub inten: u32,
    pub dr: [u32; 8],
    pub gdr: u32,
}

impl Adc {
    pub fn read(&mut self, off: u32) -> u32 {
        match off {
            0x00 => self.cr,
            0x04 => {
                let v = self.gdr;
                self.gdr &= !0x8000_0000;
                v
            }
            0x0C => self.inten,
            0x10..=0x2C => {
                let i = ((off - 0x10) / 4) as usize;
                let v = self.dr[i];
                self.dr[i] &= !0x8000_0000;
                v
            }
            0x30 => self.dr.iter().enumerate().fold(0, |a, (i, d)| a | ((d >> 31) << i)),
            _ => 0,
        }
    }

    /// Writes a register. Returns the channel to convert if a conversion was started.
    pub fn write(&mut self, off: u32, v: u32) -> Option<usize> {
        match off {
            0x00 => {
                self.cr = v & !(7 << 24);
                if (v >> 24) & 7 == 1 && v & 0xFF != 0 {
                    return Some((v & 0xFF).trailing_zeros() as usize);
                }
                None
            }
            0x0C => {
                self.inten = v;
                None
            }
            _ => None,
        }
    }

    pub fn complete(&mut self, ch: usize, value: u16) {
        let v = 0x8000_0000 | ((value as u32 & 0x3FF) << 6);
        self.dr[ch] = v;
        self.gdr = v | (ch as u32) << 24;
    }
}
