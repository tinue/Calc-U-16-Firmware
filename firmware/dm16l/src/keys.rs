//! Keyboard: 10 columns driven low one at a time, 4 rows read through pull-ups,
//! ON on its own line (see docs/keyboard.md).

use crate::regs::{Pin, PULL_NONE, PULL_UP};
use cortex_m::asm::delay;

pub const COLUMNS: [Pin; 10] = [
    Pin(2, 7),
    Pin(2, 8),
    Pin(2, 1),
    Pin(0, 3),
    Pin(1, 9),
    Pin(3, 4),
    Pin(2, 4),
    Pin(2, 5),
    Pin(3, 5),
    Pin(0, 6),
];
const ROWS: [Pin; 4] = [Pin(0, 7), Pin(0, 8), Pin(0, 10), Pin(1, 0)];
pub const ON_LINE: Pin = Pin(0, 11);
/// The ON key connects PIO0_11 to column C0.
pub const ON_DRIVE: Pin = Pin(2, 7);

/// A physical key: grid row 1..4 and column 1..10 (as printed in docs/keyboard.md), or ON.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhysKey {
    Grid { row: u8, col: u8 },
    On,
}

/// Raw key state: bit (row) per column, plus ON.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    cols: [u8; 10],
    on: bool,
}

impl State {
    /// Debug text: ON flag, then one hex digit (row bits) per column C0..C9.
    pub fn hex(&self) -> [u8; 12] {
        let mut out = [b' '; 12];
        out[0] = if self.on { b'O' } else { b'-' };
        for (i, &c) in self.cols.iter().enumerate() {
            out[2 + i] = b"0123456789ABCDEF"[c as usize & 15];
        }
        out
    }

    pub fn pressed(&self, key: PhysKey) -> bool {
        match key {
            PhysKey::On => self.on,
            PhysKey::Grid { row, col } => self.cols[(col - 1) as usize] & (1 << (row - 1)) != 0,
        }
    }

    /// First key pressed in `self` but not in `before` (ON first, then row by row).
    fn new_press(&self, before: &State) -> Option<PhysKey> {
        if self.on && !before.on {
            return Some(PhysKey::On);
        }
        for row in 0..4u8 {
            for col in 0..10u8 {
                let bit = 1 << row;
                if self.cols[col as usize] & bit != 0 && before.cols[col as usize] & bit == 0 {
                    // ENTER spans grid 3-6 and 4-6; report it as 3-6.
                    let row = if col == 5 && row == 3 { 2 } else { row };
                    return Some(PhysKey::Grid {
                        row: row + 1,
                        col: col + 1,
                    });
                }
            }
        }
        None
    }
}

pub fn init() {
    for p in COLUMNS.iter().chain(ROWS.iter()).chain([ON_LINE].iter()) {
        p.configure(PULL_UP);
        p.input();
    }
}

/// For deep sleep: all columns driven low with their pull-ups off. A pull-up on a pin
/// driven low wastes tens of µA; with every column low, ON (via C0) can wake the chip.
pub fn park_columns() {
    for p in COLUMNS {
        p.configure(PULL_NONE);
        p.output(false);
    }
}

/// Undoes `park_columns`: columns back to inputs with pull-ups.
pub fn unpark_columns() {
    for p in COLUMNS {
        p.input();
        p.configure(PULL_UP);
    }
}

/// Reads the whole keyboard once.
pub fn scan() -> State {
    let mut s = State::default();
    for (c, col) in COLUMNS.iter().enumerate() {
        col.output(false);
        delay(60);
        for (r, row) in ROWS.iter().enumerate() {
            if !row.read() {
                s.cols[c] |= 1 << r;
            }
        }
        col.input();
    }
    // ON connects PIO0_11 to column C0. Count it as pressed only if the line follows
    // C0: low while C0 is driven low, high again when C0 is released. A line held
    // low by something else (e.g. the RTC's /INT output) is not a key press.
    ON_DRIVE.output(false);
    delay(60);
    let low_when_driven = !ON_LINE.read();
    ON_DRIVE.input();
    delay(60);
    let high_when_released = ON_LINE.read();
    s.on = low_when_driven && high_when_released;
    s
}

/// Debouncer: a change must be seen on two consecutive scans.
pub struct Keyboard {
    stable: State,
    last: State,
}

impl Keyboard {
    pub const fn new() -> Self {
        Keyboard {
            stable: State {
                cols: [0; 10],
                on: false,
            },
            last: State {
                cols: [0; 10],
                on: false,
            },
        }
    }

    /// Accepts the current state as stable without generating presses (e.g. after wake-up).
    pub fn sync(&mut self) {
        self.last = scan();
        self.stable = self.last;
    }

    /// Feeds one scan result and returns a newly pressed key, if any.
    pub fn update(&mut self, now: State) -> Option<PhysKey> {
        let mut press = None;
        if now == self.last && now != self.stable {
            press = now.new_press(&self.stable);
            self.stable = now;
        }
        self.last = now;
        press
    }
}
