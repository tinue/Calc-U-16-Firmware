//! Keyboard legends of the Voyager models (grid row 1..4, column 1..10).
//!
//! DM16 legends are complete (base, yellow/f, blue/g; see docs/keyboard.md). For the
//! DM12 and DM15 only the base legends are listed.

use crate::board::keyboard::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Dm12,
    Dm15,
    Dm16,
}

#[derive(Debug, Clone, Copy)]
pub struct Legend {
    pub base: &'static str,
    pub f: &'static str,
    pub g: &'static str,
}

const fn l(base: &'static str, f: &'static str, g: &'static str) -> Legend {
    Legend { base, f, g }
}

const DM16: [[Legend; 10]; 4] = [
    [
        l("A", "SL", "LJ"),
        l("B", "SR", "ASR"),
        l("C", "RL", "RLC"),
        l("D", "RR", "RRC"),
        l("E", "RLn", "RLCn"),
        l("F", "RRn", "RRCn"),
        l("7", "MASKL", "#B"),
        l("8", "MASKR", "ABS"),
        l("9", "RMD", "DBLR"),
        l("÷", "XOR", "DBL÷"),
    ],
    [
        l("GSB", "x≷(i)", "RTN"),
        l("GTO", "x≷I", "LBL"),
        l("HEX", "SHOW HEX", "DSZ"),
        l("DEC", "SHOW DEC", "ISZ"),
        l("OCT", "SHOW OCT", "√x"),
        l("BIN", "SHOW BIN", "1/x"),
        l("4", "SB", "SF"),
        l("5", "CB", "CF"),
        l("6", "B?", "F?"),
        l("×", "AND", "DBL×"),
    ],
    [
        l("R/S", "(i)", "P/R"),
        l("SST", "I", "BST"),
        l("R↓", "CLEAR PRGM", "R↑"),
        l("x≷y", "CLEAR REG", "PSE"),
        l("←", "CLEAR PREFIX", "CLx"),
        l("ENTER", "WINDOW", "LSTx"),
        l("1", "1'S", "x≤y"),
        l("2", "2'S", "x<0"),
        l("3", "UNSGN", "x>y"),
        l("−", "NOT", "x>0"),
    ],
    [
        l("ON", "", ""),
        l("f", "", ""),
        l("g", "", ""),
        l("STO", "WSIZE", "<"),
        l("RCL", "FLOAT", ">"),
        l("ENTER", "", ""),
        l("0", "MEM", "x≠y"),
        l(".", "STATUS", "x≠0"),
        l("CHS", "EEX", "x=y"),
        l("+", "OR", "x=0"),
    ],
];

const fn b(base: &'static str) -> Legend {
    Legend { base, f: "", g: "" }
}

const DM15: [[Legend; 10]; 4] = [
    [b("√x"), b("eˣ"), b("10ˣ"), b("yˣ"), b("1/x"), b("CHS"), b("7"), b("8"), b("9"), b("÷")],
    [b("SST"), b("GTO"), b("SIN"), b("COS"), b("TAN"), b("EEX"), b("4"), b("5"), b("6"), b("×")],
    [b("R/S"), b("GSB"), b("R↓"), b("x≷y"), b("←"), b("ENTER"), b("1"), b("2"), b("3"), b("−")],
    [b("ON"), b("f"), b("g"), b("STO"), b("RCL"), b("ENTER"), b("0"), b("."), b("Σ+"), b("+")],
];

const DM12: [[Legend; 10]; 4] = [
    [b("n"), b("i"), b("PV"), b("PMT"), b("FV"), b("CHS"), b("7"), b("8"), b("9"), b("÷")],
    [b("yˣ"), b("1/x"), b("%T"), b("Δ%"), b("%"), b("EEX"), b("4"), b("5"), b("6"), b("×")],
    [b("R/S"), b("SST"), b("R↓"), b("x≷y"), b("CLx"), b("ENTER"), b("1"), b("2"), b("3"), b("−")],
    [b("ON"), b("f"), b("g"), b("STO"), b("RCL"), b("ENTER"), b("0"), b("."), b("Σ+"), b("+")],
];

/// ASCII spellings accepted for legends in scripts and commands.
const ALIASES: &[(&str, &str)] = &[
    ("/", "÷"),
    ("*", "×"),
    ("-", "−"),
    ("x<>y", "x≷y"),
    ("xy", "x≷y"),
    ("rdn", "R↓"),
    ("rup", "R↑"),
    ("bsp", "←"),
    ("back", "←"),
    ("<-", "←"),
    ("sqrt", "√x"),
    ("x<>(i)", "x≷(i)"),
    ("x<>i", "x≷I"),
    ("dbl/", "DBL÷"),
    ("dbl*", "DBL×"),
    ("x<=y", "x≤y"),
    ("x!=y", "x≠y"),
    ("x!=0", "x≠0"),
    ("y^x", "yˣ"),
    ("e^x", "eˣ"),
    ("10^x", "10ˣ"),
    ("sigma+", "Σ+"),
    ("enter", "ENTER"),
];

/// Shift prefix needed to reach a legend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    None,
    F,
    G,
}

impl Model {
    pub fn parse(s: &str) -> Option<Model> {
        match s.to_ascii_lowercase().as_str() {
            "dm12" | "12" | "12c" => Some(Model::Dm12),
            "dm15" | "15" | "15c" => Some(Model::Dm15),
            "dm16" | "16" | "16c" => Some(Model::Dm16),
            _ => None,
        }
    }

    /// Guesses the model from the firmware image ("DM12", "DM15", "DM16" strings).
    pub fn detect(flash: &[u8]) -> Model {
        let find = |s: &[u8]| flash.windows(s.len()).any(|w| w == s);
        if find(b"DM12") {
            Model::Dm12
        } else if find(b"DM15") {
            Model::Dm15
        } else {
            Model::Dm16
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Model::Dm12 => "DM12",
            Model::Dm15 => "DM15",
            Model::Dm16 => "DM16",
        }
    }

    pub fn legends(&self) -> &'static [[Legend; 10]; 4] {
        match self {
            Model::Dm12 => &DM12,
            Model::Dm15 => &DM15,
            Model::Dm16 => &DM16,
        }
    }

    pub fn legend(&self, key: Key) -> Legend {
        let (r, c) = key.grid();
        let c = if c == 0 { 10 } else { c };
        self.legends()[(r - 1) as usize][(c - 1) as usize]
    }

    /// Resolves a key name: grid "r-c", a base legend, or a shifted legend (returns the shift).
    pub fn resolve(&self, name: &str) -> Option<(Shift, Key)> {
        if let Some((r, c)) = name.split_once('-') {
            if let (Ok(r), Ok(c)) = (r.parse::<u8>(), c.parse::<u8>()) {
                return Key::from_grid(r, c).map(|k| (Shift::None, k));
            }
        }
        let canon = ALIASES.iter().find(|(a, _)| a.eq_ignore_ascii_case(name)).map_or(name, |(_, c)| *c);
        let norm = |s: &str| s.replace(' ', "").to_lowercase();
        let want = norm(canon);
        let exact = |s: &str| s == canon;
        let loose = |s: &str| !s.is_empty() && norm(s) == want;
        for matcher in [&exact as &dyn Fn(&str) -> bool, &loose] {
            for shift in [Shift::None, Shift::F, Shift::G] {
                for (r, row) in self.legends().iter().enumerate() {
                    for (c, lg) in row.iter().enumerate() {
                        let s = match shift {
                            Shift::None => lg.base,
                            Shift::F => lg.f,
                            Shift::G => lg.g,
                        };
                        if matcher(s) {
                            // Grid 4-6 (lower half of ENTER) resolves to 3-6.
                            let key = Key::from_grid(r as u8 + 1, (c as u8 + 1) % 10)?;
                            return Some((shift, key));
                        }
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_names() {
        let m = Model::Dm16;
        assert_eq!(m.resolve("ENTER"), Some((Shift::None, Key::Matrix { col: 5, row: 2 })));
        assert_eq!(m.resolve("on"), Some((Shift::None, Key::On)));
        assert_eq!(m.resolve("SL"), Some((Shift::F, Key::Matrix { col: 0, row: 0 })));
        assert_eq!(m.resolve("CLx"), Some((Shift::G, Key::Matrix { col: 4, row: 2 })));
        assert_eq!(m.resolve("2-3"), Some((Shift::None, Key::Matrix { col: 2, row: 1 })));
        assert_eq!(m.resolve("+"), Some((Shift::None, Key::Matrix { col: 9, row: 3 })));
        assert_eq!(m.resolve("show hex"), Some((Shift::F, Key::Matrix { col: 2, row: 1 })));
        assert_eq!(m.resolve("f"), Some((Shift::None, Key::Matrix { col: 1, row: 3 })));
    }
}
