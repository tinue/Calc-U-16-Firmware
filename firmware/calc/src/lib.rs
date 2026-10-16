//! Calc-U-16 calculator core.
//!
//! Scope: integer mode only, 2's complement, four bases, word size 1..64 bits
//! (WSIZE). Entry is algebraic in strict order of entry: `5 + 4 × 3 ENTER` gives 27
//! (`9` shows when × is pressed). ENTER works as `=`. Arithmetic wraps within the
//! word size.

#![no_std]

pub mod display;
pub mod fonts;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    Hex,
    Dec,
    Oct,
    Bin,
}

impl Base {
    pub fn radix(self) -> u64 {
        match self {
            Base::Hex => 16,
            Base::Dec => 10,
            Base::Oct => 8,
            Base::Bin => 2,
        }
    }

    /// Display suffix, as on the HP-16C.
    pub fn suffix(self) -> u8 {
        match self {
            Base::Hex => b'h',
            Base::Dec => b'd',
            Base::Oct => b'o',
            Base::Bin => b'b',
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    /// Digit 0..15 (A..F are 10..15).
    Digit(u8),
    /// `=`: completes the pending operation.
    Enter,
    Add,
    Sub,
    Mul,
    Div,
    Base(Base),
    /// ← : deletes the last digit during entry, otherwise clears X.
    Back,
    /// CHS: 2's complement negation.
    Chs,
    /// f WSIZE: X is the new word size (postfix: `32 WSIZE`); 0 means 64.
    Wsize,
}

/// Error codes shown as "Error n" (HP-16C numbering).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// 0: improper mathematical operation.
    DivideByZero = 0,
    /// 2: improper bit number (word size above 64).
    WordSize = 2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

pub const DEFAULT_WORD: u8 = 32;

#[derive(Clone, Debug)]
pub struct Calc {
    /// The value shown: the number being typed, or the last result.
    x: u64,
    pub base: Base,
    /// Word size in bits, 1..=64.
    pub word: u8,
    /// True while digits are being typed into X.
    pub entering: bool,
    /// Left operand and operator, waiting for the right operand.
    pending: Option<(u64, Op)>,
    /// X was typed or changed since the last operator, so it is a new operand.
    operand: bool,
    /// X before the current entry started (shown again after WSIZE).
    before_entry: u64,
    pub error: Option<Error>,
}

impl Default for Calc {
    fn default() -> Self {
        Calc {
            x: 0,
            base: Base::Dec,
            word: DEFAULT_WORD,
            entering: false,
            pending: None,
            operand: false,
            before_entry: 0,
            error: None,
        }
    }
}

impl Calc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn x(&self) -> u64 {
        self.x
    }

    /// All ones within the word size.
    pub fn mask(&self) -> u64 {
        u64::MAX >> (64 - self.word as u32)
    }

    /// `v` as a signed (2's complement) value of the word size.
    pub fn signed(&self, v: u64) -> i64 {
        let shift = 64 - self.word as u32;
        ((v << shift) as i64) >> shift
    }

    pub fn key(&mut self, key: Key) {
        // Any key clears an error display and is otherwise ignored.
        if self.error.take().is_some() {
            return;
        }
        match key {
            Key::Digit(d) => self.digit(d as u64),
            Key::Add => self.operator(Op::Add),
            Key::Sub => self.operator(Op::Sub),
            Key::Mul => self.operator(Op::Mul),
            Key::Div => self.operator(Op::Div),
            Key::Enter => {
                self.entering = false;
                if let Some((acc, op)) = self.pending.take() {
                    if self.operand {
                        self.apply(acc, op);
                    }
                }
                self.operand = false;
            }
            Key::Base(b) => {
                self.entering = false;
                self.base = b;
            }
            Key::Back => {
                if self.entering {
                    self.x /= self.base.radix();
                    self.entering = self.x != 0;
                } else {
                    self.x = 0;
                }
                self.operand = true;
            }
            Key::Chs => {
                self.entering = false;
                self.x = self.x.wrapping_neg() & self.mask();
                self.operand = true;
            }
            Key::Wsize => self.wsize(),
        }
    }

    /// Strict order of entry: a pending operation is completed first, and its
    /// result becomes the left operand of `op`.
    fn operator(&mut self, op: Op) {
        self.entering = false;
        match self.pending {
            Some((acc, prev)) if self.operand => {
                if !self.apply(acc, prev) {
                    return;
                }
            }
            // Two operators in a row: the second replaces the first.
            Some((acc, _)) => self.x = acc,
            None => {}
        }
        self.pending = Some((self.x, op));
        self.operand = false;
    }

    /// X = acc op X. False (and the error shown) if the operation is improper.
    fn apply(&mut self, acc: u64, op: Op) -> bool {
        let (a, b) = (acc, self.x);
        let r = match op {
            Op::Add => a.wrapping_add(b),
            Op::Sub => a.wrapping_sub(b),
            Op::Mul => a.wrapping_mul(b),
            Op::Div => {
                if b == 0 {
                    self.error = Some(Error::DivideByZero);
                    self.pending = None;
                    return false;
                }
                self.signed(a).wrapping_div(self.signed(b)) as u64
            }
        };
        self.x = r & self.mask();
        true
    }

    fn wsize(&mut self) {
        let n = self.x;
        if n > 64 {
            self.error = Some(Error::WordSize);
            return;
        }
        // The size was a parameter, not a value: show the number from before it was typed.
        if self.entering {
            self.entering = false;
            self.x = self.before_entry;
            self.operand = false;
        }
        self.word = if n == 0 { 64 } else { n as u8 };
        let mask = self.mask();
        self.x &= mask;
        if let Some((acc, op)) = self.pending {
            self.pending = Some((acc & mask, op));
        }
    }

    fn digit(&mut self, d: u64) {
        let radix = self.base.radix();
        if d >= radix {
            return;
        }
        if !self.entering {
            self.before_entry = self.x;
            self.x = 0;
            self.entering = true;
            self.operand = true;
        }
        // Digits that would not fit in the word size are ignored.
        if let Some(v) = self.x.checked_mul(radix).and_then(|v| v.checked_add(d)) {
            if v <= self.mask() {
                self.x = v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn run(keys: &[Key]) -> Calc {
        let mut c = Calc::new();
        for &k in keys {
            c.key(k);
        }
        c
    }

    /// Parses "HEX FF + DEC 55 =" into keys; number tokens become digit keys.
    fn k(s: &str) -> Vec<Key> {
        let mut out = Vec::new();
        for t in s.split_whitespace() {
            match t {
                "=" | "ENTER" => out.push(Key::Enter),
                "+" => out.push(Key::Add),
                "-" => out.push(Key::Sub),
                "*" => out.push(Key::Mul),
                "/" => out.push(Key::Div),
                "HEX" => out.push(Key::Base(Base::Hex)),
                "DEC" => out.push(Key::Base(Base::Dec)),
                "OCT" => out.push(Key::Base(Base::Oct)),
                "BIN" => out.push(Key::Base(Base::Bin)),
                "BSP" => out.push(Key::Back),
                "CHS" => out.push(Key::Chs),
                "WSIZE" => out.push(Key::Wsize),
                d => out.extend(d.chars().map(|c| Key::Digit(c.to_digit(16).unwrap() as u8))),
            }
        }
        out
    }

    fn x(s: &str) -> u64 {
        run(&k(s)).x()
    }

    #[test]
    fn strict_order_of_entry() {
        assert_eq!(x("5 + 4 * 3 ="), 27);
        // The intermediate result shows when the next operator is pressed.
        assert_eq!(x("5 + 4 *"), 9);
        assert_eq!(x("2 * 3 + 4 ="), 10);
    }

    #[test]
    fn hex_ff_plus_dec_55() {
        let c = run(&k("HEX FF + DEC 55 ="));
        assert_eq!(c.x(), 310);
        assert_eq!(c.base, Base::Dec);
    }

    #[test]
    fn second_operator_replaces_the_first() {
        assert_eq!(x("5 + * 3 ="), 15);
    }

    #[test]
    fn result_continues_as_left_operand() {
        assert_eq!(x("5 + 5 = * 2 ="), 20);
        // A new number after = starts over.
        assert_eq!(x("5 + 5 = 7 + 1 ="), 8);
    }

    #[test]
    fn enter_without_right_operand_drops_the_operator() {
        assert_eq!(x("5 + ="), 5);
        assert_eq!(x("5 + = 1 ="), 1);
    }

    #[test]
    fn subtraction_wraps() {
        let c = run(&k("2 - 3 ="));
        assert_eq!(c.x(), 0xFFFF_FFFF);
        assert_eq!(c.signed(c.x()), -1);
    }

    #[test]
    fn division_is_signed_and_truncates() {
        assert_eq!(x("7 / 2 ="), 3);
        let c = run(&k("7 CHS / 2 ="));
        assert_eq!(c.signed(c.x()), -3);
    }

    #[test]
    fn divide_by_zero() {
        let mut c = run(&k("7 / 0 ="));
        assert_eq!(c.error, Some(Error::DivideByZero));
        // The next key only clears the error; the pending operation is gone.
        c.key(Key::Add);
        assert_eq!(c.error, None);
        assert_eq!(c.x(), 0);
    }

    #[test]
    fn digits_outside_base_are_ignored() {
        assert_eq!(x("OCT 9 7"), 7);
        assert_eq!(x("BIN 2 1 0 1"), 0b101);
        assert_eq!(x("DEC A 5"), 5);
    }

    #[test]
    fn entry_stops_at_word_size() {
        assert_eq!(x("HEX FFFFFFFF F"), 0xFFFF_FFFF);
        assert_eq!(x("DEC 4294967295 9"), 4294967295);
        assert_eq!(x("8 WSIZE HEX FFF"), 0xFF);
    }

    #[test]
    fn multiplication_wraps() {
        assert_eq!(x("HEX 10000 * 10000 ="), 0);
    }

    #[test]
    fn backspace() {
        let c = run(&k("123 BSP"));
        assert_eq!(c.x(), 12);
        assert!(c.entering);
        // Outside entry it clears X; as the right operand, 0 is then used.
        assert_eq!(x("5 + 7 = BSP"), 0);
        assert_eq!(x("9 + 4 BSP BSP 6 ="), 15);
    }

    #[test]
    fn base_change_keeps_the_entry() {
        assert_eq!(x("HEX 1F + DEC 1 ="), 0x20);
    }

    #[test]
    fn wsize_is_postfix_and_restores_the_value() {
        let c = run(&k("1234 = 16 WSIZE"));
        assert_eq!((c.word, c.x()), (16, 1234));
        // 0 means 64; above 64 is Error 2 and changes nothing.
        assert_eq!(run(&k("0 WSIZE")).word, 64);
        let c = run(&k("65 WSIZE"));
        assert_eq!((c.word, c.error), (DEFAULT_WORD, Some(Error::WordSize)));
        // A result can be used directly.
        assert_eq!(run(&k("4 * 4 = WSIZE")).word, 16);
    }

    #[test]
    fn wsize_truncates() {
        // HP-16C handbook example: 32767 at 16 bits becomes -1 at 8 bits.
        let c = run(&k("16 WSIZE 32767 = 8 WSIZE"));
        assert_eq!((c.x(), c.signed(c.x())), (0xFF, -1));
        // A pending operation keeps going with the truncated left operand.
        assert_eq!(x("300 + 8 WSIZE 1 ="), 45);
    }

    #[test]
    fn word_size_64() {
        assert_eq!(x("0 WSIZE HEX FFFFFFFFFFFFFFFF + 1 ="), 0);
        let c = run(&k("0 WSIZE 1 CHS"));
        assert_eq!(c.signed(c.x()), -1);
    }
}
