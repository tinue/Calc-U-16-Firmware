//! Display pipeline: calculator state → display model → pixels.
//!
//! The model says what to show (screen kind, values with their base, indicators)
//! without fonts or positions. `render` decides the layout. The rules are in
//! docs/requirements.md (R-3..R-13); the prototype is tools/display_lab/lab.py.

use crate::fonts::{F3, F4, F5};
use crate::{Base, Calc};

pub const W: usize = 132;
pub const H: usize = 16;

// --- fonts and frame -------------------------------------------------------------

pub struct Font {
    pub height: u8,
    /// (character, columns with bit 0 = top row), sorted by character.
    pub glyphs: &'static [(u8, &'static [u8])],
}

impl Font {
    fn glyph(&self, c: u8) -> &'static [u8] {
        match self.glyphs.binary_search_by_key(&c, |g| g.0) {
            Ok(i) => self.glyphs[i].1,
            Err(_) => &[0, 0, 0],
        }
    }
}

/// Digit grouping: `gap` extra pixels between groups of `size` characters, counted from the right.
#[derive(Clone, Copy)]
struct Group {
    size: usize,
    gap: i32,
}

const UNGROUPED: Group = Group { size: 0, gap: 0 };

fn text_width(s: &[u8], f: &Font, g: Group) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let w: i32 = s.iter().map(|&c| f.glyph(c).len() as i32 + 1).sum::<i32>() - 1;
    w + (s.len() - 1).checked_div(g.size).unwrap_or(0) as i32 * g.gap
}

/// 132×16 pixels: bit y of `cols[x]` is row y, row 0 at the top.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    pub cols: [u16; W],
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub const fn new() -> Self {
        Frame { cols: [0; W] }
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        self.cols[x] >> y & 1 != 0
    }

    fn set(&mut self, x: i32, y: i32) {
        if (0..W as i32).contains(&x) && (0..H as i32).contains(&y) {
            self.cols[x as usize] |= 1 << y;
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.set(x, y);
            }
        }
    }

    fn invert(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for x in x0.max(0)..=x1.min(W as i32 - 1) {
            for y in y0.max(0)..=y1.min(H as i32 - 1) {
                self.cols[x as usize] ^= 1 << y;
            }
        }
    }

    /// Draws `s` from `x`; returns the x after it (including one column of spacing).
    fn text(&mut self, mut x: i32, y: i32, s: &[u8], f: &Font, g: Group) -> i32 {
        let n = s.len();
        for (i, &c) in s.iter().enumerate() {
            let cols = f.glyph(c);
            for (k, &col) in cols.iter().enumerate() {
                for r in 0..f.height as i32 {
                    if col >> r & 1 != 0 {
                        self.set(x + k as i32, y + r);
                    }
                }
            }
            x += cols.len() as i32 + 1;
            if g.size > 0 && i + 1 < n && (n - 1 - i).is_multiple_of(g.size) {
                x += g.gap;
            }
        }
        x
    }

    /// Draws `s` with its last pixel column at `right`.
    fn text_right(&mut self, right: i32, y: i32, s: &[u8], f: &Font, g: Group) {
        self.text(right - text_width(s, f, g) + 1, y, s, f, g);
    }

    /// `s` inverse in a box one pixel larger; returns the x after the box.
    fn chip(&mut self, x: i32, y: i32, s: &[u8], f: &Font) -> i32 {
        let (w, h) = (text_width(s, f, UNGROUPED), f.height as i32);
        self.rect(x - 1, y - 1, x + w, y + h);
        self.invert(x, y, x + w - 1, y + h - 1);
        self.text(x, y, s, f, UNGROUPED);
        self.invert(x, y, x + w - 1, y + h - 1);
        x + w + 1
    }
}

// --- display model -----------------------------------------------------------------

/// Up to 24 ASCII characters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Text {
    buf: [u8; 24],
    len: u8,
}

impl Text {
    pub const fn new() -> Self {
        Text {
            buf: [0; 24],
            len: 0,
        }
    }

    pub fn from(s: &[u8]) -> Self {
        let mut t = Text::new();
        for &c in s {
            t.push(c);
        }
        t
    }

    pub fn push(&mut self, c: u8) {
        if (self.len as usize) < self.buf.len() {
            self.buf[self.len as usize] = c;
            self.len += 1;
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

/// Set of indicators that are on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Indicators(pub u16);

impl Indicators {
    pub const BATT: u16 = 1 << 0;
    pub const F: u16 = 1 << 1;
    pub const G: u16 = 1 << 2;
    pub const C: u16 = 1 << 3;
    pub const V: u16 = 1 << 4;
    pub const LZ: u16 = 1 << 5;
    pub const F0: u16 = 1 << 6;
    pub const F1: u16 = 1 << 7;
    pub const F2: u16 = 1 << 8;
    pub const M1: u16 = 1 << 9;
    pub const M2: u16 = 1 << 10;
    pub const M3: u16 = 1 << 11;
    pub const M4: u16 = 1 << 12;

    pub fn has(self, i: u16) -> bool {
        self.0 & i != 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    Twos,
    Ones,
    Unsigned,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Model {
    /// Main screen: the entry line (lower) and the same value in the other base (upper).
    Numbers {
        entry: Text,
        entry_base: Base,
        other: Text,
        other_base: Base,
        ind: Indicators,
    },
    /// BIN screen: page `page` (0 = lowest 64 bits) of a `word`-bit value.
    Bits {
        value: u128,
        word: u8,
        page: u8,
        ind: Indicators,
    },
    /// Status display, while the status key is held.
    Status {
        word: u8,
        sign: Sign,
        ind: Indicators,
        vbat_mv: Option<u16>,
    },
    /// Plain text: errors, settings feedback, start-up banner.
    Message { top: Text, bottom: Text },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shift {
    F,
    G,
}

/// Display-related state outside the calculator core.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Ui {
    pub shift: Option<Shift>,
    /// The status key is held.
    pub status: bool,
}

/// `v` in `base`. With `signed_word` = Some(word size), DEC shows it as a signed
/// (2's complement) value.
fn number(v: u64, base: Base, signed_word: Option<u8>) -> Text {
    let neg = match signed_word {
        Some(w) => base == Base::Dec && v >> (w - 1) & 1 != 0,
        None => false,
    };
    let mut mag = if neg {
        v.wrapping_neg() & (u64::MAX >> (64 - signed_word.unwrap_or(64) as u32))
    } else {
        v
    };
    let mut tmp = [0u8; 64];
    let mut n = 0;
    loop {
        let d = (mag % base.radix()) as u8;
        tmp[n] = if d < 10 { b'0' + d } else { b'A' + d - 10 };
        n += 1;
        mag /= base.radix();
        if mag == 0 {
            break;
        }
    }
    let mut t = Text::new();
    if neg {
        t.push(b'-');
    }
    for i in (0..n).rev() {
        t.push(tmp[i]);
    }
    t
}

/// The display model for the current core (2's complement, no carry/overflow yet).
pub fn model(c: &Calc, ui: &Ui) -> Model {
    if let Some(e) = c.error {
        let mut bottom = Text::from(b"ERROR ");
        bottom.push(b'0' + e as u8);
        return Model::Message {
            top: Text::new(),
            bottom,
        };
    }
    if ui.status {
        return Model::Status {
            word: c.word,
            sign: Sign::Twos,
            ind: Indicators::default(),
            vbat_mv: None,
        };
    }
    let ind = Indicators(match ui.shift {
        Some(Shift::F) => Indicators::F,
        Some(Shift::G) => Indicators::G,
        None => 0,
    });
    if c.base == Base::Bin {
        return Model::Bits {
            value: c.x() as u128,
            word: c.word,
            page: 0,
            ind,
        };
    }
    let other_base = if c.base == Base::Dec {
        Base::Hex
    } else {
        Base::Dec
    };
    Model::Numbers {
        // While typing, the digits are shown as entered (no sign).
        entry: number(c.x(), c.base, if c.entering { None } else { Some(c.word) }),
        entry_base: c.base,
        other: number(c.x(), other_base, Some(c.word)),
        other_base,
        ind,
    }
}

// --- renderer -------------------------------------------------------------------------

fn grouping(base: Base, f: &Font) -> Group {
    match base {
        Base::Hex => Group {
            size: 4,
            gap: if core::ptr::eq(f, &F5) { 3 } else { 2 },
        },
        _ => Group { size: 3, gap: 1 },
    }
}

fn value_width(s: &[u8], base: Base, f: &Font, grouped: bool) -> i32 {
    text_width(
        s,
        f,
        if grouped {
            grouping(base, f)
        } else {
            UNGROUPED
        },
    ) + 5
}

/// First font that fits in `room`; grouping is dropped before the font shrinks.
fn fit(s: &[u8], base: Base, room: i32, fonts: &[&'static Font]) -> (&'static Font, bool) {
    for &f in fonts {
        for grouped in [true, false] {
            if value_width(s, base, f, grouped) <= room {
                return (f, grouped);
            }
        }
    }
    (fonts[fonts.len() - 1], false)
}

/// A value right-aligned on the 7-row line at `y`, its suffix bottom-aligned after it.
fn put_value(fr: &mut Frame, y: i32, s: &[u8], base: Base, f: &Font, grouped: bool) {
    fr.text_right(W as i32 - 1, y + 2, &[base.suffix()], &F3, UNGROUPED);
    let g = if grouped {
        grouping(base, f)
    } else {
        UNGROUPED
    };
    fr.text_right(W as i32 - 6, y + 7 - f.height as i32, s, f, g);
}

/// Indicators at the left of the upper line; returns the x after them (0 if none).
fn line_indicators(fr: &mut Frame, ind: Indicators) -> i32 {
    let mut x = 0;
    if ind.has(Indicators::BATT) {
        fr.rect(0, 0, 1, 1);
        x = 4;
    }
    for (i, label) in [
        (Indicators::F, b"f"),
        (Indicators::G, b"g"),
        (Indicators::C, b"C"),
        (Indicators::V, b"V"),
    ] {
        if ind.has(i) {
            x = fr.text(x, 2, label, &F3, UNGROUPED) + 2;
        }
    }
    x
}

fn render_numbers(
    fr: &mut Frame,
    entry: &Text,
    entry_base: Base,
    other: &Text,
    other_base: Base,
    ind: Indicators,
) {
    // The entry line shrinks only for its own length, never for indicators.
    let (f, grouped) = fit(entry.as_bytes(), entry_base, W as i32, &[&F5, &F4]);
    put_value(fr, 9, entry.as_bytes(), entry_base, f, grouped);
    // The upper line gives way to the indicators.
    let x = line_indicators(fr, ind);
    let room = W as i32 - if x > 0 { x + 2 } else { 0 };
    let (f, grouped) = fit(other.as_bytes(), other_base, room, &[&F5, &F4, &F3]);
    put_value(fr, 0, other.as_bytes(), other_base, f, grouped);
}

/// `n` bits of `value` from bit `low` upwards, most significant first, as '0'/'1'.
fn bit_chars(value: u128, low: u32, n: usize, out: &mut [u8; 64]) -> &[u8] {
    for (i, c) in out[..n].iter_mut().enumerate() {
        *c = b'0' + (value >> (low as usize + n - 1 - i) & 1) as u8;
    }
    &out[..n]
}

fn render_bits(fr: &mut Frame, value: u128, word: u8, page: u8, ind: Indicators) {
    let word = word as usize;
    let nibbles = Group { size: 4, gap: 4 };
    let mut buf = [0u8; 64];
    if word <= 16 {
        // One line in 5x7, nibble positions numbered above.
        let b = bit_chars(value, 0, word, &mut buf);
        fr.text_right(W as i32 - 1, 9, b, &F5, nibbles);
        let mut x = W as i32 - text_width(b, &F5, nibbles);
        for i in 0..word {
            let pos = word - 1 - i;
            if pos % 4 == 3 || i == 0 {
                let label = number(pos as u64, Base::Dec, None);
                fr.text(x, 2, label.as_bytes(), &F3, UNGROUPED);
            }
            x += 6 + if pos.is_multiple_of(4) { 4 } else { 0 };
        }
        line_indicators(fr, ind);
    } else if word <= 32 {
        // Two lines of 16 in 5x7.
        fr.text_right(
            W as i32 - 1,
            0,
            bit_chars(value, 16, word - 16, &mut buf),
            &F5,
            nibbles,
        );
        fr.text_right(
            W as i32 - 1,
            9,
            bit_chars(value, 0, 16, &mut buf),
            &F5,
            nibbles,
        );
        line_indicators(fr, ind);
    } else {
        // Pages of 64: two lines of 32 in 3x5, every other nibble overlined.
        let low = 64 * page as usize;
        let count = (word - low).min(64);
        for (y, from, n) in [
            (0, low + 32, count.saturating_sub(32)),
            (9, low, count.min(32)),
        ] {
            let b = bit_chars(value, from as u32, n, &mut buf);
            let x = W as i32 - 4 * n as i32;
            fr.text(x, y + 2, b, &F3, UNGROUPED);
            for k in (1..n.div_ceil(4)).step_by(2) {
                let x1 = 130 - 16 * k as i32;
                fr.rect((x1 - 14).max(x), y, x1, y);
            }
        }
        let pages = word.div_ceil(64);
        if pages > 1 {
            // Position in the left margin: the visible page solid, the rest dotted
            // and kept one row away from it.
            let h = (H / pages) as i32;
            let top = (pages - 1 - page as usize) as i32 * h;
            let bot = top + h - 1;
            fr.rect(0, top, 1, bot);
            let mut y = top - 2;
            while y >= 0 {
                fr.set(0, y);
                y -= 2;
            }
            let mut y = bot + 2;
            while y < H as i32 {
                fr.set(0, y);
                y += 2;
            }
        }
    }
}

/// Status indicators in their fixed places (R-9): each keeps its slot.
const STATUS_SLOTS: [(u16, &[u8]); 10] = [
    (Indicators::C, b"C"),
    (Indicators::V, b"V"),
    (Indicators::LZ, b"LZ"),
    (Indicators::F0, b"F0"),
    (Indicators::F1, b"F1"),
    (Indicators::F2, b"F2"),
    (Indicators::M1, b"M1"),
    (Indicators::M2, b"M2"),
    (Indicators::M3, b"M3"),
    (Indicators::M4, b"M4"),
];

fn render_status(fr: &mut Frame, word: u8, sign: Sign, ind: Indicators, vbat_mv: Option<u16>) {
    // Line 1: values.
    let ws = number(word as u64, Base::Dec, None);
    let x = fr.text(1, 1, ws.as_bytes(), &F3, UNGROUPED) + 3;
    let sign: &[u8] = match sign {
        Sign::Twos => b"2C",
        Sign::Ones => b"1C",
        Sign::Unsigned => b"UN",
    };
    fr.text(x, 1, sign, &F3, UNGROUPED);
    if let Some(mv) = vbat_mv {
        let v = mv / 10;
        let volts = [
            b'0' + (v / 100 % 10) as u8,
            b'.',
            b'0' + (v / 10 % 10) as u8,
            b'0' + (v % 10) as u8,
            b'V',
        ];
        if ind.has(Indicators::BATT) {
            fr.chip(
                W as i32 - 1 - text_width(&volts, &F3, UNGROUPED),
                1,
                &volts,
                &F3,
            );
        } else {
            fr.text_right(W as i32 - 1, 1, &volts, &F3, UNGROUPED);
        }
    }
    // Line 2: indicators that are on.
    let mut x = 1;
    for (i, label) in STATUS_SLOTS {
        if ind.has(i) {
            fr.chip(x, 10, label, &F3);
        }
        x += text_width(label, &F3, UNGROUPED) + 5;
    }
}

pub fn render(m: &Model, fr: &mut Frame) {
    *fr = Frame::new();
    match m {
        Model::Numbers {
            entry,
            entry_base,
            other,
            other_base,
            ind,
        } => render_numbers(fr, entry, *entry_base, other, *other_base, *ind),
        Model::Bits {
            value,
            word,
            page,
            ind,
        } => render_bits(fr, *value, *word, *page, *ind),
        Model::Status {
            word,
            sign,
            ind,
            vbat_mv,
        } => render_status(fr, *word, *sign, *ind, *vbat_mv),
        Model::Message { top, bottom } => {
            fr.text(1, 0, top.as_bytes(), &F5, UNGROUPED);
            fr.text(1, 9, bottom.as_bytes(), &F5, UNGROUPED);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::Key;
    use std::string::String;

    fn calc(keys: &[Key]) -> Calc {
        let mut c = Calc::new();
        for &k in keys {
            c.key(k);
        }
        c
    }

    fn numbers(m: Model) -> (String, String) {
        let Model::Numbers { entry, other, .. } = m else {
            panic!("not a number screen: {m:?}")
        };
        let s = |t: Text| String::from_utf8(t.as_bytes().into()).unwrap();
        (s(other), s(entry))
    }

    fn frame(m: &Model) -> Frame {
        let mut f = Frame::new();
        render(m, &mut f);
        f
    }

    /// Leftmost lit column in rows y0..y1.
    fn left_edge(f: &Frame, y0: usize, y1: usize) -> usize {
        (0..W).find(|&x| (y0..y1).any(|y| f.get(x, y))).unwrap()
    }

    use Key::{Digit, Div, Enter, Sub, Wsize};

    #[test]
    fn dec_entry_shows_hex_above() {
        let m = model(&calc(&[Digit(2), Sub, Digit(3), Enter]), &Ui::default());
        assert_eq!(numbers(m), ("FFFFFFFF".into(), "-1".into()));
    }

    #[test]
    fn hex_and_oct_entry_show_dec_above() {
        let c = calc(&[Digit(2), Sub, Digit(3), Enter, Key::Base(crate::Base::Hex)]);
        assert_eq!(
            numbers(model(&c, &Ui::default())),
            ("-1".into(), "FFFFFFFF".into())
        );
        let c = calc(&[Digit(8), Key::Base(crate::Base::Oct)]);
        assert_eq!(
            numbers(model(&c, &Ui::default())),
            ("8".into(), "10".into())
        );
    }

    #[test]
    fn typing_shows_digits_unsigned() {
        let c = calc(&[
            Key::Base(crate::Base::Hex),
            Digit(15),
            Digit(15),
            Digit(15),
            Digit(15),
            Digit(15),
            Digit(15),
            Digit(15),
            Digit(15),
        ]);
        assert_eq!(
            numbers(model(&c, &Ui::default())),
            ("-1".into(), "FFFFFFFF".into())
        );
    }

    #[test]
    fn bin_and_status_have_their_own_screens() {
        let c = calc(&[Digit(5), Key::Base(crate::Base::Bin)]);
        assert!(matches!(
            model(&c, &Ui::default()),
            Model::Bits {
                value: 5,
                word: 32,
                ..
            }
        ));
        assert!(matches!(
            model(
                &c,
                &Ui {
                    status: true,
                    ..Ui::default()
                }
            ),
            Model::Status { word: 32, .. }
        ));
    }

    #[test]
    fn error_is_a_message() {
        let m = model(&calc(&[Digit(1), Div, Digit(0), Enter]), &Ui::default());
        assert!(matches!(m, Model::Message { bottom, .. } if bottom.as_bytes() == b"ERROR 0"));
    }

    #[test]
    fn word_size_sets_sign_and_status() {
        let mut c = calc(&[Digit(8), Wsize, Digit(2), Digit(5), Digit(5), Enter]);
        assert_eq!(
            numbers(model(&c, &Ui::default())),
            ("FF".into(), "-1".into())
        );
        let status = model(
            &c,
            &Ui {
                status: true,
                ..Ui::default()
            },
        );
        assert!(matches!(status, Model::Status { word: 8, .. }));
        c.key(Digit(0));
        c.key(Wsize);
        assert_eq!(
            numbers(model(&c, &Ui::default())),
            ("FF".into(), "255".into())
        );
    }

    fn numbers_model(
        entry: &[u8],
        entry_base: Base,
        other: &[u8],
        other_base: Base,
        ind: u16,
    ) -> Model {
        Model::Numbers {
            entry: Text::from(entry),
            entry_base,
            other: Text::from(other),
            other_base,
            ind: Indicators(ind),
        }
    }

    #[test]
    fn indicators_never_touch_the_entry_line() {
        let dec = b"18364758544493064720";
        let plain = frame(&numbers_model(
            b"FEDCBA9876543210",
            Base::Hex,
            dec,
            Base::Dec,
            0,
        ));
        let flags = frame(&numbers_model(
            b"FEDCBA9876543210",
            Base::Hex,
            dec,
            Base::Dec,
            Indicators::C | Indicators::V,
        ));
        assert!((0..W).all(|x| (8..H).all(|y| plain.get(x, y) == flags.get(x, y))));
        // The upper line gives way: 20 digits in 5x7 fill it from x = 2 (the "1" has an
        // empty first column); with C and V on the left they drop to 4x7, from x = 22.
        assert_eq!(left_edge(&plain, 0, 8), 3);
        let first_digit = (12..W).find(|&x| (0..8).any(|y| flags.get(x, y))).unwrap();
        assert_eq!(first_digit, 131 - 110 + 1);
    }

    #[test]
    fn entry_line_shrinks_for_its_own_length() {
        let oct21 = frame(&numbers_model(
            b"777777777777777777777",
            Base::Oct,
            b"0",
            Base::Dec,
            0,
        ));
        let oct22 = frame(&numbers_model(
            b"1777777777777777777777",
            Base::Oct,
            b"0",
            Base::Dec,
            0,
        ));
        // 21 digits: 5x7 ungrouped (125 px + suffix); 22 digits: 4x7 grouped (116 px + suffix).
        assert_eq!(left_edge(&oct21, 9, 16), 131 - 130 + 1);
        assert_eq!(left_edge(&oct22, 9, 16), 131 - 121 + 1);
    }

    #[test]
    fn status_slots_are_fixed() {
        let st = |ind| {
            frame(&Model::Status {
                word: 64,
                sign: Sign::Twos,
                ind: Indicators(ind),
                vbat_mv: None,
            })
        };
        let v_only = st(Indicators::V);
        let c_and_v = st(Indicators::C | Indicators::V);
        // V stays in its place whether C is on or not.
        assert!((5..W).all(|x| (9..H).all(|y| v_only.get(x, y) == c_and_v.get(x, y))));
    }

    #[test]
    fn bin_page_mark_keeps_a_gap() {
        let f = frame(&Model::Bits {
            value: u128::MAX,
            word: 128,
            page: 1,
            ind: Indicators::default(),
        });
        let col: std::vec::Vec<bool> = (0..H).map(|y| f.get(0, y)).collect();
        assert!(col[..8].iter().all(|&b| b));
        assert!(!col[8]);
        assert!(col[9]);
    }
}
