//! Calc-U-16 firmware for the SwissMicros DM16L: integer calculator (strict order of entry) with
//! HEX/DEC/OCT/BIN and word sizes 1..64.
//!
//! Keys: 0-9 A-F, + − × ÷, ENTER (=), HEX DEC OCT BIN, ← (backspace / clear X), CHS,
//! f STO (WSIZE: X is the new word size),
//! ON (off/on), f then + / − (LCD contrast), R/S held (status display). Hold D while
//! pressing ON, at reset, or send "bootloader" on the serial port to enter the ISP bootloader.

#![no_std]
#![no_main]

mod keys;
mod lcd;
mod power;
mod probe;
mod regs;
mod rtc;
mod timer;
mod uart;

use calc::display::{self, Frame, Model, Shift, Text, Ui};
use calc::{Base, Calc, Key};
use cortex_m_rt::entry;
use keys::PhysKey;
use panic_halt as _;
use regs::syscon;

const BANNER: &[u8] = b"Calc-U-16";
const KEY_D: PhysKey = PhysKey::Grid { row: 1, col: 4 };
/// Shows the status display while held.
const KEY_STATUS: PhysKey = PhysKey::Grid { row: 3, col: 1 }; // R/S

/// What a key does in this firmware.
enum Action {
    Calc(Key),
    On,
    F,
    Sto,
    None,
}

fn action(key: PhysKey) -> Action {
    use Action::*;
    let PhysKey::Grid { row, col } = key else {
        return On;
    };
    let d = |n| Calc(Key::Digit(n));
    match (row, col) {
        (1, c @ 1..=6) => d(9 + c), // A..F
        (1, 7) => d(7),
        (1, 8) => d(8),
        (1, 9) => d(9),
        (1, 10) => Calc(Key::Div),
        (2, 3) => Calc(Key::Base(Base::Hex)),
        (2, 4) => Calc(Key::Base(Base::Dec)),
        (2, 5) => Calc(Key::Base(Base::Oct)),
        (2, 6) => Calc(Key::Base(Base::Bin)),
        (2, 7) => d(4),
        (2, 8) => d(5),
        (2, 9) => d(6),
        (2, 10) => Calc(Key::Mul),
        (3, 5) => Calc(Key::Back),
        (3, 6) | (4, 6) => Calc(Key::Enter),
        (3, 7) => d(1),
        (3, 8) => d(2),
        (3, 9) => d(3),
        (3, 10) => Calc(Key::Sub),
        (4, 2) => F,
        (4, 4) => Sto,
        (4, 7) => d(0),
        (4, 9) => Calc(Key::Chs),
        (4, 10) => Calc(Key::Add),
        _ => None,
    }
}

fn show(lcd: &mut lcd::Lcd, m: &Model) {
    let mut frame = Frame::new();
    display::render(m, &mut frame);
    lcd.show(&frame);
    lcd.flush();
}

fn draw(lcd: &mut lcd::Lcd, calc: &Calc, ui: &Ui, contrast_msg: bool) {
    let m = if contrast_msg {
        let ev = lcd.contrast;
        Model::Message {
            top: Text::from(b"CONTRAST"),
            bottom: Text::from(&[b'E', b'V', b' ', b'0' + ev / 10, b'0' + ev % 10]),
        }
    } else {
        display::model(calc, ui)
    };
    show(lcd, &m);
}

fn init_clocks() {
    // When started from the ISP bootloader ("Go" after flashing, no reset), the boot ROM's
    // vector table is still mapped at 0, so our interrupt handlers would never run.
    regs::write(syscon::SYSMEMREMAP, 2);
    regs::modify(syscon::SYSAHBCLKCTRL, |v| {
        v | syscon::CLK_GPIO | syscon::CLK_IOCON
    });
}

/// True if ON reads `pressed` on `ticks` consecutive 10 ms scans; false at the first other reading.
fn on_stays(pressed: bool, ticks: u32) -> bool {
    (0..ticks).all(|_| {
        timer::wait_tick();
        keys::scan().pressed(PhysKey::On) == pressed
    })
}

fn diag_key(key: PhysKey) {
    match key {
        PhysKey::On => uart::write(b"key ON\r\n"),
        PhysKey::Grid { row, col } => {
            uart::write(b"key ");
            uart::write_dec(row as u32);
            uart::write(b"-");
            uart::write_dec((col % 10) as u32);
            uart::write(b"\r\n");
        }
    }
}

fn parse_hex(s: &[u8]) -> Option<u32> {
    let s = s.strip_prefix(b"0x").unwrap_or(s);
    if s.is_empty() || s.len() > 8 {
        return None;
    }
    s.iter()
        .try_fold(0u32, |v, &c| Some(v << 4 | (c as char).to_digit(16)?))
}

/// Serial monitor for hardware bring-up: peek/poke registers, scan keys, re-init the LCD.
fn monitor(cmd: &[u8], lcd_reinit: &mut bool) {
    let mut words = cmd.split(|&c| c == b' ').filter(|w| !w.is_empty());
    match words.next() {
        Some(b"bootloader") => {
            uart::write(b"entering bootloader\r\n");
            power::enter_isp();
        }
        Some(b"peek") => match words.next().and_then(parse_hex) {
            Some(a) if a % 4 == 0 => {
                uart::write_hex(a);
                uart::write(b" = ");
                uart::write_hex(regs::read(a));
                uart::write(b"\r\n");
            }
            _ => uart::write(b"usage: peek <hex addr>\r\n"),
        },
        Some(b"poke") => match (
            words.next().and_then(parse_hex),
            words.next().and_then(parse_hex),
        ) {
            (Some(a), Some(v)) if a % 4 == 0 => {
                regs::write(a, v);
                uart::write(b"ok\r\n");
            }
            _ => uart::write(b"usage: poke <hex addr> <hex value>\r\n"),
        },
        Some(b"scan") => {
            uart::write(b"scan ");
            uart::write(&keys::scan().hex());
            uart::write(b"\r\n");
        }
        Some(b"probe") => probe::probe(),
        Some(b"hunt") => probe::hunt(),
        Some(b"lcd") => *lcd_reinit = true,
        _ => {
            uart::write(b"commands: peek A | poke A V | scan | probe | hunt | lcd | bootloader\r\n")
        }
    }
}

#[entry]
fn main() -> ! {
    init_clocks();
    keys::init();
    power::park_unused_pins();
    // Escape hatch first: D held at reset enters the bootloader before anything else runs.
    // Require several consistent scans so a noisy line cannot trigger it.
    if (0..5).all(|_| {
        cortex_m::asm::delay(12_000);
        keys::scan().pressed(KEY_D)
    }) {
        power::enter_isp();
    }

    let mut lcd = lcd::Lcd::new();
    lcd.init();
    show(
        &mut lcd,
        &Model::Message {
            top: Text::new(),
            bottom: Text::from(b"CALC-U-16"),
        },
    );
    uart::init();
    uart::write(BANNER);
    uart::write(b"\r\n");
    // The RTC's /INT shares the ON line; a flag left set by other firmware would hold it low.
    if let Err(s) = rtc::silence_interrupts() {
        uart::write(b"RTC: i2c status ");
        uart::write_hex(s);
        uart::write(b"\r\n");
    }
    timer::init();

    let mut calc = Calc::new();
    let mut kbd = keys::Keyboard::new();
    let mut line = uart::LineReader::new();
    let mut ui = Ui::default();
    let mut contrast_msg = false;
    let mut tick: u32 = 0;
    let mut first_draw = true;

    loop {
        timer::wait_tick();
        tick = tick.wrapping_add(1);

        let mut lcd_reinit = false;
        line.poll(|cmd| monitor(cmd, &mut lcd_reinit));
        if lcd_reinit {
            lcd.init();
            draw(&mut lcd, &calc, &ui, contrast_msg);
            uart::write(b"lcd re-initialised\r\n");
        }

        // Keep the banner for half a second after power-up.
        if first_draw {
            if tick < 50 {
                continue;
            }
            first_draw = false;
            draw(&mut lcd, &calc, &ui, contrast_msg);
        }

        let scan = keys::scan();
        if scan.pressed(KEY_STATUS) != ui.status {
            ui.status = !ui.status;
            draw(&mut lcd, &calc, &ui, contrast_msg);
        }
        let Some(key) = kbd.update(scan) else {
            continue;
        };
        diag_key(key);
        let was_f = ui.shift.take() == Some(Shift::F);
        contrast_msg = false;
        match action(key) {
            Action::On => {
                uart::write(b"off\r\n");
                lcd.sleep();
                // The release of ON bounces, and every bounce is a falling edge that would
                // wake the chip at once: sleep only after ON has been released stably.
                while !on_stays(false, 5) {}
                // Wake-ups are only a hint (bounce, RTC /INT glitch): turn on only if ON
                // is really held, as the vendor firmware does.
                loop {
                    power::sleep_until_on();
                    if on_stays(true, 3) {
                        break;
                    }
                }
                uart::write(b"on\r\n");
                if keys::scan().pressed(KEY_D) {
                    power::enter_isp();
                }
                lcd.init();
                kbd.sync();
            }
            Action::F => ui.shift = if was_f { None } else { Some(Shift::F) },
            Action::Calc(Key::Add) if was_f => {
                let ev = lcd.contrast.saturating_add(1).min(63);
                lcd.set_contrast(ev);
                contrast_msg = true;
            }
            Action::Calc(Key::Sub) if was_f => {
                let ev = lcd.contrast.saturating_sub(1);
                lcd.set_contrast(ev);
                contrast_msg = true;
            }
            Action::Sto if was_f => calc.key(Key::Wsize),
            Action::Sto => {}
            Action::Calc(k) => calc.key(k),
            Action::None => {}
        }
        draw(&mut lcd, &calc, &ui, contrast_msg);
    }
}
