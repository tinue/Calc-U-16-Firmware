//! NXP PCF8563 real-time clock (I2C address 0x51).
//!
//! Time runs with emulated time: `base + now_s + offset`, so fast-forwarding
//! the emulation also advances the clock.

use crate::periph::i2c::I2cDevice;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

fn bcd(v: u32) -> u8 {
    (((v / 10) << 4) | (v % 10)) as u8
}

fn unbcd(b: u8) -> u32 {
    ((b >> 4) as u32) * 10 + (b & 15) as u32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rtc {
    /// Wall-clock time corresponding to emulated time 0.
    pub base: NaiveDateTime,
    /// Seconds added by "set time" operations.
    pub offset: i64,
    /// Current emulated time in seconds (updated by the bus before each access).
    pub now_s: f64,
    /// Time frozen while the STOP bit is set.
    pub frozen: Option<NaiveDateTime>,
    pub ctrl1: u8,
    pub ctrl2: u8,
    pub vl: bool,
    pub alarm: [u8; 4],
    pub clkout: u8,
    pub timer_ctrl: u8,
    pub timer_count: u8,
    /// Emulated time (s) of the next timer expiry, if the timer runs.
    pub timer_next: Option<f64>,
    /// Last minute checked for alarm matches.
    pub alarm_minute: i64,
    /// Weekday register = (days since Monday + offset) % 7. The PCF8563 leaves the
    /// weekday numbering to software; the firmware's last write sets the offset.
    pub weekday_offset: u8,
    // Transaction state
    ptr: u8,
    first_write: bool,
    /// Time registers latched at transaction start; written fields are committed at STOP.
    latch: [u8; 7],
    time_written: bool,
}

impl Rtc {
    pub fn new(base: NaiveDateTime, power_lost: bool) -> Self {
        Rtc {
            base,
            offset: 0,
            now_s: 0.0,
            frozen: None,
            ctrl1: 0,
            ctrl2: 0,
            vl: power_lost,
            alarm: [0x80; 4],
            clkout: 0x80,
            timer_ctrl: 0x03,
            timer_count: 0,
            timer_next: None,
            alarm_minute: 0,
            weekday_offset: 0,
            ptr: 0,
            first_write: false,
            latch: [0; 7],
            time_written: false,
        }
    }

    pub fn time(&self) -> NaiveDateTime {
        self.frozen.unwrap_or_else(|| {
            self.base + Duration::milliseconds((self.now_s * 1000.0) as i64) + Duration::seconds(self.offset)
        })
    }

    pub fn set_time(&mut self, t: NaiveDateTime) {
        if self.frozen.is_some() {
            self.frozen = Some(t);
        } else {
            let running = self.base + Duration::milliseconds((self.now_s * 1000.0) as i64);
            self.offset = (t - running).num_seconds();
        }
    }

    /// Open-drain INT output active (low).
    pub fn int_active(&self) -> bool {
        (self.ctrl2 & 0x08 != 0 && self.ctrl2 & 0x02 != 0) || (self.ctrl2 & 0x04 != 0 && self.ctrl2 & 0x01 != 0)
    }

    fn timer_period(&self) -> Option<f64> {
        if self.timer_ctrl & 0x80 == 0 || self.timer_count == 0 {
            return None;
        }
        let hz = [4096.0, 64.0, 1.0, 1.0 / 60.0][(self.timer_ctrl & 3) as usize];
        Some(self.timer_count as f64 / hz)
    }

    fn restart_timer(&mut self) {
        self.timer_next = self.timer_period().map(|p| self.now_s + p);
    }

    /// Next emulated time at which `service` has something to do.
    pub fn next_event(&self) -> Option<f64> {
        let alarm = (self.ctrl2 & 0x02 != 0 && self.alarm.iter().any(|a| a & 0x80 == 0)).then(|| {
            let t = self.time();
            self.now_s + 60.0 - t.second() as f64
        });
        match (self.timer_next, alarm) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Advances timer/alarm flags to the current emulated time.
    pub fn service(&mut self) {
        while let (Some(t), Some(p)) = (self.timer_next, self.timer_period()) {
            if t > self.now_s {
                break;
            }
            self.ctrl2 |= 0x04;
            self.timer_next = Some(t + p);
        }
        let now = self.time();
        let minute = now.and_utc().timestamp() / 60;
        if minute != self.alarm_minute {
            self.alarm_minute = minute;
            let fields = [
                (self.alarm[0], now.minute()),
                (self.alarm[1], now.hour()),
                (self.alarm[2], now.day()),
                (self.alarm[3], now.weekday().num_days_from_sunday()),
            ];
            let enabled: Vec<_> = fields.iter().filter(|(a, _)| a & 0x80 == 0).collect();
            if !enabled.is_empty() && enabled.iter().all(|(a, v)| unbcd(a & 0x7F) == *v) {
                self.ctrl2 |= 0x08;
            }
        }
    }

    fn time_regs(&self) -> [u8; 7] {
        let t = self.time();
        [
            bcd(t.second()) | if self.vl { 0x80 } else { 0 },
            bcd(t.minute()),
            bcd(t.hour()),
            bcd(t.day()),
            ((t.weekday().num_days_from_monday() as u8) + self.weekday_offset) % 7,
            bcd(t.month()) | if t.year() >= 2100 { 0x80 } else { 0 },
            bcd((t.year() % 100) as u32),
        ]
    }

    fn read_reg(&self, r: u8) -> u8 {
        match r {
            0 => self.ctrl1,
            1 => self.ctrl2,
            2..=8 => self.latch[(r - 2) as usize],
            9..=12 => self.alarm[(r - 9) as usize],
            13 => self.clkout,
            14 => self.timer_ctrl,
            15 => self.timer_count,
            _ => 0,
        }
    }

    fn write_reg(&mut self, r: u8, v: u8) {
        match r {
            0 => {
                let was_stopped = self.ctrl1 & 0x20 != 0;
                self.ctrl1 = v & 0xA8;
                let stopped = v & 0x20 != 0;
                if stopped && !was_stopped {
                    self.frozen = Some(self.time());
                } else if !stopped && was_stopped {
                    if let Some(t) = self.frozen.take() {
                        self.set_time(t);
                    }
                }
            }
            1 => {
                // TF/AF are cleared by writing 0, unchanged by writing 1.
                let keep = self.ctrl2 & v & 0x0C;
                self.ctrl2 = (v & 0x13) | keep;
            }
            2..=8 => {
                self.latch[(r - 2) as usize] = v;
                self.time_written = true;
            }
            9..=12 => self.alarm[(r - 9) as usize] = v,
            13 => self.clkout = v & 0x83,
            14 => {
                self.timer_ctrl = v & 0x83;
                self.restart_timer();
            }
            15 => {
                self.timer_count = v;
                self.restart_timer();
            }
            _ => {}
        }
    }

    fn commit_time(&mut self) {
        let l = self.latch;
        let year = 2000 + unbcd(l[6]) as i32 + if l[5] & 0x80 != 0 { 100 } else { 0 };
        let date = NaiveDate::from_ymd_opt(year, unbcd(l[5] & 0x1F).max(1), unbcd(l[3] & 0x3F).max(1));
        if let Some(t) = date.and_then(|d| d.and_hms_opt(unbcd(l[2] & 0x3F), unbcd(l[1] & 0x7F), unbcd(l[0] & 0x7F))) {
            self.set_time(t);
            self.weekday_offset = ((l[4] & 7) + 7 - (t.weekday().num_days_from_monday() as u8 % 7)) % 7;
        }
        self.vl = l[0] & 0x80 != 0;
    }
}

impl I2cDevice for Rtc {
    fn address(&self) -> u8 {
        super::RTC_ADDR
    }

    fn start(&mut self, read: bool) -> bool {
        self.stop();
        self.latch = self.time_regs();
        self.time_written = false;
        self.first_write = !read;
        true
    }

    fn write(&mut self, byte: u8) -> bool {
        if self.first_write {
            self.ptr = byte & 15;
            self.first_write = false;
        } else {
            self.write_reg(self.ptr, byte);
            self.ptr = (self.ptr + 1) & 15;
        }
        true
    }

    fn read(&mut self) -> u8 {
        let v = self.read_reg(self.ptr);
        self.ptr = (self.ptr + 1) & 15;
        v
    }

    fn stop(&mut self) {
        if self.time_written {
            self.commit_time();
            self.time_written = false;
        }
    }
}
