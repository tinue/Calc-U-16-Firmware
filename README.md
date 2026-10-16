# calcu16-fw

Calc-U-16 firmware for the SwissMicros DM16L.

The DM16L is a modern recreation of the HP-16C Computer Scientist. This project replaces its firmware with Calc-U-16, a programmer's calculator for coding and debugging: bases, word sizes, bit and boolean operations. Calc-U-16 also exists as an iOS app (`../calcu16`). The firmware runs natively on the calculator's ARM microcontroller, with no HP CPU emulator and no original ROM.

## ⚠ Warning: flashing is entirely at your own risk

**Flashing this firmware can brick your DM16L.** Read this before you flash anything.

- This is an unofficial hobby project. It is **not affiliated with, endorsed by or supported by SwissMicros or HP**.
- Flashing replaces the factory firmware and erases the calculator's memory. An interrupted or failed flash, a bug in this firmware, a mistake in these instructions or a mistake of yours can leave the calculator unusable, temporarily or permanently. It may also void your warranty.
- **The author accepts no responsibility or liability whatsoever** for damaged or bricked calculators, lost data or any other consequence, whatever the cause.
- **The author will not help you recover your calculator, and will not pay for repair or replacement.** Do not ask.
- **Do not expect SwissMicros to help either.** Their support covers their own products and firmware. They have no obligation to assist with a calculator that ran third-party firmware, and will most likely refuse.
- **Flashing has only been tested from a Mac running macOS.** Any other computer or operating system (Windows, Linux, …) is completely untested; the tools and instructions may not work there, or may fail halfway.
- If you are not prepared to lose your calculator for good, **do not flash it.**

Everything here is provided "as is", without warranty of any kind.

## Status

The work so far:
- the hardware has been reverse-engineered from the vendor firmware;
- a hardware simulator runs the original firmware unmodified, so the new firmware can be developed and tested before it is flashed onto a real calculator;
- a first version of the firmware runs in the simulator and on the real calculator: integer arithmetic in strict order of entry, HEX/DEC/OCT/BIN, word sizes 1..64, and the new two-line display ([docs/requirements.md](docs/requirements.md)).

## Layout

| Path | Contents |
|---|---|
| `docs/hardware.md` | Hardware reference: LPC111x MCU, pin map, LCD (ST7565-class, 132×16), keyboard matrix, I²C RTC (PCF8563), UART/USB (CP2102), battery measurement, clocks, sleep and wake-up, bootloader/ISP |
| `docs/keyboard.md` | Key labels (base, f/yellow, g/blue, brackets) mapped to matrix positions and key codes |
| `docs/images/` | Photo of the DM16L |
| `docs/reference/` | Scan of the original *HP-16C Owner's Handbook* |
| `sim/` | `voyager-sim`: Cortex-M0 + board emulator with a terminal UI, serial pty and scripted tests ([sim/README.md](sim/README.md)) |
| `tools/` | Helper scripts: vendor firmware download, guided flashing (`flash.py`), analysis notes and the disassembly helper |
| `vendor/` | SwissMicros factory firmware, **fetched, never committed** (`tools/fetch-vendor-firmware.sh`) |
| `docs/requirements.md` | Requirements and design decisions for Calc-U-16 on the DM16L |
| `firmware/` | The firmware (Rust): calculator core, display, board support ([firmware/README.md](firmware/README.md)) |
| `tools/display_lab/` | Display layout lab: prototypes screens and generates the firmware's fonts |

## Quick start

```sh
tools/fetch-vendor-firmware.sh          # SwissMicros firmware into vendor/
firmware/build.sh                       # → firmware/target/calcu16-fw.hex
cd sim && cargo test --release          # simulator tests: vendor firmware and calcu16-fw scenarios
cargo run --release -- run ../firmware/target/calcu16-fw.hex
```

## Intellectual property

The SwissMicros firmware images and artwork are not part of this repository; `.gitignore` excludes them. The HP-16C handbook scan and the photo are included as references.
