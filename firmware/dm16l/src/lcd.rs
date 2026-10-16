//! ST7565-class LCD on a bit-banged SPI link (see docs/hardware.md, "Display").
//!
//! The visible glass is 132×16 pixels on controller pages 0 and 1. Rows are
//! mirrored: physical row y shows controller line 15 - y.

use crate::regs::{Pin, PULL_NONE};
use calc::display::Frame;
use cortex_m::asm::delay;

pub const WIDTH: usize = 132;
pub const HEIGHT: usize = 16;

const SCK: Pin = Pin(1, 8);
const MOSI: Pin = Pin(0, 2);
const A0: Pin = Pin(2, 0);
const RST: Pin = Pin(2, 6);
const CS: Pin = Pin(3, 3);

/// Vendor firmware cold-start values: regulator ratio 5, contrast (EV) 0.
pub const DEFAULT_RR: u8 = 5;
pub const DEFAULT_EV: u8 = 0;

const MS: u32 = 12_000; // core cycles per millisecond at 12 MHz

pub struct Lcd {
    /// fb[page][column], page 0 = controller lines 0..7.
    fb: [[u8; WIDTH]; 2],
    pub contrast: u8,
}

impl Lcd {
    pub const fn new() -> Self {
        Lcd {
            fb: [[0; WIDTH]; 2],
            contrast: DEFAULT_EV,
        }
    }

    fn byte(&self, b: u8) {
        for i in (0..8).rev() {
            SCK.set(false);
            MOSI.set(b & (1 << i) != 0);
            SCK.set(true);
        }
    }

    fn cmd(&self, b: u8) {
        A0.set(false);
        self.byte(b);
    }

    fn data(&self, b: u8) {
        A0.set(true);
        self.byte(b);
    }

    /// Configures the pins, resets the controller and powers it up.
    pub fn init(&mut self) {
        for p in [SCK, MOSI, A0, RST, CS] {
            p.configure(PULL_NONE);
        }
        SCK.output(true);
        MOSI.output(false);
        A0.output(false);
        CS.output(false);
        RST.output(false);
        delay(MS);
        RST.set(true);
        delay(5 * MS);
        for c in [0xA2, 0xA0, 0xC0, 0xF8, 0x00, 0x40] {
            self.cmd(c);
        }
        // Booster, regulator, follower on in steps (vendor delays).
        self.cmd(0x2C);
        delay(50 * MS);
        self.cmd(0x2E);
        delay(50 * MS);
        self.cmd(0x2F);
        delay(10 * MS);
        self.cmd(0x20 | DEFAULT_RR);
        self.set_contrast(self.contrast);
        self.flush();
        self.cmd(0xAF);
        self.cmd(0xA4);
    }

    pub fn set_contrast(&mut self, ev: u8) {
        self.contrast = ev & 0x3F;
        self.cmd(0x81);
        self.cmd(self.contrast);
    }

    /// Power save: static indicator off, display off, all points on.
    pub fn sleep(&self) {
        self.cmd(0xAC);
        self.cmd(0xAE);
        self.cmd(0xA5);
    }

    /// Copies a rendered frame into the framebuffer (send it with `flush`).
    pub fn show(&mut self, frame: &Frame) {
        self.fb = [[0; WIDTH]; 2];
        for (x, &col) in frame.cols.iter().enumerate() {
            for y in 0..HEIGHT {
                if col >> y & 1 != 0 {
                    let line = HEIGHT - 1 - y;
                    self.fb[line / 8][x] |= 1 << (line % 8);
                }
            }
        }
    }

    /// Sends the framebuffer to the controller.
    pub fn flush(&self) {
        for (page, row) in self.fb.iter().enumerate() {
            self.cmd(0xB0 | page as u8);
            self.cmd(0x10);
            self.cmd(0x00);
            for &b in row.iter() {
                self.data(b);
            }
        }
    }
}
