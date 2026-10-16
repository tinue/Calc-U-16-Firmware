//! Memory map, pin resolution, event scheduling and timekeeping.

use crate::board::{self, keyboard::Keyboard, lcd::Lcd, rtc::Rtc, Beeper};
use crate::cpu::exceptions::Scs;
use crate::cpu::Halt;
use crate::periph::{
    adc::Adc,
    gpio::GpioPort,
    i2c::I2c,
    iap,
    iocon::{Iocon, IOCON_MAP, MATCH_PINS},
    irq,
    syscon::Syscon,
    timer::Timer,
    uart::Uart,
    Pin,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FLASH_SIZE: usize = 0x1_0000;
pub const RAM_BASE: u32 = 0x1000_0000;
pub const RAM_SIZE: usize = 0x2000;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Trace {
    pub periph: bool,
    pub irq: bool,
    pub i2c: bool,
    pub lcd: bool,
    pub iap: bool,
    pub pins: bool,
}

impl Trace {
    pub fn parse(s: &str) -> Trace {
        let mut t = Trace::default();
        for f in s.split(',') {
            match f.trim() {
                "periph" => t.periph = true,
                "irq" => t.irq = true,
                "i2c" => t.i2c = true,
                "lcd" => t.lcd = true,
                "iap" => t.iap = true,
                "pins" => t.pins = true,
                "all" => t = Trace { periph: true, irq: true, i2c: true, lcd: true, iap: true, pins: true },
                _ => {}
            }
        }
        t
    }
}

fn pin_index(p: Pin) -> u32 {
    p.0 as u32 * 12 + p.1 as u32
}

/// Start-logic inputs: bit n = PIO0_n for n < 12, bit 12 = PIO1_0.
fn start_pin(bit: u32) -> Pin {
    if bit < 12 {
        Pin(0, bit as u8)
    } else {
        Pin(1, 0)
    }
}

const WATCHED: &[Pin] = &[
    Pin(0, 0),
    Pin(0, 1),
    Pin(0, 2),
    Pin(0, 3),
    Pin(0, 4),
    Pin(0, 5),
    Pin(0, 6),
    Pin(0, 7),
    Pin(0, 8),
    Pin(0, 9),
    Pin(0, 10),
    Pin(0, 11),
    Pin(1, 0),
    board::LCD_SCK,
    board::LCD_RST,
    board::LCD_CS,
    board::BEEPER_A,
];

#[derive(Serialize, Deserialize)]
pub struct Bus {
    #[serde(skip)]
    pub flash: Vec<u8>,
    pub ram: Vec<u8>,
    pub cycles: u64,
    hz: f64,
    time_base: f64,
    cycles_base: u64,
    /// Cycle at which `service` must run next (0 forces a recomputation).
    #[serde(skip)]
    pub next_event: u64,
    pub scs: Scs,
    pub syscon: Syscon,
    pub iocon: Iocon,
    pub gpio: [GpioPort; 4],
    pub timers: [Timer; 4],
    pub uart: Uart,
    pub i2c: I2c,
    pub adc: Adc,
    pub keyboard: Keyboard,
    pub lcd: Lcd,
    pub rtc: Rtc,
    pub beeper: Beeper,
    pub vbat_mv: f64,
    /// Last resolved level of the watched pins (bit = pin_index).
    pins: u64,
    pub deep_sleep: bool,
    /// Registers of peripherals that are not modelled.
    pub unknown: BTreeMap<u32, u32>,
    pub unknown_seen: BTreeSet<u32>,
    #[serde(skip)]
    fault: Option<u32>,
    #[serde(skip)]
    pub trace: Trace,
    #[serde(skip)]
    pub log: Vec<String>,
    /// Statistics: number of times the CPU entered sleep / deep sleep.
    pub sleeps: u64,
    pub deep_sleeps: u64,
}

impl Bus {
    pub fn new(flash: Vec<u8>, rtc: Rtc) -> Self {
        let mut b = Bus {
            flash,
            ram: vec![0; RAM_SIZE],
            cycles: 0,
            hz: 12e6,
            time_base: 0.0,
            cycles_base: 0,
            next_event: 0,
            scs: Scs::default(),
            syscon: Syscon::default(),
            iocon: Iocon::default(),
            gpio: Default::default(),
            timers: [Timer::new(false), Timer::new(false), Timer::new(true), Timer::new(true)],
            uart: Uart::default(),
            i2c: I2c::default(),
            adc: Adc::default(),
            keyboard: Keyboard::default(),
            lcd: Lcd::default(),
            rtc,
            beeper: Beeper::default(),
            vbat_mv: 3000.0,
            pins: 0,
            deep_sleep: false,
            unknown: BTreeMap::new(),
            unknown_seen: BTreeSet::new(),
            fault: None,
            trace: Trace::default(),
            log: Vec::new(),
            sleeps: 0,
            deep_sleeps: 0,
        };
        b.pins = b.resolve_watched();
        b
    }

    /// MCU reset: peripherals return to reset state; RAM, RTC, LCD and keyboard are external.
    pub fn reset_mcu(&mut self, sysreset: bool) {
        self.rebase_clock();
        self.deep_sleep = false;
        self.scs = Scs::default();
        self.syscon = Syscon::default();
        if sysreset {
            self.syscon.set(0x030, 0x10);
        }
        self.iocon = Iocon::default();
        self.gpio = Default::default();
        self.timers = [Timer::new(false), Timer::new(false), Timer::new(true), Timer::new(true)];
        let host_rx = std::mem::take(&mut self.uart.host_rx);
        self.uart = Uart { host_rx, ..Uart::default() };
        self.i2c.reset();
        self.adc = Adc::default();
        self.rebase_clock();
        self.pins = self.resolve_watched();
        self.next_event = 0;
    }

    // ---- time ----

    pub fn hz(&self) -> f64 {
        self.hz
    }

    pub fn now_s(&self) -> f64 {
        self.time_base + (self.cycles - self.cycles_base) as f64 / self.hz
    }

    /// Cycle count at emulated time `t` (at the current clock).
    pub fn cycles_at(&self, t: f64) -> u64 {
        let dt = (t - self.now_s()).max(0.0);
        self.cycles + (dt * self.hz).ceil() as u64
    }

    fn rebase_clock(&mut self) {
        self.time_base = self.now_s();
        self.cycles_base = self.cycles;
        let hz = self.syscon.core_hz();
        if (hz - self.hz).abs() > 0.5 && self.trace.periph {
            let msg = format!("clock: {:.3} MHz -> {:.3} MHz", self.hz / 1e6, hz / 1e6);
            self.trace_line(msg);
        }
        self.hz = hz;
        self.next_event = 0;
    }

    pub fn trace_line(&mut self, s: String) {
        let t = self.now_s();
        self.log.push(format!("[{t:12.6}] {s}"));
    }

    pub fn take_fault(&mut self) -> Option<u32> {
        self.fault.take()
    }

    pub fn on_sleep(&mut self) {
        self.sleeps += 1;
        if self.scs.scr & 4 != 0 {
            self.deep_sleeps += 1;
            self.deep_sleep = true;
            self.scs.systick_freeze(self.cycles);
            self.next_event = 0;
        }
    }

    pub fn on_wake(&mut self) {
        self.deep_sleep = false;
        self.scs.systick_thaw(self.cycles);
        self.next_event = 0;
    }

    // ---- events ----

    /// Runs all due peripheral events and computes the next event cycle.
    pub fn service(&mut self) {
        let c = self.cycles;
        let now = self.now_s();
        self.scs.systick_service(c);
        for i in 0..4 {
            if self.timers[i].next_event().is_some_and(|t| t <= c) {
                let em_changed = self.timers[i].service(c);
                self.update_timer_line(i);
                if em_changed {
                    self.update_pins();
                }
            }
        }
        if self.i2c.pending_at.is_some_and(|t| t <= c) {
            self.rtc.now_s = now;
            self.i2c.service(c, &mut self.rtc);
            self.scs.set_line(irq::I2C, self.i2c.irq_line());
            self.flush_i2c_log();
            self.update_pins();
        }
        if self.uart.next_rx.is_some_and(|t| t <= c) {
            if self.deep_sleep {
                self.uart.host_rx.pop_front();
                self.uart.dropped += 1;
            } else {
                self.uart.deliver();
            }
            self.uart.next_rx = if self.uart.host_rx.is_empty() { None } else { Some(c + self.uart_byte_cycles()) };
            self.scs.set_line(irq::UART, self.uart.irq_line());
        }
        self.rtc.now_s = now;
        if self.rtc.next_event().is_some_and(|t| t <= now) {
            self.rtc.service();
            self.update_pins();
        }
        self.beeper.flush(now);

        let mut next = u64::MAX;
        let mut consider = |v: Option<u64>| {
            if let Some(v) = v {
                next = next.min(v);
            }
        };
        consider(self.scs.systick_next());
        for t in &self.timers {
            consider(t.next_event());
        }
        consider(self.i2c.pending_at);
        consider(self.uart.next_rx);
        consider(self.rtc.next_event().map(|t| self.cycles_at(t).max(c + 1)));
        if let Some((_, last, _)) = self.beeper.burst {
            consider(Some(self.cycles_at(last + 0.021)));
        }
        self.next_event = next.max(c + 1);
    }

    fn uart_byte_cycles(&self) -> u64 {
        let clkdiv = (self.syscon.get(0x098) & 0xFF).max(1) as f64;
        let pclk = self.hz * (self.syscon.get(0x078) & 0xFF).max(1) as f64 / clkdiv;
        let baud = self.uart.baud(pclk);
        ((10.0 / baud) * self.hz).max(16.0) as u64
    }

    /// Queues bytes from the host for the UART receiver.
    pub fn uart_send(&mut self, bytes: &[u8]) {
        self.uart.host_rx.extend(bytes.iter().copied());
        if self.uart.next_rx.is_none() && !self.uart.host_rx.is_empty() {
            self.uart.next_rx = Some(self.cycles + self.uart_byte_cycles());
            self.next_event = 0;
        }
    }

    fn update_timer_line(&mut self, i: usize) {
        let line = self.timers[i].irq_line();
        self.scs.set_line(irq::CT16B0 + i as u32, line);
    }

    fn update_start_lines(&mut self) {
        let lines = self.syscon.start_lines();
        for bit in 0..13 {
            self.scs.set_line(bit, lines & (1 << bit) != 0);
        }
    }

    fn flush_i2c_log(&mut self) {
        if self.i2c.log.is_empty() {
            return;
        }
        if self.trace.i2c && self.i2c.log.last().is_some_and(|s| s == "P") {
            let s = self.i2c.log.join(" ");
            self.i2c.log.clear();
            self.trace_line(format!("i2c: {s}"));
        } else if !self.trace.i2c {
            self.i2c.log.clear();
        }
    }

    // ---- pins ----

    fn driven_low(&self, p: Pin) -> bool {
        let port = &self.gpio[p.0 as usize];
        let bit = 1 << p.1;
        port.dir & bit != 0 && port.data & bit == 0 && self.iocon.is_gpio(p)
    }

    /// Level driven onto a pin by the board, if any.
    fn external(&self, p: Pin) -> Option<bool> {
        if let Some(r) = board::ROWS.iter().position(|&x| x == p) {
            let low = board::COLUMNS
                .iter()
                .enumerate()
                .any(|(c, &col)| self.keyboard.pressed[c] & (1 << r) != 0 && self.driven_low(col));
            return low.then_some(false);
        }
        if p == board::ON_LINE {
            let low = (self.keyboard.on && self.driven_low(board::ON_DRIVE)) || self.rtc.int_active();
            return low.then_some(false);
        }
        None
    }

    pub fn pin_level(&self, p: Pin) -> bool {
        let func = self.iocon.func(p);
        for &(mp, f, t, ch) in MATCH_PINS {
            if mp == p && func == f {
                return self.timers[t].em(ch);
            }
        }
        let port = &self.gpio[p.0 as usize];
        let bit = 1 << p.1;
        if port.dir & bit != 0 && self.iocon.is_gpio(p) {
            return port.data & bit != 0;
        }
        self.external(p).unwrap_or_else(|| self.iocon.pull_level(p))
    }

    fn resolve_watched(&self) -> u64 {
        WATCHED.iter().fold(0u64, |a, &p| a | (self.pin_level(p) as u64) << pin_index(p))
    }

    /// Re-resolves the watched pins and reacts to edges (start logic, LCD, beeper).
    pub fn update_pins(&mut self) {
        let new = self.resolve_watched();
        let changed = new ^ self.pins;
        if changed == 0 {
            return;
        }
        self.pins = new;
        let level = |p: Pin| new & (1 << pin_index(p)) != 0;
        let edge = |p: Pin| changed & (1 << pin_index(p)) != 0;
        let mut start_changed = false;
        for bit in 0..13 {
            let p = start_pin(bit);
            if edge(p) {
                if self.trace.pins {
                    let msg = format!("pin {p} -> {}", level(p) as u8);
                    self.trace_line(msg);
                }
                if self.syscon.start_edge(bit, level(p)) {
                    start_changed = true;
                    if self.trace.irq {
                        self.trace_line(format!("start logic {bit} latched ({p} -> {})", level(p) as u8));
                    }
                }
            }
        }
        if start_changed {
            self.update_start_lines();
        }
        if edge(board::LCD_RST) && !level(board::LCD_RST) {
            self.lcd.reset();
        }
        if edge(board::LCD_CS) && level(board::LCD_CS) {
            self.lcd.deselect();
        }
        if edge(board::LCD_SCK) && level(board::LCD_SCK) && !level(board::LCD_CS) {
            let mosi = self.pin_level(board::LCD_MOSI);
            let a0 = self.pin_level(board::LCD_A0);
            self.lcd.clock(mosi, a0);
            if self.trace.lcd && !self.lcd.log.is_empty() {
                for l in std::mem::take(&mut self.lcd.log) {
                    self.trace_line(format!("lcd: {l}"));
                }
            }
        }
        if edge(board::BEEPER_A) {
            let now = self.now_s();
            self.beeper.toggle(now);
            self.next_event = 0;
        }
    }

    fn gpio_read_data(&self, port: usize, mask: u32) -> u32 {
        let mut v = 0;
        for b in 0..12 {
            if mask & (1 << b) != 0 && self.pin_level(Pin(port as u8, b as u8)) {
                v |= 1 << b;
            }
        }
        v
    }

    // ---- memory access ----

    #[inline]
    pub fn fetch16(&mut self, addr: u32) -> u16 {
        let a = addr as usize;
        if a + 1 < FLASH_SIZE {
            u16::from_le_bytes([self.flash[a], self.flash[a + 1]])
        } else if (RAM_BASE..RAM_BASE + RAM_SIZE as u32 - 1).contains(&addr) {
            let o = (addr - RAM_BASE) as usize;
            u16::from_le_bytes([self.ram[o], self.ram[o + 1]])
        } else {
            self.fault = Some(addr);
            0xDE00
        }
    }

    pub fn read32(&mut self, addr: u32) -> u32 {
        if addr & 3 != 0 {
            self.fault = Some(addr);
            return 0;
        }
        match addr >> 28 {
            0 if (addr as usize) < FLASH_SIZE => {
                let a = addr as usize;
                u32::from_le_bytes(self.flash[a..a + 4].try_into().unwrap())
            }
            1 if (RAM_BASE..RAM_BASE + RAM_SIZE as u32).contains(&addr) => {
                let o = (addr - RAM_BASE) as usize;
                u32::from_le_bytes(self.ram[o..o + 4].try_into().unwrap())
            }
            1 if addr >= 0x1FFF_0000 => 0,
            4 => self.periph_read(addr),
            5 => self.gpio_read(addr),
            0xE if (0xE000_E000..0xE000_F000).contains(&addr) => {
                let c = self.cycles;
                self.scs.read(addr - 0xE000_E000, c)
            }
            _ => {
                self.fault = Some(addr);
                0
            }
        }
    }

    pub fn read16(&mut self, addr: u32) -> u16 {
        if addr >> 28 <= 1 && addr & 1 == 0 {
            if (addr as usize) < FLASH_SIZE {
                let a = addr as usize;
                return u16::from_le_bytes([self.flash[a], self.flash[a + 1]]);
            }
            if (RAM_BASE..RAM_BASE + RAM_SIZE as u32).contains(&addr) {
                let o = (addr - RAM_BASE) as usize;
                return u16::from_le_bytes([self.ram[o], self.ram[o + 1]]);
            }
        }
        (self.read32(addr & !3) >> ((addr & 2) * 8)) as u16
    }

    pub fn read8(&mut self, addr: u32) -> u8 {
        if (addr as usize) < FLASH_SIZE {
            return self.flash[addr as usize];
        }
        if (RAM_BASE..RAM_BASE + RAM_SIZE as u32).contains(&addr) {
            return self.ram[(addr - RAM_BASE) as usize];
        }
        (self.read32(addr & !3) >> ((addr & 3) * 8)) as u8
    }

    pub fn write32(&mut self, addr: u32, v: u32) {
        if addr & 3 != 0 {
            self.fault = Some(addr);
            return;
        }
        match addr >> 28 {
            1 if (RAM_BASE..RAM_BASE + RAM_SIZE as u32).contains(&addr) => {
                let o = (addr - RAM_BASE) as usize;
                self.ram[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }
            4 => self.periph_write(addr, v),
            5 => self.gpio_write(addr, v),
            0xE if (0xE000_E000..0xE000_F000).contains(&addr) => {
                let c = self.cycles;
                self.scs.write(addr - 0xE000_E000, v, c);
                self.next_event = 0;
                if self.trace.irq && (0x100..0x300).contains(&(addr - 0xE000_E000)) {
                    self.trace_line(format!("nvic: [{addr:#010x}] <- {v:#010x}"));
                }
            }
            _ => self.fault = Some(addr),
        }
    }

    pub fn write16(&mut self, addr: u32, v: u16) {
        if (RAM_BASE..RAM_BASE + RAM_SIZE as u32 - 1).contains(&addr) {
            let o = (addr - RAM_BASE) as usize;
            self.ram[o..o + 2].copy_from_slice(&v.to_le_bytes());
        } else {
            self.write32(addr & !3, (v as u32) << ((addr & 2) * 8));
        }
    }

    pub fn write8(&mut self, addr: u32, v: u8) {
        if (RAM_BASE..RAM_BASE + RAM_SIZE as u32).contains(&addr) {
            self.ram[(addr - RAM_BASE) as usize] = v;
        } else {
            self.write32(addr & !3, (v as u32) << ((addr & 3) * 8));
        }
    }

    fn gpio_read(&mut self, addr: u32) -> u32 {
        let port = ((addr >> 16) & 0xF) as usize;
        let off = addr & 0xFFFF;
        if port > 3 || addr & 0x0FF0_0000 != 0 {
            self.fault = Some(addr);
            return 0;
        }
        if off < 0x4000 {
            let mask = (off >> 2) & 0xFFF;
            self.gpio_read_data(port, mask)
        } else {
            self.gpio[port].read_ctrl(off)
        }
    }

    fn gpio_write(&mut self, addr: u32, v: u32) {
        let port = ((addr >> 16) & 0xF) as usize;
        let off = addr & 0xFFFF;
        if port > 3 || addr & 0x0FF0_0000 != 0 {
            self.fault = Some(addr);
            return;
        }
        if off < 0x4000 {
            let g = &mut self.gpio[port];
            // Real silicon ignores DATA writes to pins configured as inputs (measured on a
            // DM16L): a level must be written after the pin has been made an output.
            let mask = (off >> 2) & 0xFFF & g.dir;
            g.data = (g.data & !mask) | (v & mask);
        } else {
            self.gpio[port].write_ctrl(off, v);
        }
        self.update_pins();
    }

    fn unknown_access(&mut self, addr: u32, write: Option<u32>) -> u32 {
        if self.unknown_seen.insert(addr) && self.trace.periph {
            let msg = match write {
                Some(v) => format!("unmodelled write [{addr:#010x}] <- {v:#x}"),
                None => format!("unmodelled read [{addr:#010x}]"),
            };
            self.trace_line(msg);
        }
        match write {
            Some(v) => {
                self.unknown.insert(addr, v);
                v
            }
            None => *self.unknown.get(&addr).unwrap_or(&0),
        }
    }

    fn periph_read(&mut self, addr: u32) -> u32 {
        let off = addr & 0x3FFF;
        let c = self.cycles;
        match addr & !0x3FFF {
            0x4000_0000 => self.i2c.read(off),
            0x4000_8000 => {
                let v = self.uart.read(off);
                self.scs.set_line(irq::UART, self.uart.irq_line());
                v
            }
            0x4000_C000 => self.timers[0].read(off, c),
            0x4001_0000 => self.timers[1].read(off, c),
            0x4001_4000 => self.timers[2].read(off, c),
            0x4001_8000 => self.timers[3].read(off, c),
            0x4001_C000 => self.adc.read(off),
            0x4003_8000 if off <= 0x10 => self.syscon.pmu[(off / 4) as usize],
            0x4004_4000 if off < 0x100 => self.iocon.regs[(off / 4) as usize],
            0x4004_8000 if off < 0x400 => self.syscon.read(off),
            _ => self.unknown_access(addr, None),
        }
    }

    fn periph_write(&mut self, addr: u32, v: u32) {
        let off = addr & 0x3FFF;
        let c = self.cycles;
        if self.trace.periph && !(0x4000_8000..0x4000_C000).contains(&addr) {
            self.trace_line(format!("[{addr:#010x}] <- {v:#x}"));
        }
        match addr & !0x3FFF {
            0x4000_0000 => {
                self.i2c.write(off, v, c);
                self.scs.set_line(irq::I2C, self.i2c.irq_line());
                self.next_event = 0;
            }
            0x4000_8000 => {
                self.uart.write(off, v);
                self.scs.set_line(irq::UART, self.uart.irq_line());
            }
            0x4000_C000 | 0x4001_0000 | 0x4001_4000 | 0x4001_8000 => {
                let i = ((addr - 0x4000_C000) >> 14) as usize;
                self.timers[i].write(off, v, c);
                self.update_timer_line(i);
                self.next_event = 0;
                self.update_pins();
            }
            0x4001_C000 => {
                if let Some(ch) = self.adc.write(off, v) {
                    let value = self.analog(ch);
                    self.adc.complete(ch, value);
                }
            }
            0x4003_8000 if off <= 0x10 => self.syscon.pmu[(off / 4) as usize] = v,
            0x4004_4000 if off < 0x100 => {
                self.iocon.regs[(off / 4) as usize] = v;
                self.update_pins();
            }
            0x4004_8000 if off < 0x400 => {
                if off == 0x004 && v & 2 == 0 {
                    self.i2c.reset();
                    self.scs.set_line(irq::I2C, false);
                }
                if self.syscon.write(off, v) {
                    self.rebase_clock();
                }
                if (0x200..=0x20C).contains(&off) {
                    self.update_start_lines();
                }
            }
            _ => {
                self.unknown_access(addr, Some(v));
            }
        }
    }

    /// Analog input value (10 bit) for an ADC channel.
    fn analog(&self, ch: usize) -> u16 {
        if ch == board::BAT_ADC_CH && self.pin_level(board::BAT_REF_EN) {
            (board::BAT_K / self.vbat_mv.max(1.0)).round().min(1023.0) as u16
        } else {
            0
        }
    }

    // ---- boot ROM ----

    /// Emulates a call into the boot ROM (IAP). Returns a halt reason for "Reinvoke ISP".
    pub fn rom_call(&mut self, r: &mut [u32; 16]) -> Option<Halt> {
        let (cmd_ptr, res_ptr) = (r[0], r[1]);
        let cmd = self.read32(cmd_ptr);
        let p: Vec<u32> = (1..5).map(|i| self.read32(cmd_ptr + 4 * i)).collect();
        if self.trace.iap {
            self.trace_line(format!("iap: {} ({cmd}) params {:x?}", iap::name(cmd), p));
        }
        let mut result = vec![iap::CMD_SUCCESS];
        match cmd {
            iap::REINVOKE_ISP => return Some(Halt::Bootloader),
            iap::READ_PART_ID => result.push(iap::PART_ID),
            iap::READ_BOOT_VERSION => result.push(0x0000_0207),
            iap::READ_UID => result.extend_from_slice(&iap::UID),
            iap::PREPARE | iap::BLANK_CHECK | iap::COMPARE => {}
            iap::ERASE => {
                for s in p[0]..=p[1].min(15) {
                    let a = (s as usize) * 4096;
                    self.flash[a..a + 4096].fill(0xFF);
                }
            }
            iap::COPY_RAM_TO_FLASH => {
                let (dst, src, len) = (p[0] as usize, p[1], p[2] as usize);
                for i in 0..len.min(FLASH_SIZE.saturating_sub(dst)) {
                    let b = self.read8(src + i as u32);
                    self.flash[dst + i] &= b;
                }
            }
            _ => result[0] = iap::INVALID_COMMAND,
        }
        for (i, v) in result.iter().enumerate() {
            self.write32(res_ptr + 4 * i as u32, *v);
        }
        None
    }

    pub fn iocon_dump(&self) -> String {
        let mut s = self.iocon.describe();
        for (_, p) in IOCON_MAP {
            let g = &self.gpio[p.0 as usize];
            if g.dir & (1 << p.1) != 0 {
                s.push_str(&format!("\n{p} output {}", (g.data >> p.1) & 1));
            }
        }
        s
    }
}
