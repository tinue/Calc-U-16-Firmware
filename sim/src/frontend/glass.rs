//! Photo-like SVG rendering of the 132×16 glass, in physical millimetres.
//!
//! Geometry is measured on a real DM16L: the pixel field is about 69 × 9 mm,
//! seen through a black-masked window of about 73 × 13 mm in the aluminium
//! bezel. Pixels have a thin gap between them and cast a faint shadow onto
//! the reflector behind the liquid crystal.

use crate::board::lcd::{HEIGHT, WIDTH};
use std::fmt::Write;

pub type Pixels = [[bool; WIDTH]; HEIGHT];

/// Horizontal / vertical pixel pitch in mm.
const PITCH_X: f64 = 69.0 / WIDTH as f64;
const PITCH_Y: f64 = 9.0 / HEIGHT as f64;
/// Fraction of the pitch covered by a lit pixel (the rest is the gap).
const FILL: f64 = 0.90;
/// Visible window in the mask, and the pixel field's offset inside it.
const WIN_W: f64 = 73.0;
const WIN_H: f64 = 13.0;
const FIELD_X: f64 = (WIN_W - 69.0) / 2.0;
const FIELD_Y: f64 = 1.7;
/// Bezel around the window.
const BEZEL: f64 = 3.0;
/// Shadow offset of a pixel on the reflector.
const SHADOW_DX: f64 = 0.18;
const SHADOW_DY: f64 = 0.22;

/// Parse text art ('#' = on, anything else = off) into 16-row frames.
/// Blank lines are ignored, so frames may be separated by empty lines.
pub fn parse_text_art(text: &str) -> Vec<Pixels> {
    let rows: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    rows.chunks(HEIGHT)
        .map(|chunk| {
            let mut px = [[false; WIDTH]; HEIGHT];
            for (y, line) in chunk.iter().enumerate() {
                for (x, c) in line.chars().take(WIDTH).enumerate() {
                    px[y][x] = c == '#';
                }
            }
            px
        })
        .collect()
}

/// Render one frame as a standalone SVG. `scale` is screen pixels per mm.
pub fn svg(px: &Pixels, scale: f64) -> String {
    let (w, h) = (WIN_W + 2.0 * BEZEL, WIN_H + 2.0 * BEZEL);
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="0 0 {w} {h}">
<defs>
  <linearGradient id="alu" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#d9dadc"/><stop offset="0.45" stop-color="#b9bbbe"/>
    <stop offset="0.55" stop-color="#c8cacc"/><stop offset="1" stop-color="#a4a6a9"/>
  </linearGradient>
  <linearGradient id="glass" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#b3b597"/><stop offset="0.6" stop-color="#aeb092"/>
    <stop offset="1" stop-color="#a5a789"/>
  </linearGradient>
  <linearGradient id="mask" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#000" stop-opacity="0.55"/><stop offset="0.12" stop-color="#000" stop-opacity="0"/>
  </linearGradient>
  <linearGradient id="maskl" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#000" stop-opacity="0.45"/><stop offset="0.025" stop-color="#000" stop-opacity="0"/>
  </linearGradient>
  <filter id="blur" x="-5%" y="-20%" width="110%" height="140%"><feGaussianBlur stdDeviation="0.09"/></filter>
</defs>
<rect width="{w}" height="{h}" rx="1.2" fill="url(#alu)"/>
<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" rx="0.9" fill="#8e9093"/>
<rect x="{BEZEL}" y="{BEZEL}" width="{WIN_W}" height="{WIN_H}" rx="0.6" fill="url(#glass)"/>
"##,
        w * scale,
        h * scale,
        BEZEL - 0.35,
        BEZEL - 0.35,
        WIN_W + 0.7,
        WIN_H + 0.7,
    );

    let (ox, oy) = (BEZEL + FIELD_X, BEZEL + FIELD_Y);
    let (pw, ph) = (PITCH_X * FILL, PITCH_Y * FILL);
    let mut path = String::new();
    for (y, row) in px.iter().enumerate() {
        let py = oy + y as f64 * PITCH_Y;
        for (x, _) in row.iter().enumerate().filter(|(_, &on)| on) {
            let _ = write!(path, "M{:.3} {:.3}h{pw:.3}v{ph:.3}h-{pw:.3}z", ox + x as f64 * PITCH_X, py);
        }
    }
    let _ = write!(
        s,
        r##"<path d="{path}" transform="translate({SHADOW_DX} {SHADOW_DY})" fill="#4a4c3a" opacity="0.35" filter="url(#blur)"/>
<path d="{path}" fill="#23241c" opacity="0.92"/>
<rect x="{BEZEL}" y="{BEZEL}" width="{WIN_W}" height="{WIN_H}" rx="0.6" fill="url(#mask)"/>
<rect x="{BEZEL}" y="{BEZEL}" width="{WIN_W}" height="{WIN_H}" rx="0.6" fill="url(#maskl)"/>
<rect x="{BEZEL}" y="{BEZEL}" width="{WIN_W}" height="{WIN_H}" rx="0.6" fill="none" stroke="#141414" stroke-width="0.5"/>
</svg>
"##
    );
    s
}
