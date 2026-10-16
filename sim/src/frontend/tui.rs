//! Interactive terminal front end: LCD, keyboard legend, serial log, command line.

use super::glass;
use super::pty::Pty;
use super::script::Runner;
use crate::board::keyboard::Key;
use crate::board::lcd::{HEIGHT, WIDTH};
use crate::machine::Machine;
use crate::models::{Model, Shift};
use anyhow::Result;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct TuiOptions {
    /// Emulated seconds per wall-clock second.
    pub speed: f64,
    pub pty_link: Option<PathBuf>,
    pub state_path: Option<PathBuf>,
    pub mute: bool,
    pub script_dir: PathBuf,
}

const GRID_KEYS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl;", "zxcvbnm,./"];

const LCD_BG: Color = Color::Rgb(168, 180, 140);
const LCD_FG: Color = Color::Rgb(28, 34, 28);
const YELLOW: Color = Color::Rgb(240, 200, 40);
const BLUE: Color = Color::Rgb(140, 190, 230);

struct App<'a> {
    m: &'a mut Machine,
    model: Model,
    opts: TuiOptions,
    pty: Option<Pty>,
    grid_mode: bool,
    latch: bool,
    paused: bool,
    help: bool,
    enhanced: bool,
    /// Keys currently held, with the emulated time at which to release them (None = until released).
    held: Vec<(Key, Option<f64>)>,
    serial: String,
    messages: VecDeque<String>,
    cmdline: Option<String>,
    beeps_seen: usize,
    quit: bool,
    wall_speed: f64,
}

impl<'a> App<'a> {
    fn msg(&mut self, s: impl Into<String>) {
        self.messages.push_back(s.into());
        while self.messages.len() > 200 {
            self.messages.pop_front();
        }
    }

    fn mnemonic(&self, code: KeyCode) -> Option<&'static str> {
        let name = match code {
            KeyCode::Enter => "ENTER",
            KeyCode::Backspace => "←",
            KeyCode::Esc => "ON",
            KeyCode::Char(c) => match c {
                '0' => "0",
                '1' => "1",
                '2' => "2",
                '3' => "3",
                '4' => "4",
                '5' => "5",
                '6' => "6",
                '7' => "7",
                '8' => "8",
                '9' => "9",
                'A' => "A",
                'B' => "B",
                'C' => "C",
                'D' => "D",
                'E' => "E",
                'F' => "F",
                '+' => "+",
                '-' => "−",
                '*' => "×",
                '/' => "÷",
                '.' => ".",
                'f' => "f",
                'g' => "g",
                's' => "STO",
                'r' => "RCL",
                'h' => "HEX",
                'd' => "DEC",
                'o' => "OCT",
                'b' => "BIN",
                'x' => "x≷y",
                'c' => "CHS",
                'v' => "R↓",
                'n' => "R/S",
                't' => "GTO",
                'u' => "GSB",
                'i' => "SST",
                'e' => "EEX",
                _ => return None,
            },
            _ => return None,
        };
        Some(name)
    }

    fn map_key(&self, code: KeyCode) -> Option<Key> {
        if self.grid_mode {
            if let KeyCode::Char(c) = code {
                for (r, row) in GRID_KEYS.iter().enumerate() {
                    if let Some(ci) = row.find(c) {
                        return Key::from_grid(r as u8 + 1, ((ci + 1) % 10) as u8);
                    }
                }
            }
            return match code {
                KeyCode::Enter => Key::from_grid(3, 6),
                KeyCode::Esc => Some(Key::On),
                _ => None,
            };
        }
        let name = self.mnemonic(code)?;
        match self.model.resolve(name) {
            Some((Shift::None, k)) => Some(k),
            _ => None,
        }
    }

    /// PC key bound to a calculator key in the current mapping (for the legend panel).
    fn binding(&self, key: Key) -> String {
        if self.grid_mode {
            let (r, c) = key.grid();
            if key == Key::On {
                return "z/Esc".into();
            }
            let ci = if c == 0 { 9 } else { c as usize - 1 };
            return GRID_KEYS[(r - 1) as usize].chars().nth(ci).map(|c| c.to_string()).unwrap_or_default();
        }
        let candidates: Vec<KeyCode> = "0123456789ABCDEF+-*/.fgsrhdobxcvntuie"
            .chars()
            .map(KeyCode::Char)
            .chain([KeyCode::Enter, KeyCode::Backspace, KeyCode::Esc])
            .collect();
        for code in candidates {
            if self.map_key(code) == Some(key) {
                return match code {
                    KeyCode::Char(c) => c.to_string(),
                    KeyCode::Enter => "⏎".into(),
                    KeyCode::Backspace => "⌫".into(),
                    KeyCode::Esc => "Esc".into(),
                    _ => String::new(),
                };
            }
        }
        String::new()
    }

    fn press(&mut self, key: Key) {
        let release = if self.latch || self.enhanced { None } else { Some(self.m.now() + 0.12) };
        if let Some(h) = self.held.iter_mut().find(|(k, _)| *k == key) {
            if h.1.is_some() {
                h.1 = release;
            }
            return;
        }
        self.m.key(key, true);
        self.held.push((key, release));
        if std::env::var_os("VOYAGER_DEBUG").is_some() {
            let msg = format!("key {} down at {:.3}s, release {:?}", key.grid_name(), self.m.now(), release);
            self.msg(msg);
        }
    }

    fn release(&mut self, key: Key) {
        if self.latch {
            return;
        }
        self.held.retain(|(k, _)| *k != key);
        self.m.key(key, false);
    }

    fn release_all(&mut self) {
        self.held.clear();
        self.m.release_all();
    }

    fn handle_key(&mut self, ev: KeyEvent) {
        if let Some(cmd) = &mut self.cmdline {
            if ev.kind == KeyEventKind::Release {
                return;
            }
            match ev.code {
                KeyCode::Esc => self.cmdline = None,
                KeyCode::Enter => {
                    let line = std::mem::take(cmd);
                    self.cmdline = None;
                    self.command(&line);
                }
                KeyCode::Backspace => {
                    cmd.pop();
                }
                KeyCode::Char(c) => cmd.push(c),
                _ => {}
            }
            return;
        }
        if ev.kind == KeyEventKind::Release {
            if let Some(k) = self.map_key(ev.code) {
                self.release(k);
            }
            return;
        }
        if ev.kind == KeyEventKind::Repeat {
            return;
        }
        if ev.modifiers.contains(KeyModifiers::CONTROL) && matches!(ev.code, KeyCode::Char('c') | KeyCode::Char('q')) {
            self.quit = true;
            return;
        }
        match ev.code {
            KeyCode::F(1) => self.help = !self.help,
            KeyCode::F(2) => {
                if self.held.iter().any(|(k, _)| *k == Key::On) {
                    self.held.retain(|(k, _)| *k != Key::On);
                    self.m.key(Key::On, false);
                    self.msg("ON released");
                } else {
                    self.m.key(Key::On, true);
                    self.held.push((Key::On, None));
                    self.msg("ON held (F2 releases)");
                }
            }
            KeyCode::F(3) => {
                self.grid_mode = !self.grid_mode;
                self.msg(if self.grid_mode { "key mapping: grid" } else { "key mapping: mnemonic" });
            }
            KeyCode::F(4) => {
                self.latch = !self.latch;
                if self.latch {
                    self.msg("latch on: keys stay pressed until F4");
                } else {
                    self.release_all();
                    self.msg("latch off: all keys released");
                }
            }
            KeyCode::F(5) => {
                self.m.reset();
                self.msg("hardware reset");
            }
            KeyCode::F(6) => self.paused = !self.paused,
            KeyCode::F(7) => match self.screenshot() {
                Ok(p) => self.msg(format!("screenshot: {}", p.display())),
                Err(e) => self.msg(format!("screenshot failed: {e:#}")),
            },
            KeyCode::F(10) => self.quit = true,
            KeyCode::Char(':') => self.cmdline = Some(String::new()),
            code => {
                if let Some(k) = self.map_key(code) {
                    self.press(k);
                }
            }
        }
    }

    /// Save the current LCD as a photo-like SVG in screenshots/.
    fn screenshot(&self) -> Result<PathBuf> {
        let dir = PathBuf::from("screenshots");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("screen-{}.svg", chrono::Local::now().format("%Y%m%d-%H%M%S%.3f")));
        std::fs::write(&path, glass::svg(&self.m.pixels(), 12.0))?;
        Ok(path)
    }

    fn command(&mut self, line: &str) {
        let line = line.trim();
        match line {
            "q" | "quit" => {
                self.quit = true;
                return;
            }
            "" => return,
            _ => {}
        }
        self.msg(format!(":{line}"));
        let mut buf = Vec::new();
        let (res, serial) = {
            let mut r = Runner::new(self.m, self.model, Box::new(&mut buf), &self.opts.script_dir);
            let res = r.exec(line);
            (res, std::mem::take(&mut r.serial))
        };
        self.serial_in(serial.as_bytes());
        for l in String::from_utf8_lossy(&buf).lines() {
            self.msg(l.to_string());
        }
        if let Err(e) = res {
            self.msg(format!("error: {e:#}"));
        }
    }

    fn serial_in(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Some(p) = &mut self.pty {
            p.write(bytes);
        }
        self.serial.push_str(&String::from_utf8_lossy(bytes).replace("\r\n", "\n").replace('\r', "\n"));
        if self.serial.len() > 20000 {
            let cut = self.serial.len() - 16000;
            let cut = (cut..self.serial.len()).find(|&i| self.serial.is_char_boundary(i)).unwrap_or(0);
            self.serial.drain(..cut);
        }
    }

    /// Runs the emulation for `dt` emulated seconds, releasing timed key presses on time.
    fn advance(&mut self, dt: f64) {
        let end = self.m.now() + dt;
        while self.m.now() < end && self.m.halted().is_none() {
            let next_release = self.held.iter().filter_map(|(_, t)| *t).fold(f64::INFINITY, f64::min);
            let t = end.min(next_release);
            self.m.run_until(t);
            let now = self.m.now();
            let due: Vec<Key> = self.held.iter().filter(|(_, t)| t.is_some_and(|t| t <= now)).map(|(k, _)| *k).collect();
            for k in due {
                self.held.retain(|(h, _)| *h != k);
                self.m.key(k, false);
            }
            if t == end {
                break;
            }
        }
        let tx = self.m.serial_take();
        self.serial_in(&tx);
        for l in self.m.take_log() {
            self.msg(l);
        }
        let beeps: Vec<_> = self.m.bus.beeper.beeps[self.beeps_seen..].to_vec();
        self.beeps_seen = self.m.bus.beeper.beeps.len();
        for (_, f, d) in beeps {
            self.msg(format!("beep {f:.0} Hz {:.0} ms", d * 1000.0));
            if !self.opts.mute {
                print!("\x07");
            }
        }
    }

    fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let wide = area.width as usize >= WIDTH + 2;
        let lcd_h = if wide { HEIGHT / 2 } else { HEIGHT / 4 } as u16 + 2;
        let [header, lcd, keys, log, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(lcd_h),
            Constraint::Length(18),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(area);

        let state = match self.m.halted() {
            Some(h) => format!("HALTED: {h:?}"),
            None if self.paused => "paused".into(),
            None if self.m.cpu.sleeping => {
                if self.m.bus.deep_sleep { "deep sleep".into() } else { "sleep".into() }
            }
            None => "running".into(),
        };
        let head = format!(
            " {} │ t={:.1}s │ {:.3} MHz │ {} │ ×{:.1} │ {}",
            self.model.name(),
            self.m.now(),
            self.m.bus.hz() / 1e6,
            state,
            self.wall_speed,
            self.pty.as_ref().map_or("no pty".to_string(), |p| format!("serial: {}", p.path.display()))
        );
        f.render_widget(Paragraph::new(head).style(Style::new().add_modifier(Modifier::REVERSED)), header);

        let title = if self.m.bus.lcd.display_on { " LCD ".to_string() } else { " LCD (off) ".to_string() };
        let block = Block::bordered().title(title);
        let inner = block.inner(lcd);
        f.render_widget(block, lcd);
        let px = self.m.pixels();
        let lines = if wide { half_block_lines(&px) } else { braille_lines(&px) };
        f.render_widget(Paragraph::new(lines), inner);

        self.draw_keys(f, keys);

        let mut text: Vec<Line> = Vec::new();
        let serial_lines: Vec<&str> = self.serial.lines().collect();
        let msg_lines: Vec<&String> = self.messages.iter().collect();
        let [sl, ml] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(log);
        let h = sl.height.saturating_sub(2) as usize;
        for l in serial_lines.iter().skip(serial_lines.len().saturating_sub(h)) {
            text.push(Line::raw(l.to_string()));
        }
        f.render_widget(Paragraph::new(text).block(Block::bordered().title(" serial ")), sl);
        let h = ml.height.saturating_sub(2) as usize;
        let mtext: Vec<Line> =
            msg_lines.iter().skip(msg_lines.len().saturating_sub(h)).map(|l| Line::raw(l.to_string())).collect();
        f.render_widget(Paragraph::new(mtext).block(Block::bordered().title(" messages ")), ml);

        let st = match &self.cmdline {
            Some(c) => format!(":{c}█"),
            None => format!(
                " F1 help  F2 hold ON{}  F3 map:{}  F4 latch{}  F5 reset  F6 pause  F7 screenshot  : command  F10 quit",
                if self.held.iter().any(|(k, _)| *k == Key::On) { "*" } else { "" },
                if self.grid_mode { "grid" } else { "mnemonic" },
                if self.latch { "*" } else { "" }
            ),
        };
        f.render_widget(Paragraph::new(st), status);

        if self.help {
            let w = 76.min(area.width);
            let h = 22.min(area.height);
            let r = Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h);
            f.render_widget(Clear, r);
            f.render_widget(Paragraph::new(HELP).wrap(Wrap { trim: false }).block(Block::bordered().title(" help ")), r);
        }
    }

    fn draw_keys(&self, f: &mut Frame, area: Rect) {
        let block = Block::bordered().title(format!(" {} keyboard ", self.model.name()));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let cw = (inner.width / 10).clamp(6, 12) as usize;
        let mut lines = Vec::new();
        for r in 1..=4u8 {
            let mut fl = Vec::new();
            let mut bl = Vec::new();
            let mut gl = Vec::new();
            let mut kl = Vec::new();
            for c in 1..=10u8 {
                let key = if r == 4 && c == 6 { Key::from_grid(3, 6) } else { Key::from_grid(r, c % 10) };
                let Some(key) = key else { continue };
                let lg = self.model.legend(key);
                let pressed = self.held.iter().any(|(k, _)| *k == key);
                let base = if r == 4 && c == 6 { "" } else { lg.base };
                let base_style = if pressed {
                    Style::new().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)
                } else if base == "f" {
                    Style::new().fg(Color::Black).bg(YELLOW)
                } else if base == "g" {
                    Style::new().fg(Color::Black).bg(BLUE)
                } else {
                    Style::new().fg(Color::White).bg(Color::Rgb(70, 55, 45)).add_modifier(Modifier::BOLD)
                };
                let (fs, gs) = if r == 4 && c == 6 { ("", "") } else { (lg.f, lg.g) };
                let fs = fs.strip_prefix("SHOW ").or_else(|| fs.strip_prefix("CLEAR ")).unwrap_or(fs);
                fl.push(Span::styled(center(fs, cw), Style::new().fg(YELLOW)));
                bl.push(Span::styled(center(base, cw - 1), base_style));
                bl.push(Span::raw(" "));
                gl.push(Span::styled(center(gs, cw), Style::new().fg(BLUE)));
                let hint = if r == 4 && c == 6 { String::new() } else { self.binding(key) };
                kl.push(Span::styled(center(&hint, cw), Style::new().fg(Color::DarkGray)));
            }
            lines.push(Line::from(fl));
            lines.push(Line::from(bl));
            lines.push(Line::from(gl));
            lines.push(Line::from(kl));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}

fn center(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        return s.chars().take(w).collect();
    }
    let left = (w - n) / 2;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(w - n - left))
}

fn half_block_lines(px: &[[bool; WIDTH]; HEIGHT]) -> Vec<Line<'static>> {
    (0..HEIGHT / 2)
        .map(|r| {
            let spans: Vec<Span> = (0..WIDTH)
                .map(|x| {
                    let (t, b) = (px[2 * r][x], px[2 * r + 1][x]);
                    let (ch, fg, bg) = match (t, b) {
                        (true, true) => ('█', LCD_FG, LCD_BG),
                        (true, false) => ('▀', LCD_FG, LCD_BG),
                        (false, true) => ('▄', LCD_FG, LCD_BG),
                        (false, false) => (' ', LCD_FG, LCD_BG),
                    };
                    Span::styled(ch.to_string(), Style::new().fg(fg).bg(bg))
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn braille_lines(px: &[[bool; WIDTH]; HEIGHT]) -> Vec<Line<'static>> {
    const BITS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
    (0..HEIGHT / 4)
        .map(|r| {
            let s: String = (0..WIDTH / 2)
                .map(|cx| {
                    let mut v = 0;
                    for (dy, row) in BITS.iter().enumerate() {
                        for (dx, bit) in row.iter().enumerate() {
                            if px[4 * r + dy][2 * cx + dx] {
                                v |= bit;
                            }
                        }
                    }
                    char::from_u32(0x2800 + v).unwrap_or(' ')
                })
                .collect();
            Line::styled(s, Style::new().fg(LCD_FG).bg(LCD_BG))
        })
        .collect()
}

const HELP: &str = "\
Keys (mnemonic map): digits 0-9, A-F (uppercase), Enter, Backspace (←),
  + - * /, '.', f, g, Esc = ON, s STO, r RCL, h HEX, d DEC, o OCT, b BIN,
  x x≷y, c CHS, v R↓, n R/S, t GTO, u GSB, i SST, e EEX.
F3 switches to the grid map: PC rows 1..0 / q..p / a..; / z../ are the
  calculator rows (z = ON). The dim hint under each key shows its binding.
Terminals without key-release events get 120 ms taps. For ON+key combos
  use F4 (latch: keys stay down) or F2 (hold ON), or ':hold 1-3' etc.
Setup items (calculator off): hold the item key, press ON. Examples:
  A (1-1) date/time, C (1-3) serial console, D (1-4) bootloader.
F7 saves the LCD as a photo-like SVG in screenshots/.
Commands (':'): keys 2 ENTER 3 +  |  press ON 300ms  |  hold X / release all
  wait 2s | serial \"?\\r\" | vbat 2000 | rtc 2026-01-01T12:00:00 | reset
  state save FILE | state load FILE | info | pins | beeps | quit
The serial console is also on the pseudo-terminal shown in the header,
  e.g. `screen /dev/ttysNNN 38400`.";

pub fn run(m: &mut Machine, model: Model, opts: TuiOptions) -> Result<()> {
    let pty = match Pty::open(opts.pty_link.as_deref()) {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("warning: no pty: {e}");
            None
        }
    };
    let speed = opts.speed;
    let mut app = App {
        m,
        model,
        opts,
        pty,
        grid_mode: false,
        latch: false,
        paused: false,
        help: false,
        enhanced: false,
        held: Vec::new(),
        serial: String::new(),
        messages: VecDeque::new(),
        cmdline: None,
        beeps_seen: 0,
        quit: false,
        wall_speed: 0.0,
    };
    app.beeps_seen = app.m.bus.beeper.beeps.len();
    if let Some(p) = &app.pty {
        app.msg(format!("serial console on {}", p.path.display()));
    }
    app.msg("F1 for help");

    let mut terminal = ratatui::init();
    app.enhanced = matches!(terminal::supports_keyboard_enhancement(), Ok(true));
    if app.enhanced {
        let _ = execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES | KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
        app.msg("terminal reports key releases: keys are held while pressed");
    }
    let frame = Duration::from_millis(20);
    let mut last = Instant::now();
    let mut speed_acc = (0.0f64, 0.0f64, Instant::now());
    let result = (|| -> Result<()> {
        while !app.quit {
            terminal.draw(|f| app.draw(f))?;
            let deadline = Instant::now() + frame;
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                if !event::poll(left)? {
                    break;
                }
                if let Event::Key(k) = event::read()? {
                    app.handle_key(k);
                }
            }
            if let Some(p) = &mut app.pty {
                let input = p.read();
                if !input.is_empty() {
                    app.m.serial_send(&input);
                }
            }
            let now = Instant::now();
            let dt = (now - last).as_secs_f64().min(0.25);
            last = now;
            if !app.paused && app.m.halted().is_none() {
                let t0 = app.m.now();
                if speed.is_infinite() {
                    // As fast as possible, but keep the UI responsive.
                    let start = Instant::now();
                    while start.elapsed() < Duration::from_millis(15) && app.m.halted().is_none() {
                        app.advance(0.005);
                    }
                } else {
                    app.advance(dt * speed);
                }
                speed_acc.0 += app.m.now() - t0;
            }
            speed_acc.1 += dt;
            if speed_acc.2.elapsed() > Duration::from_secs(1) {
                app.wall_speed = speed_acc.0 / speed_acc.1.max(1e-9);
                speed_acc = (0.0, 0.0, Instant::now());
            }
        }
        Ok(())
    })();
    if app.enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    if let Some(path) = &app.opts.state_path {
        app.m.save_state(path)?;
        eprintln!("state saved to {}", path.display());
    }
    result
}

/// Parses a `--speed` value: a factor ("1", "4.5") or "max".
pub fn parse_speed(s: &str) -> Result<f64> {
    if s == "max" {
        return Ok(f64::INFINITY);
    }
    Ok(s.parse::<f64>()?)
}
