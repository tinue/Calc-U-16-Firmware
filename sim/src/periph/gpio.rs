//! GPIO ports 0..3 register file. Pin levels are resolved by the bus.

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GpioPort {
    pub data: u32,
    pub dir: u32,
    pub is: u32,
    pub ibe: u32,
    pub iev: u32,
    pub ie: u32,
    pub ris: u32,
}

impl GpioPort {
    /// Register access at offset 0x8000.. (not DATA).
    pub fn read_ctrl(&self, off: u32) -> u32 {
        match off {
            0x8000 => self.dir,
            0x8004 => self.is,
            0x8008 => self.ibe,
            0x800C => self.iev,
            0x8010 => self.ie,
            0x8014 => self.ris,
            0x8018 => self.ris & self.ie,
            _ => 0,
        }
    }

    pub fn write_ctrl(&mut self, off: u32, v: u32) {
        let v = v & 0xFFF;
        match off {
            0x8000 => self.dir = v,
            0x8004 => self.is = v,
            0x8008 => self.ibe = v,
            0x800C => self.iev = v,
            0x8010 => self.ie = v,
            0x801C => self.ris &= !v,
            _ => {}
        }
    }
}
