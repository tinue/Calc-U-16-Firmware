//! Bit-banged SPI receiver and ST7565R/ST7567-class LCD controller model.

use serde::{Deserialize, Serialize};

pub const WIDTH: usize = 132;
pub const HEIGHT: usize = 16;
const PAGES: usize = 9;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lcd {
    pub ram: Vec<Vec<u8>>,
    pub page: usize,
    pub column: usize,
    pub start_line: u8,
    pub display_on: bool,
    pub all_on: bool,
    pub reverse: bool,
    pub seg_reverse: bool,
    pub com_reverse: bool,
    pub contrast: u8,
    pub regulator: u8,
    pub power: u8,
    pub bias: u8,
    /// Second byte expected for a two-byte command (0x81 contrast, 0xF8 booster, 0xAD indicator).
    pending: Option<u8>,
    // SPI shift register
    shift: u8,
    bits: u8,
    /// Number of bytes received (statistics).
    pub bytes: u64,
    /// Incremented whenever display content or state changes.
    pub generation: u64,
    #[serde(skip)]
    pub log: Vec<String>,
    pub log_enabled: bool,
}

impl Default for Lcd {
    fn default() -> Self {
        Lcd {
            ram: vec![vec![0; WIDTH]; PAGES],
            page: 0,
            column: 0,
            start_line: 0,
            display_on: false,
            all_on: false,
            reverse: false,
            seg_reverse: false,
            com_reverse: false,
            contrast: 0x20,
            regulator: 0,
            power: 0,
            bias: 0,
            pending: None,
            shift: 0,
            bits: 0,
            bytes: 0,
            generation: 0,
            log: Vec::new(),
            log_enabled: false,
        }
    }
}

impl Lcd {
    /// Controller reset (RST pin low or command 0xE2).
    pub fn reset(&mut self) {
        let ram = std::mem::take(&mut self.ram);
        let (log, log_enabled, bytes, generation) = (std::mem::take(&mut self.log), self.log_enabled, self.bytes, self.generation);
        *self = Lcd::default();
        self.ram = ram;
        self.log = log;
        self.log_enabled = log_enabled;
        self.bytes = bytes;
        self.generation = generation + 1;
    }

    /// Chip select went inactive or idle: drop partial bytes.
    pub fn deselect(&mut self) {
        self.bits = 0;
    }

    /// Rising edge of SCK with chip select active.
    pub fn clock(&mut self, mosi: bool, a0: bool) {
        self.shift = (self.shift << 1) | mosi as u8;
        self.bits += 1;
        if self.bits == 8 {
            self.bits = 0;
            self.bytes += 1;
            let b = self.shift;
            if a0 {
                self.data(b);
            } else {
                self.command(b);
            }
        }
    }

    fn data(&mut self, b: u8) {
        if self.page < PAGES && self.column < WIDTH {
            self.ram[self.page][self.column] = b;
        }
        if self.column < WIDTH {
            self.column += 1;
        }
        self.generation += 1;
    }

    fn command(&mut self, b: u8) {
        if self.log_enabled {
            self.log.push(format!("cmd {b:02x}"));
        }
        if let Some(first) = self.pending.take() {
            if first == 0x81 { self.contrast = b & 0x3F }
            self.generation += 1;
            return;
        }
        match b {
            0x00..=0x0F => self.column = (self.column & 0xF0) | b as usize,
            0x10..=0x1F => self.column = (self.column & 0x0F) | ((b as usize & 0x0F) << 4),
            0x20..=0x27 => self.regulator = b & 7,
            0x28..=0x2F => self.power = b & 7,
            0x40..=0x7F => self.start_line = b & 0x3F,
            0x81 | 0xF8 | 0xAD => self.pending = Some(b),
            0xA0 | 0xA1 => self.seg_reverse = b & 1 != 0,
            0xA2 | 0xA3 => self.bias = b & 1,
            0xA4 | 0xA5 => self.all_on = b & 1 != 0,
            0xA6 | 0xA7 => self.reverse = b & 1 != 0,
            0xAE | 0xAF => self.display_on = b & 1 != 0,
            0xB0..=0xBF => self.page = (b & 0x0F) as usize,
            0xC0..=0xCF => self.com_reverse = b & 0x08 != 0,
            0xE2 => self.reset(),
            _ => {}
        }
        self.generation += 1;
    }

    /// Visible pixels of the 132×16 glass, row 0 at the top.
    ///
    /// The glass shows controller lines 0..15 bottom-up: physical row y shows
    /// display line 15 - y (see docs/hardware.md, "rows are mirrored").
    pub fn pixels(&self) -> [[bool; WIDTH]; HEIGHT] {
        let mut out = [[false; WIDTH]; HEIGHT];
        if !self.display_on {
            return out;
        }
        for (y, row) in out.iter_mut().enumerate() {
            let com = (15 - y) as u8;
            let com = if self.com_reverse { 63 - com } else { com };
            let line = ((com + self.start_line) % 64) as usize;
            let (page, bit) = (line / 8, line % 8);
            for (x, px) in row.iter_mut().enumerate() {
                let col = if self.seg_reverse { WIDTH - 1 - x } else { x };
                let on = page < PAGES && self.ram[page][col] & (1 << bit) != 0;
                *px = self.all_on || (on != self.reverse);
            }
        }
        out
    }

    /// Text-art rendering ('#' = pixel on, '.' = off), one line per pixel row.
    pub fn text_art(&self) -> String {
        let mut s = String::new();
        for row in self.pixels() {
            s.extend(row.iter().map(|&p| if p { '#' } else { '.' }));
            s.push('\n');
        }
        s
    }
}
