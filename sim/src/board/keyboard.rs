//! Key matrix state and key naming (grid positions, HP key codes, labels).

use serde::{Deserialize, Serialize};

/// A physical key: matrix column 0..9 and row 0..3, or the ON key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Matrix { col: u8, row: u8 },
    On,
}

impl Key {
    /// Key at HP grid position `row-col` (rows 1..4, cols 1..10 with 0 = 10).
    /// Grid 4-6 does not exist (ENTER is 3-6); 4-1 is ON.
    pub fn from_grid(row: u8, col: u8) -> Option<Key> {
        let col = if col == 0 { 10 } else { col };
        if !(1..=4).contains(&row) || !(1..=10).contains(&col) {
            return None;
        }
        if row == 4 && col == 1 {
            return Some(Key::On);
        }
        if row == 4 && col == 6 {
            return Some(Key::Matrix { col: 5, row: 2 });
        }
        Some(Key::Matrix { col: col - 1, row: row - 1 })
    }

    pub fn grid(&self) -> (u8, u8) {
        match *self {
            Key::On => (4, 1),
            Key::Matrix { col, row } => (row + 1, (col + 1) % 10),
        }
    }

    pub fn grid_name(&self) -> String {
        let (r, c) = self.grid();
        format!("{r}-{c}")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keyboard {
    /// pressed[col] bit `row`.
    pub pressed: [u8; 10],
    pub on: bool,
}

impl Keyboard {
    pub fn set(&mut self, key: Key, down: bool) {
        match key {
            Key::On => self.on = down,
            Key::Matrix { col, row } => {
                // ENTER is a single switch; the firmware decodes it at C5·R3 and C5·R4.
                let row = if col == 5 && row == 3 { 2 } else { row };
                let bit = 1 << row;
                if down {
                    self.pressed[col as usize] |= bit;
                } else {
                    self.pressed[col as usize] &= !bit;
                }
            }
        }
    }

    pub fn release_all(&mut self) {
        self.pressed = [0; 10];
        self.on = false;
    }

    pub fn any(&self) -> bool {
        self.on || self.pressed.iter().any(|&b| b != 0)
    }
}
