//! The Voyager (DM-L) board: how the LPC111x pins connect to keyboard, LCD, RTC, beeper.

pub mod keyboard;
pub mod lcd;
pub mod rtc;

use crate::periph::Pin;
use serde::{Deserialize, Serialize};

/// Keyboard matrix columns C0..C9 (driven low one at a time), left to right.
pub const COLUMNS: [Pin; 10] =
    [Pin(2, 7), Pin(2, 8), Pin(2, 1), Pin(0, 3), Pin(1, 9), Pin(3, 4), Pin(2, 4), Pin(2, 5), Pin(3, 5), Pin(0, 6)];
/// Keyboard matrix rows R1..R4 (read, pulled up), top to bottom.
pub const ROWS: [Pin; 4] = [Pin(0, 7), Pin(0, 8), Pin(0, 10), Pin(1, 0)];
/// ON key / RTC interrupt line.
pub const ON_LINE: Pin = Pin(0, 11);
/// ON key's other contact: column C0.
pub const ON_DRIVE: Pin = Pin(2, 7);

pub const LCD_SCK: Pin = Pin(1, 8);
pub const LCD_MOSI: Pin = Pin(0, 2);
pub const LCD_A0: Pin = Pin(2, 0);
pub const LCD_RST: Pin = Pin(2, 6);
pub const LCD_CS: Pin = Pin(3, 3);

pub const BEEPER_A: Pin = Pin(1, 1);
pub const BEEPER_B: Pin = Pin(1, 2);

pub const BAT_REF_EN: Pin = Pin(3, 2);
/// Battery measurement: ADC channel 7 (PIO1_11).
pub const BAT_ADC_CH: usize = 7;
/// Vbat[mV] = BAT_K / ADC (from the firmware, 0x294C).
pub const BAT_K: f64 = 1_263_405.0;

pub const RTC_ADDR: u8 = 0x51;

/// Beeper activity derived from toggles of PIO1_1.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Beeper {
    /// Time of the first and last toggle of the current burst (seconds).
    pub burst: Option<(f64, f64, u32)>,
    /// Completed beeps: (start time s, frequency Hz, duration s).
    pub beeps: Vec<(f64, f64, f64)>,
}

impl Beeper {
    pub fn toggle(&mut self, now: f64) {
        match &mut self.burst {
            Some((_, last, n)) if now - *last < 0.02 => {
                *last = now;
                *n += 1;
            }
            _ => {
                self.flush(now + 1.0);
                self.burst = Some((now, now, 1));
            }
        }
    }

    /// Closes a burst once no toggle happened for 20 ms.
    pub fn flush(&mut self, now: f64) {
        if let Some((start, last, n)) = self.burst {
            if now - last >= 0.02 {
                let dur = last - start;
                if n > 2 && dur > 0.0 {
                    self.beeps.push((start, (n - 1) as f64 / dur / 2.0, dur));
                }
                self.burst = None;
            }
        }
    }
}
