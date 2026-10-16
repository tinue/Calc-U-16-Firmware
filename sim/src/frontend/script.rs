//! Script language for headless runs and the TUI command line.
//!
//! One command per line; `#` starts a comment. Durations: `150ms`, `2s`, `3m`, plain number = ms.
//!
//! ```text
//! keys 2 ENTER 3 +          # press keys in turn; shifted legends add f/g ("keys SL", "keys CLx")
//! press ON 300ms            # press one key (by legend or grid "r-c") for a given time
//! hold ON / release ON / release all
//! wait 2s
//! screen                    # print the LCD as text art
//! expect-screen file.txt    # compare with a golden file (UPDATE_SNAPSHOTS=1 rewrites it)
//! serial "?\r"              # send bytes to the UART (escapes \r \n \t \\ \")
//! expect-serial "VOYAGER" [5s]
//! expect-halt bootloader [10s]
//! expect-beeps 2
//! expect-display on|off
//! vbat 2900 / rtc 2026-10-16T08:00:00 / rtc-int / reset / state save|load FILE / echo on|off
//! ```

use crate::board::keyboard::Key;
use crate::cpu::Halt;
use crate::machine::Machine;
use crate::models::{Model, Shift};
use anyhow::{anyhow, bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn parse_duration(s: &str) -> Result<f64> {
    let s = s.trim();
    let (num, mul) = if let Some(n) = s.strip_suffix("ms") {
        (n, 1e-3)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60.0)
    } else {
        (s, 1e-3)
    };
    Ok(num.parse::<f64>().with_context(|| format!("bad duration {s:?}"))? * mul)
}

/// Splits a line into tokens, honouring double quotes and escapes inside them.
pub fn tokenize(line: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            break;
        } else if c == '"' {
            chars.next();
            let mut s = String::new();
            loop {
                match chars.next() {
                    None => bail!("unterminated string"),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some('r') => s.push('\r'),
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('e') => s.push('\x1b'),
                        Some(o) => s.push(o),
                        None => bail!("bad escape"),
                    },
                    Some(o) => s.push(o),
                }
            }
            out.push(s);
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                chars.next();
            }
            out.push(s);
        }
    }
    Ok(out)
}

pub struct Runner<'a> {
    pub m: &'a mut Machine,
    pub model: Model,
    pub out: Box<dyn Write + 'a>,
    /// Directory for relative file names (snapshots, state).
    pub dir: PathBuf,
    /// Serial output not yet consumed by `expect-serial`.
    pub serial: String,
    pub echo_serial: bool,
    pub press_time: f64,
    pub gap_time: f64,
    pub update_snapshots: bool,
    pub failures: Vec<String>,
}

impl<'a> Runner<'a> {
    pub fn new(m: &'a mut Machine, model: Model, out: Box<dyn Write + 'a>, dir: &Path) -> Self {
        Runner {
            m,
            model,
            out,
            dir: dir.to_path_buf(),
            serial: String::new(),
            echo_serial: false,
            press_time: 0.12,
            gap_time: 0.25,
            update_snapshots: std::env::var_os("UPDATE_SNAPSHOTS").is_some(),
            failures: Vec::new(),
        }
    }

    /// Advances emulated time, collecting serial output and trace lines.
    pub fn advance(&mut self, dt: f64) {
        let end = self.m.now() + dt;
        while self.m.now() < end && self.m.halted().is_none() {
            let step = (end - self.m.now()).min(0.05);
            self.m.run_for(step);
            self.collect();
        }
        self.collect();
    }

    fn collect(&mut self) {
        let bytes = self.m.serial_take();
        if !bytes.is_empty() {
            let s = String::from_utf8_lossy(&bytes);
            if self.echo_serial {
                let _ = write!(self.out, "{s}");
            }
            self.serial.push_str(&s);
        }
        for l in self.m.take_log() {
            let _ = writeln!(self.out, "{l}");
        }
    }

    fn resolve(&self, name: &str) -> Result<(Shift, Key)> {
        self.model.resolve(name).ok_or_else(|| anyhow!("unknown key {name:?} for {}", self.model.name()))
    }

    pub fn tap(&mut self, key: Key, hold: f64) {
        self.m.key(key, true);
        self.advance(hold);
        self.m.key(key, false);
        self.advance(self.gap_time);
    }

    fn shift_key(&self, shift: Shift) -> Option<Key> {
        let name = match shift {
            Shift::None => return None,
            Shift::F => "f",
            Shift::G => "g",
        };
        self.model.resolve(name).map(|(_, k)| k)
    }

    /// Presses a key by name, adding the f/g prefix for shifted legends.
    pub fn press_named(&mut self, name: &str, hold: f64) -> Result<()> {
        let (shift, key) = self.resolve(name)?;
        if let Some(sk) = self.shift_key(shift) {
            self.tap(sk, self.press_time);
        }
        self.tap(key, hold);
        Ok(())
    }

    fn fail(&mut self, msg: String) {
        let _ = writeln!(self.out, "FAIL: {msg}");
        self.failures.push(msg);
    }

    fn path(&self, p: &str) -> PathBuf {
        let p = Path::new(p);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.dir.join(p)
        }
    }

    pub fn exec(&mut self, line: &str) -> Result<()> {
        let t = tokenize(line)?;
        let Some(cmd) = t.first() else { return Ok(()) };
        let arg = |i: usize| t.get(i).map(String::as_str).ok_or_else(|| anyhow!("{cmd}: missing argument"));
        match cmd.as_str() {
            "keys" | "k" => {
                for name in &t[1..] {
                    self.press_named(name, self.press_time)?;
                }
            }
            "press" | "p" => {
                let hold = t.get(2).map(|d| parse_duration(d)).transpose()?.unwrap_or(self.press_time);
                self.press_named(arg(1)?, hold)?;
            }
            "hold" => {
                let (_, key) = self.resolve(arg(1)?)?;
                self.m.key(key, true);
                self.advance(self.press_time);
            }
            "release" => {
                if arg(1)? == "all" {
                    self.m.release_all();
                } else {
                    let (_, key) = self.resolve(arg(1)?)?;
                    self.m.key(key, false);
                }
                self.advance(self.gap_time);
            }
            "wait" | "run" => self.advance(parse_duration(arg(1)?)?),
            "screen" => {
                let s = self.m.screen_text();
                let on = if self.m.bus.lcd.display_on { "" } else { " (display off)" };
                writeln!(self.out, "screen at {:.3}s{on}:\n{s}", self.m.now())?;
            }
            "expect-screen" => {
                let path = self.path(arg(1)?);
                let actual = self.m.screen_text();
                if self.update_snapshots || !path.exists() {
                    std::fs::write(&path, &actual)?;
                    writeln!(self.out, "wrote snapshot {}", path.display())?;
                } else {
                    let expected = std::fs::read_to_string(&path)?;
                    if expected != actual {
                        self.fail(format!("screen differs from {}:\n{actual}", path.display()));
                    }
                }
            }
            "expect-display" => {
                let want = arg(1)? == "on";
                if self.m.bus.lcd.display_on != want {
                    self.fail(format!("display is {}", if want { "off" } else { "on" }));
                }
            }
            "serial" | "send" => {
                let bytes = arg(1)?.as_bytes().to_vec();
                self.m.serial_send(&bytes);
                // Let the bytes arrive (38400 baud ≈ 0.26 ms per byte).
                self.advance(bytes.len() as f64 * 0.0003 + 0.01);
            }
            "expect-serial" => {
                let want = arg(1)?.to_string();
                let timeout = t.get(2).map(|d| parse_duration(d)).transpose()?.unwrap_or(5.0);
                let deadline = self.m.now() + timeout;
                loop {
                    if let Some(pos) = self.serial.find(&want) {
                        self.serial.drain(..pos + want.len());
                        break;
                    }
                    if self.m.now() >= deadline || self.m.halted().is_some() {
                        let got = std::mem::take(&mut self.serial);
                        self.fail(format!("serial output {want:?} not seen; got {got:?}"));
                        break;
                    }
                    self.advance(0.01);
                }
            }
            "print-serial" => {
                let s = std::mem::take(&mut self.serial);
                writeln!(self.out, "{s}")?;
            }
            "expect-halt" => {
                let timeout = t.get(2).map(|d| parse_duration(d)).transpose()?.unwrap_or(10.0);
                let deadline = self.m.now() + timeout;
                while self.m.halted().is_none() && self.m.now() < deadline {
                    self.advance(0.05);
                }
                let ok = matches!((arg(1)?, self.m.halted()), ("bootloader", Some(Halt::Bootloader)) | ("any", Some(_)));
                if !ok {
                    self.fail(format!("expected halt {:?}, got {:?}", arg(1)?, self.m.halted()));
                }
            }
            "expect-beeps" => {
                let want: usize = arg(1)?.parse()?;
                self.advance(0.05);
                let got = self.m.bus.beeper.beeps.len();
                if got != want {
                    self.fail(format!("expected {want} beeps, got {got}"));
                }
            }
            "beeps" => {
                for (t0, f, d) in &self.m.bus.beeper.beeps {
                    writeln!(self.out, "beep at {t0:.3}s: {f:.0} Hz, {:.0} ms", d * 1000.0)?;
                }
            }
            "vbat" => self.m.bus.vbat_mv = arg(1)?.parse()?,
            "rtc-int" => {
                // Leave the RTC timer flag + interrupt enable set (its /INT pulls the ON line low),
                // as the vendor firmware can leave it.
                self.m.bus.rtc.ctrl2 |= 0x05;
                self.m.bus.update_pins();
            }
            "rtc" => {
                let t = chrono::NaiveDateTime::parse_from_str(arg(1)?, "%Y-%m-%dT%H:%M:%S")?;
                self.m.bus.rtc.now_s = self.m.now();
                self.m.bus.rtc.set_time(t);
            }
            "reset" => self.m.reset(),
            "state" => {
                let path = self.path(arg(2)?);
                match arg(1)? {
                    "save" => self.m.save_state(&path)?,
                    "load" => self.m.load_state(&path)?,
                    o => bail!("state {o}: expected save or load"),
                }
            }
            "echo" => self.echo_serial = arg(1)? == "on",
            "timing" => {
                self.press_time = parse_duration(arg(1)?)?;
                self.gap_time = parse_duration(arg(2)?)?;
            }
            "iocon" => {
                let b = &self.m.bus;
                let mut s = String::new();
                for (off, p) in crate::periph::iocon::IOCON_MAP {
                    let g = &b.gpio[p.0 as usize];
                    s.push_str(&format!(
                        "{:<8} iocon[{off:#05x}]={:#06x} dir={} out={} level={}\n",
                        p.to_string(),
                        b.iocon.reg(*p),
                        (g.dir >> p.1) & 1,
                        (g.data >> p.1) & 1,
                        b.pin_level(*p) as u8
                    ));
                }
                write!(self.out, "{s}")?;
            }
            "pins" => {
                let b = &self.m.bus;
                let mut s = String::new();
                for (name, pins) in [("col", &crate::board::COLUMNS[..]), ("row", &crate::board::ROWS[..])] {
                    for (i, &p) in pins.iter().enumerate() {
                        let g = &b.gpio[p.0 as usize];
                        s.push_str(&format!(
                            "{name}{i} {p}: iocon={:#05x} dir={} out={} level={}\n",
                            b.iocon.reg(p),
                            (g.dir >> p.1) & 1,
                            (g.data >> p.1) & 1,
                            b.pin_level(p) as u8
                        ));
                    }
                }
                write!(self.out, "{s}")?;
            }
            "info" => {
                let m = &self.m;
                writeln!(
                    self.out,
                    "t={:.3}s pc={:#06x} clock={:.3}MHz sleeping={} halted={:?} instr={} rtc={} vbat={}mV",
                    m.now(),
                    m.cpu.r[15],
                    m.bus.hz() / 1e6,
                    m.cpu.sleeping,
                    m.halted(),
                    m.cpu.instret,
                    m.bus.rtc.time(),
                    m.bus.vbat_mv
                )?;
            }
            o => bail!("unknown command {o:?}"),
        }
        Ok(())
    }

    /// Runs a whole script. Returns the number of failed expectations.
    pub fn run_script(&mut self, text: &str) -> Result<usize> {
        for (i, line) in text.lines().enumerate() {
            self.exec(line).with_context(|| format!("line {}: {}", i + 1, line.trim()))?;
        }
        self.out.flush()?;
        Ok(self.failures.len())
    }
}
