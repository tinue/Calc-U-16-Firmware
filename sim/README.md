# voyager-sim

A hardware simulator for the SwissMicros Voyager (DM-L) calculators. It runs the **unmodified vendor firmware** (`DM16_34.hex`, `DM16_MC0_34.hex`, `DM15_34.hex`, `DM12_34.hex`) on a model of the NXP LPC111x and the DM-L board.

It does not reimplement any calculator logic. The vendor firmware runs its own HP Nut emulator and the original ROM; this program only emulates the ARM CPU and the hardware around it. That makes it useful for two things:

- **Checking the hardware notes:** if the firmware works here, the pin map in `../docs/hardware.md` and `../docs/keyboard.md` is right.
- **Testing new firmware** before flashing a real calculator.

## Build and run

The firmware images are SwissMicros property and are not in this repository. Fetch them first:

```sh
../tools/fetch-vendor-firmware.sh   # downloads into ../vendor/ (git-ignored)
cargo build --release
./target/release/voyager-sim run ../vendor/DM16_34.hex
./target/release/voyager-sim run ../vendor/DM15_34.hex --speed 4
./target/release/voyager-sim script ../vendor/DM16_34.hex tests/scripts/dm16_arithmetic.vs
./target/release/voyager-sim boot ../vendor/DM16_34.hex --secs 3 --trace periph,i2c
./target/release/voyager-sim dis ../vendor/DM16_34.hex 3180 3300
cargo test --release
```

### Subcommands

**`run`**: interactive terminal UI.
- **Screen:** the LCD (132×16 pixels, drawn with half-block characters), the keyboard with base, f and g legends plus the PC key bound to each key, the serial log and messages.
- **Serial port:** the emulated UART is exposed as a pseudo-terminal, shown in the header, so `screen /dev/ttysNNN 38400` reaches the `VOYAGER >>` console.
  - `--pty-link /tmp/voyager-tty` also creates a stable symlink to it.
- **State:** the machine state (RAM, peripherals, RTC offset) is saved on exit to `state/<firmware>.state` and restored on the next start. The real calculator keeps RAM powered while it is off, so this matches its behaviour. Use `--no-state` to start fresh.
- **Keys:**

  | Key | Action |
  |---|---|
  | F1 | help |
  | F2 | hold/release ON |
  | F3 | switch between mnemonic and grid key maps |
  | F4 | latch keys (they stay pressed) |
  | F5 | hardware reset |
  | F6 | pause |
  | F7 | screenshot: save the LCD as a photo-like SVG in `screenshots/` |
  | `:` | command line (the script language) |
  | F10 | quit |

- **Key releases:** most terminals don't report them, so each key is a 120 ms tap. Terminals with the kitty keyboard protocol (kitty, WezTerm, Ghostty, recent iTerm2) do report them, and keys are then held for as long as you hold them.

**`script`**: headless runs for tests; the exit code is non-zero if any expectation fails. The language is described at the top of `src/frontend/script.rs`. Summary:

```text
keys 2 ENTER 3 +        press ON 300ms        hold E / release all
wait 2s                 screen                expect-screen golden/x.txt
serial "?\r"            expect-serial "VOYAGER >> " 5s
expect-halt bootloader  expect-beeps 2        expect-display on|off
vbat 2900               rtc 2026-10-16T08:00:00
state save F / load F   info / pins / beeps
```

Keys are named by legend (`ENTER`, `CHS`, `x<>y`, `SL`, `CLx`, `SHOW HEX`) or by grid position (`3-6`). Shifted legends add the `f`/`g` prefix automatically.

**`boot`**: runs for N emulated seconds, then prints the trace, the serial output, the screen, the emulation speed, and any registers the model does not implement.

**`dis`**: a Thumb disassembler that also shows the values loaded from literal pools.

## The vendor firmware's key combinations

| Combination | Effect |
|---|---|
| ON alone | switches on and off |
| Calculator off: hold a setup item's key, then press ON | runs that setup item (grid positions as in the manual) |
| 1-1 (A) | date and time |
| 1-3 (C) | serial console |
| 1-4 (D) | bootloader |
| 1-5 (E) | system info |
| 1-7 | cycle font |
| 3-7 | 1-line display |
| 3-8 | 2-line display |
| ON with ÷ / × / − (HP combinations) | keyboard test / self test / memory reset |
| Any byte on the UART within about 0.5 s of reset | starts the serial console |

## What is modelled

| Part | Model |
|---|---|
| CPU | Cortex-M0: the full ARMv6-M Thumb instruction set; NVIC with priorities and level-sensitive lines; SysTick (stopped in deep sleep); SCB, WFI/WFE, SYSRESETREQ |
| SYSCON | clock tree (12 MHz internal oscillator, PLL, about 9.4 kHz watchdog oscillator); start-logic wake-up; PMU; deep sleep fast-forwards to the next event |
| IOCON, GPIO | pin levels resolved from GPIO outputs, timer match outputs, the board, and the pull resistors |
| CT16B0/1, CT32B0/1 | prescaler, match interrupts, reset and stop on match, external match outputs (the PIO0_9 wake trick) |
| UART | 16550-style; received bytes are paced at the programmed baud rate; bytes arriving in deep sleep are dropped, as on the real chip |
| I²C | master state machine |
| ADC | software-started conversions |
| Boot ROM (IAP) | part ID, UID, boot code version, flash erase/copy; "Reinvoke ISP" stops the simulation |
| Board | 10×4 key matrix + ON line; bit-banged SPI to an ST7565-class LCD controller; PCF8563 RTC at I²C address 0x51 (runs on emulated time); beeper on PIO1_1/PIO1_2; battery reference on AD7 |

The emulator runs at about 135 MIPS, which is roughly 3× real time at 48 MHz and 10× at 12 MHz. Idle time in deep sleep costs almost nothing.

## Possible GUI front end

A graphical front end could use SwissMicros' front drawing of the DM16L as the background:
https://github.com/swissmicros/visual_assets/blob/main/drawings/calculator/bitmap/drw_dm16l_front_600.png
It is SwissMicros artwork: download it at build or run time (e.g. into `assets/swissmicros/`, which is git-ignored), never commit it.

## Known limitations

- Instruction timing is approximate: 1 cycle per instruction, plus extra cycles for memory access and branches.
- Analog inputs other than the battery measurement read 0.
- No watchdog, SSP or flash wait-state model; the firmware does not use them.
- After a cold start the RTC runs, but the firmware's century variable is 0 until the time is set (`ts` on the console). The real unit sets it when the RTC reports lost power (`--rtc-lost`).
- Debug: `VOYAGER_DEBUG=1` logs each key press in the TUI's message pane.
