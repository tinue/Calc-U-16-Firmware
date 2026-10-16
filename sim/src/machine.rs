//! The complete emulated calculator: CPU + bus, run loop, host-facing API.

use crate::board::keyboard::Key;
use crate::board::lcd;
use crate::board::rtc::Rtc;
use crate::bus::{Bus, Trace, FLASH_SIZE};
use crate::cpu::{Cpu, Halt};
use anyhow::{bail, Context, Result};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub struct Config {
    pub vbat_mv: f64,
    pub rtc_base: NaiveDateTime,
    pub rtc_power_lost: bool,
    pub trace: Trace,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            vbat_mv: 3000.0,
            rtc_base: chrono::Local::now().naive_local(),
            rtc_power_lost: false,
            trace: Trace::default(),
        }
    }
}

pub struct Machine {
    pub cpu: Cpu,
    pub bus: Bus,
    /// FNV-1a hash of the firmware image (guards state files).
    pub fw_hash: u64,
}

#[derive(Serialize, Deserialize)]
struct SavedState {
    fw_hash: u64,
    cpu: Cpu,
    bus: Bus,
}

fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

impl Machine {
    pub fn new(flash: Vec<u8>, cfg: &Config) -> Self {
        assert_eq!(flash.len(), FLASH_SIZE);
        let fw_hash = fnv(&flash);
        let mut bus = Bus::new(flash, Rtc::new(cfg.rtc_base, cfg.rtc_power_lost));
        bus.vbat_mv = cfg.vbat_mv;
        bus.trace = cfg.trace;
        bus.lcd.log_enabled = cfg.trace.lcd;
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        Machine { cpu, bus, fw_hash }
    }

    pub fn from_hex(path: &Path, cfg: &Config) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let flash = crate::ihex::parse(&text, FLASH_SIZE)?;
        Ok(Machine::new(flash, cfg))
    }

    /// Hardware reset (reset button): MCU state is lost, RAM content is kept.
    pub fn reset(&mut self) {
        self.bus.reset_mcu(false);
        self.cpu.reset(&mut self.bus);
    }

    pub fn now(&self) -> f64 {
        self.bus.now_s()
    }

    pub fn halted(&self) -> Option<&Halt> {
        self.cpu.halted.as_ref()
    }

    /// Runs until emulated time `t` (seconds) or until the CPU halts.
    pub fn run_until(&mut self, t: f64) {
        let bus = &mut self.bus;
        let cpu = &mut self.cpu;
        loop {
            if cpu.halted.is_some() {
                return;
            }
            if bus.scs.reset_requested {
                bus.reset_mcu(true);
                cpu.reset(bus);
                if bus.trace.periph {
                    bus.trace_line("system reset (AIRCR.SYSRESETREQ)".into());
                }
            }
            if bus.cycles >= bus.next_event {
                bus.service();
            }
            if bus.now_s() >= t {
                return;
            }
            let deadline = bus.cycles_at(t);
            if cpu.sleeping && !bus.scs.wakeup_pending() {
                // Nothing to execute: skip to the next event or the deadline.
                bus.cycles = bus.next_event.min(deadline).max(bus.cycles + 1);
                continue;
            }
            while bus.cycles < deadline && bus.cycles < bus.next_event {
                bus.cycles += cpu.step(bus) as u64;
                if cpu.halted.is_some() || (cpu.sleeping && !bus.scs.wakeup_pending()) || bus.scs.reset_requested {
                    break;
                }
            }
        }
    }

    pub fn run_for(&mut self, dt: f64) {
        let t = self.now() + dt;
        self.run_until(t);
    }

    // ---- keyboard ----

    pub fn key(&mut self, key: Key, down: bool) {
        self.bus.keyboard.set(key, down);
        self.bus.update_pins();
    }

    pub fn release_all(&mut self) {
        self.bus.keyboard.release_all();
        self.bus.update_pins();
    }

    // ---- serial ----

    pub fn serial_send(&mut self, bytes: &[u8]) {
        self.bus.uart_send(bytes);
    }

    pub fn serial_take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bus.uart.tx)
    }

    // ---- display ----

    pub fn pixels(&self) -> [[bool; lcd::WIDTH]; lcd::HEIGHT] {
        self.bus.lcd.pixels()
    }

    pub fn screen_text(&self) -> String {
        self.bus.lcd.text_art()
    }

    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.bus.log)
    }

    // ---- state ----

    pub fn save_state(&self, path: &Path) -> Result<()> {
        let s = SavedState { fw_hash: self.fw_hash, cpu: self.cpu.clone(), bus: clone_bus(&self.bus)? };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, bincode::serialize(&s)?)?;
        Ok(())
    }

    pub fn load_state(&mut self, path: &Path) -> Result<()> {
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let s: SavedState = bincode::deserialize(&data).context("decoding state file")?;
        if s.fw_hash != self.fw_hash {
            bail!("state file {} was saved with a different firmware image", path.display());
        }
        let flash = std::mem::take(&mut self.bus.flash);
        let trace = self.bus.trace;
        self.cpu = s.cpu;
        self.bus = s.bus;
        self.bus.flash = flash;
        self.bus.trace = trace;
        self.bus.next_event = 0;
        Ok(())
    }
}

fn clone_bus(bus: &Bus) -> Result<Bus> {
    Ok(bincode::deserialize(&bincode::serialize(bus)?)?)
}
