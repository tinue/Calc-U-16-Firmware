//! LPC111x on-chip peripherals.

pub mod adc;
pub mod gpio;
pub mod i2c;
pub mod iap;
pub mod iocon;
pub mod syscon;
pub mod timer;
pub mod uart;

/// A GPIO pin, identified by port and bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Pin(pub u8, pub u8);

impl std::fmt::Display for Pin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PIO{}_{}", self.0, self.1)
    }
}

/// NVIC interrupt numbers (LPC111x).
pub mod irq {
    pub const I2C: u32 = 15;
    pub const CT16B0: u32 = 16;
    pub const CT16B1: u32 = 17;
    pub const CT32B0: u32 = 18;
    pub const CT32B1: u32 = 19;
    pub const UART: u32 = 21;
    pub const ADC: u32 = 24;
}
