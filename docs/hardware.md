# SwissMicros DM16L: hardware as seen by firmware V34

Derived from `DM16_34.hex` (46 KB image, loaded at 0x0000, initial SP 0x10001FF0). All of this comes from static analysis of the disassembly; none of it has been checked on real hardware. Items marked **(inferred)** are deductions, not something the code states directly.

## MCU
- **NXP LPC111x** (Cortex-M0): 64 KB flash, 8 KB SRAM at 0x10000000. Peripherals sit at the LPC111x addresses (IOCON 0x40044000, SYSCON 0x40048000, GPIO 0x500n0000, start-logic IRQs 0–12). The part is most likely an LPC1114/LPC1115 in a 48-pin package, since ports 0–3 are all used.
- **No native USB.** The USB socket goes to an external USB-UART bridge **(inferred)** on UART PIO1_6 (RXD) and PIO1_7 (TXD). These are also the LPC ROM ISP pins, so firmware is flashed through the same cable.
- Flashing a new firmware:
  1. Use "Start bootloader" in the setup menu, or type `bootloader` three times on the serial console. Either calls IAP command 57 (Reinvoke ISP).
  2. Then use a standard tool such as `lpc21isp` or Flash Magic.
- PIO0_1 is the ISP-entry pin (the vendor firmware switches its internal pull-down on at run time). Pressing a hardware reset with it held low gives the same ISP entry.
- **The PGM button next to RESET does not reach PIO0_1** (tested 2026-10-16 on our unit):
  - The documented SwissMicros sequence (hold PGM, press and release RESET, release PGM) only resets the calculator.
  - With calcu16-fw running, PIO0_1 as an input with its pull-up and then its pull-down never changed level while PGM was pressed.
  - No firmware image involved sets Code Read Protection (word at 0x2FC: 0xFFFFFFFF in all vendor images), so CRP is not the reason. The button may work on other board revisions or models.
- **Consequence: without a hardware modification, the firmware's own escapes are the only way into the bootloader.** On this unit, calcu16-fw must always keep them working:
  - D held at reset, checked before anything else runs;
  - D + ON from off;
  - the serial `bootloader` command.
  They are covered by the simulator tests `fw_escape_reset`, `fw_escape_on` and `fw_power_and_isp`. A firmware that crashes before it checks D leaves only hardware recovery: PIO0_1 held low at reset by a wire to the pin, or SWD (SWCLK is shared with keyboard row R3, so connect under reset).
- Code Read Protection: a word 0x4E697370 (NO_ISP) or 0x43218765 (CRP3) at flash address 0x2FC disables ISP entry by pin; CRP3 also blocks all later reflashing for good. `firmware/tools/lpc_image.py` refuses images with any CRP pattern.

## Consolidated pin map
| Pin | Function | Notes |
|---|---|---|
| PIO0_1 | ISP entry | pull-down enabled |
| PIO0_2 | LCD SDA/MOSI | bit-banged |
| PIO0_3 | KBD column C3 | |
| PIO0_4 / PIO0_5 | I²C SCL / SDA | 100 kHz; RTC PCF8563 at 0x51 |
| PIO0_6 | KBD column C9 | |
| PIO0_7 | KBD row R1 (top) | wake pin |
| PIO0_8 | KBD row R2 | wake pin |
| PIO0_9 | CT16B0_MAT1 | timed wake from deep sleep through start logic; keep it unconnected |
| PIO0_10 | KBD row R3 | default function SWCLK, so set it to GPIO explicitly |
| PIO0_11 | ON key, and RTC /INT **(inferred)** | pull-up, wake pin; ON connects to C0 (PIO2_7) |
| PIO1_0 | KBD row R4 (bottom) | default function TMS, so set it to GPIO explicitly |
| PIO1_1 / PIO1_2 | piezo beeper | push-pull, driven in antiphase by the CT32B1 ISR |
| PIO1_4 | input, no pull | purpose unknown (possibly USB/VBUS sense?) |
| PIO1_6 / PIO1_7 | UART RXD / TXD | 38400 8N1; USB-UART bridge |
| PIO1_8 | LCD SCK | |
| PIO1_9 | KBD column C4 | |
| PIO1_11 | AD7 battery measurement | |
| PIO2_0 | LCD A0 (command/data) | |
| PIO2_1 | KBD column C2 | |
| PIO2_4 / PIO2_5 | KBD columns C6 / C7 | |
| PIO2_6 | LCD /RESET **(inferred)** | |
| PIO2_7 / PIO2_8 | KBD columns C0 / C1 | |
| PIO3_2 | battery reference enable | high only during a measurement |
| PIO3_3 | LCD /CS **(inferred)** | held low |
| PIO3_4 / PIO3_5 | KBD columns C5 / C8 | |

## Summary by subsystem

**Display**
- ST7565R/ST7567-class controller, 132×16 pixels (2 pages), write-only bit-banged SPI mode 3.
- No backlight.
- The rows are mirrored: the framebuffer's first band goes to page 1 (`B1`), the second to page 0 (`B0`).
- Annunciators are pixels drawn in software; there are no dedicated segments.
- Sleep: `AC AE A5`. On wake, run the full init again.

**Keyboard**
- 10 columns are driven low one at a time; 4 rows are read through pull-ups. ON is a separate line, PIO0_11.
- Scanning is polled, with a 3-read glitch filter and no time-based debounce.
- The wake-capable row pins let any key wake the chip from deep sleep.

**Power**
- "Off" means deep-sleep with RAM retained. Deep power-down is never used, and the state is never written to flash.
- Before sleeping:
  - the columns are driven low so any key can wake the chip;
  - the I²C pins are parked high;
  - analog blocks are powered off (`PDSLEEPCFG` 0x18FF).
- Timed wakes: the main clock switches to the ~9.4 kHz watchdog oscillator, and CT16B0 MAT1 on PIO0_9 triggers the start logic.
- Auto-off after about 3 minutes.

**Clocks**
- 12 MHz internal oscillator; the PLL (`SYSPLLCTRL` 0x23) gives 48 MHz.
- "Toggle CPU speed" switches between 12 and 48 MHz.
- SysTick provides a 1 ms tick.

**Battery**
- PIO3_2 high enables a ≈1.234 V reference on AD7. The ADC reference is VDD, which is the battery.
- Vbat[mV] = 1263405 / ADC10.
- Low battery is flagged at ≤ 2100 mV. The supply is a CR2032 coin cell.

**RTC**
- PCF8563-compatible chip at I²C address 0x51.
- After a power loss, its low-voltage flag makes the firmware set the time to 2014-01-01.

**Timers**
| Timer | Use |
|---|---|
| SysTick | 1 ms tick |
| CT16B0 | timed wake from deep sleep |
| CT16B1 | 6.4 kHz interrupt; appears to pace the emulator |
| CT32B0 | 30 ms periodic interrupt for the main loop |
| CT32B1 | beeper |

**Serial console**
- 38400 baud, prompt `VOYAGER >> `, commands `? p s l ct bootloader t ts td kb d b`.
- `s` and `l` save and load the calculator state as text with a `DM16` header. This is the only backup path.

## Checked on the real calculator (2026-10-16)
- The USB bridge is a **Silicon Labs CP2102** (VID 0x10C4, PID 0xEA60). On macOS it appears as `/dev/cu.usbserial-0001`.
- The console works at 38400 8N1 once it is started from the setup menu. The firmware ID is `DM16_MC0_V34`. The commands are as listed above; `p` is "Key press".
- `b` reported `BAT: 1958mV`. That is below the firmware's 2100 mV low-battery threshold, so the coin cell is nearly empty.
- `t` reported 07:11 while the host clock showed 08:07 CEST. The RTC is about 56 minutes behind, probably because it was never moved to summer time.
- **DTR and RTS are not connected to reset or the ISP pin.** Pulsing DTR, RTS and both together for 0.5 s left the running console untouched. Entering ISP therefore needs the firmware's IAP 57 call, or the reset button plus PIO0_1.
- Earlier, the console dropped back to calculator mode on its own; that was its timeout.
- A memory dump from `s` was saved to `tools/analysis/state_dump_2026-10-16.txt` (format: `DM16_MC0` header, then the registers in hex).
- **GPIO DATA writes to pins configured as inputs are ignored**, through both the masked addresses and `0x3FFC`. Set GPIOnDIR first, then write the level. The vendor code does it in that order. Writing the level first left every keyboard column high, so no key registered, and after a reset the LCD lines started at wrong levels. Measured with the PoC's serial `peek`/`poke` on the spare pin PIO2_2.
- The ISP route via IAP 57 (serial `bootloader` command) does not reset the peripherals. A firmware started by lpc21isp after flashing therefore inherits the previous firmware's GPIO directions and levels, for example the vendor's PIO1_11/PIO3_2 driven low. Test start-up behaviour with the reset button, too.
- The key matrix works as documented above (verified with the PoC: `HEX FF ENTER DEC 55 +` → `310 d`).
- **The ON key bounces on release.** If the firmware enters deep sleep while ON is still held, the bounce often wakes it at once. Sleep only after ON has been released stably, and after a wake-up check that ON is really held, as the vendor firmware does. With both in place, the PoC stays off reliably.
- For low off-current, leave no pull-up enabled on a pin that is driven low, and park the battery-measurement pins PIO3_2/PIO1_11 low (see "Battery measurement"). These values are taken from the vendor's pin state and are not measured.

## Confirmed by the simulator (`sim/`, 2026-10-16)
All four vendor firmwares (DM16, DM16_MC0, DM15, DM12 V34) run unmodified on a model built from this document, so the pin map and protocols below are confirmed in practice.
- **LCD:** the init sequence, the SPI pins and the page/row mapping are correct. Text is upright and not mirrored, so the segment direction is normal.
- **Keyboard:** the matrix and ON line are as documented.
  - ENTER is a single switch. Modelling it as closing both C5·R3 and C5·R4 makes two start-logic interrupts pend at once. The vendor firmware then hangs in deep sleep, so the real key must close only one contact.
  - The firmware decodes both positions as ENTER.
- **RTC weekday:** the firmware numbers weekdays from 0 = Monday in register 6.
- **Timed wake from deep sleep:**
  - MR0 resets CT16B0. MR1 is left at 0, so it matches each time the counter passes through 0 and drives MAT1 (PIO0_9) high, which wakes the chip through start logic 9.
  - The main loop wakes every 0.5 s, and auto-off comes after 360 idle wakes (about 3 min).
  - SysTick must not run in deep sleep; otherwise the 1 ms tick wakes the CPU continuously.
- **Start logic:** STARTSRP0 latches only enabled inputs, and a stale NVIC pending bit is fatal. The firmware never clears NVIC pending bits for IRQ 0–12 (no ICPR writes). New firmware should write ICPR before enabling wake interrupts.
- **Serial console:**
  - A byte within about 0.5 s of reset starts it.
  - `l` (load) accepts records ending in CR or LF; CRLF ends the transfer early ("Read FAILED").
- **Setup items:** with the calculator off, hold the item key (grid position from the manual's table) and press ON. Example: E (1-5) = System info "DM16 V34 12MHZ 2.800V RTC".

## From the SwissMicros user manual
- The battery is a CR2032. State survives about 30 s without a battery. The calculator cannot run from USB power.
- Flashing works with `lpc21isp <fw.hex> <port> 115200 12000` once the bootloader is running (setup menu item D, "Start bootloader"). This is the standard LPC ROM ISP: 115200 baud, 12 MHz IRC.
- There is a **hardware reset button** on the back of the PCB, lower left; some backplates have a paperclip hole for it. Pressing it cancels the bootloader. Before firmware V33, the bootloader was entered with this button (together with PGM, per SwissMicros; this does not work on our unit, see above).
- Pressing any calculator key ends serial console mode.
- **The ROM bootloader accepts one handshake per reset** (found the hard way, 2026-10-16):
  - It measures the baud rate from the first characters it receives. Stray bytes (e.g. a console command) lock it to a wrong rate.
  - A completed handshake (e.g. `lpc21isp -detectonly`) leaves it synchronised, so a second `lpc21isp` run cannot sync either.
  - In both cases only a reset helps. So the flash tool must be the first and only program to talk to it (`tools/flash.py` does this).

## Open points to verify on real hardware
- Whether PIO1_4 is VBUS sense. The bridge is now known to be a CP2102.
- Whether the RTC /INT line really goes to PIO0_11.
- Whether PIO2_6 and PIO3_3 really are LCD /RESET and /CS.
- Whether the ON key really sits between PIO2_7 and PIO0_11. The matrix table also lists ON at C0×R4.
- SWD access: PIO0_10 (SWCLK) is used as keyboard row R3, so for debugging, connect under reset.

The detailed analyses, with code addresses as evidence, follow below. `tools/analysis/dis.py` is the annotated Thumb disassembler used to produce them: `uv run --with capstone python dis.py fw.bin <start> <end>`, where `fw.bin` is built with `arm-none-eabi-objcopy -I ihex -O binary vendor/DM16_34.hex fw.bin`.

---
# DM-16L display hardware (from DM16_34 firmware)

## Interface: bit-banged 3/4-wire SPI, write-only (NOT I2C)
The I2C bus (PIO0_4/PIO0_5, ISR @0xC0) is used for the RTC at 0xA2/0xA3 = 7-bit 0x51
(routines 0x29a0..0x2b74, regs 2..8 → PCF8563/PCF85063-class). The LCD is not on I2C.

| Signal | Pin | Evidence |
|---|---|---|
| SCK  | PIO1_8 (GPIO1 bit 8) | 0x21c0: GPIO1 DATA \|= 0x100 / &= ~0x100 |
| MOSI | PIO0_2 (GPIO0 bit 2) | 0x21c0: GPIO0 DATA bit 2 |
| A0 (C/D) | PIO2_0 | 0x2210: clears bit 0 = command; 0x2272: sets bit 0 = data |
| /RES (probably) | PIO2_6 | 0x2408..0x2418: driven 0 then 1 |
| /CS (probably) | PIO3_3 | 0x23fe: driven 0 and left low |

Pin setup 0x23d4: all five pins made outputs and driven high (0x22b6 = dir out + set),
then PIO3_3 = 0, PIO2_6 = 0 → 1. These pins are not the SSP0 pins, so the SPI is bit-banged.
Byte send 0x21c0: MSB first, 8 bits. SCK idles high. For each bit: SCK low, set MOSI, SCK high.
The LCD therefore samples on the rising edge (SPI mode 3). There is no extra delay, so the
clock rate is set by the CPU speed. A full refresh (0x2228) runs with interrupts disabled (cpsid/cpsie).

## Controller: ST7565R / ST7567-compatible (132-column page-mode)
Init sequence 0x23d4 → 0x2420..0x2470:
1. pins/reset (above); framebuffer clear (0x2338) and push (0x23a8)
2. `A2` LCD bias 1/9
3. `A0` ADC (segment direction) normal
4. `C0` COM output scan normal
5. `F8 00` booster ratio select = 2x/3x/4x (0x22e8)
6. delay(10) (0x21a0 = n×100 dummy GPIO reads, ≈ms-scale)
7. `40` display start line 0
8. `2C` power control: booster on; delay 50
9. `2E` power control: + regulator; delay 50
10. `2F` power control: + follower; delay 10
11. `20|Rr` V0 regulator resistor ratio, Rr = byte @0x10000015 (default 5); delay 10 (0x2310)
12. `81, EV&0x3F` electronic volume (contrast), EV = byte @0x10000487 (0x22fa)
13. `AF` display on, `A4` normal (not all-on) display (0x22ce)

Contrast UI (0x810, "LCD:%02i - %i"): EV 0..63 and Rr 0..7 can be adjusted live.
"LCD contrast reset" (0x13b2) sets EV = 0 and Rr = 5. Both settings are re-sent by 0x2310.

Power/sleep:
- 0x22a0 power-save: `AC` (static indicator off), `AE` (display off), `A5` (all points on).
  On ST7565 this sequence enters sleep mode. It is called when the calculator turns off
  (0x19ea, 0x1a50).
- Wake: the full init 0x23d4 runs again, including the reset pulse (0x197e, "turned on").
- 0x22de sends `AE` only (display off). 0x22ce sends `AF A4` (display on).
- There is no backlight control anywhere.

## Panel / framebuffer
- Resolution **132 × 16 px**, i.e. 2 controller pages × 132 columns.
- RAM framebuffer at 0x1000037E, 264 bytes = 2 × 132.
- Shadow/saved copy at 0x10000276. Copy functions: 0x2338 / 0x2378.
  Flag 0x10000488 selects whether the saved copy is restored.
- Pixel set 0x2478(x, y), x < 132, y < 16:
  `fb[(y>>3)*132 + x] |= 0x80 >> (y&7)`.
  Fill rectangle 0x24a0(x, y, w, h).
- Flush 0x2228: sends `B1 00 10 E0` then fb[0..131] as data, then `B0 00 10 E0` then fb[132..263].
  - fb row-band 0 goes to controller page 1 and band 1 goes to page 0.
  - Within a byte, MSB = top.
  - In other words the glass is wired with COM inverted: physical row y ↔ COM(15−y).
  - Columns start at 0 and the controller uses all 132 segments.
  - `E0` = read-modify-write mode. It is harmless here.
- Flush throttling: counter at 0x10000486 (0x23a8).

## Fonts and rendering
Font descriptor (8 bytes): `{default_width, height, first_char, last_char, u32 glyph_ptr}`.
Table at 0x8AC4..0x8AF3:

| desc | w×h | chars | data |
|---|---|---|---|
| 0x8AC4 | 13×12 | 0x20-0x5A | 0x83E5 |
| 0x8ACC | 13×12 | 0x20-0x5A | 0x87E0 |
| 0x8AD4 | 3×7 (fixed, 3 bytes/glyph) | 0x20-0x67 | 0x7A1B (small: annunciators, menus) |
| 0x8ADC | 5×8 | 0x20-0xA2 | 0x7AF3 (2-line mode, console menus) |
| 0x8AE4 | 6×11 | 0x20-0x5A | 0x811B |
| 0x8AEC | 6×11 | 0x20-0x5A | 0x7E51 (default 1-line font, messages) |

Glyph encoding (0x24f4):
- Variable-width fonts store each glyph as `[nbytes, width, data...]`. The 3×7 font is
  special-cased by pointer (0x7A1B): 3 bytes per glyph, width 3.
- Bits are a continuous LSB-first stream, column-major. Each column takes `height` bits,
  and bit 0 is the top row. If the data runs out early, the remaining columns are blank.
- The function returns the glyph width.
- String draw 0x25a4(str, x, y, font): each character advances by width + spacing, where
  spacing is the byte at 0x10000018 (default 1). Drawing stops at x > 131.

The calculator display render is at 0x5eb4:
- It has 11 digit cells. The pitch is 12 px in 1-line mode (11 × 12 = 132) and 8 px in 2-line mode.
- Font index 0..6 is at 0x100000A8 ("Cycle font"):
  - indexes ≤ 2 draw vector 7-segment digits with rectangles (0x5d6a: segments 3×1 / 1×2 bars in a 5×9 cell)
  - 4 → 0x8AE4
  - 5 → 0x8ACC
  - 6 → 0x8AC4
  - other values → 0x8AEC
- Decimal point / comma (0x5d2c): the point is a 1×2 bar at (x+6, y+6). The comma adds
  pixels at (x+6, y+7) and (x+5, y+8). Byte 0x10000ED7 selects dot or comma.
- 0x10000ED5 is the 2-line mode flag: the 5×8 font with 8 px pitch.
- 0x10000ED6 is "annunciator position" (top/bottom). 0x5dfc returns the digit y-offset:
  7 in 2-line mode, 6 when annunciators are on top, else 0.
- Annunciators (USER, f, g, BEGIN, D.MY, PRGM … strings around 0xAC5D/0xAC80) are text in the
  small 3×7 font in the remaining rows, not dedicated segments.
- Message helper 0x730(x, line1, line2): line 1 uses the 6×11 font at y = 0.
  Line 2 uses the 3×7 font at (1, 9).

## Misc
- "LCD debug" (console, 0x20e4) dumps the framebuffer as text over UART.
- `0x768` is a ROM IAP call 57 (Reinvoke ISP / bootloader). This is not display-related.

---
# DM16L (LPC111x) – Keyboard & Power Management

All addresses refer to the flat firmware image (load address 0). The GPIO helpers used throughout are:
- `0x2f24(port,pin,v)` sets GPIOnDIR (1 = output). Port 0 is taken for any value other than 1..3.
- `0x2fbc(port,pin,v)` writes the GPIOnDATA bit (via the 0x3FFC mask address).
- `0x2f68(port,pin)` reads the GPIOnDATA bit.
- `0x3000(iocon_reg, mode)` replaces IOCON MODE bits [4:3]: 0x00 = no pull, 0x08 = pull-down, 0x10 = pull-up.

## 1. Key matrix (10 drive lines x 4 sense lines + ON)

Tables in flash:
- 0x8af4: IOCON offsets of the 10 drive lines (used for the pull-up on/off switching).
- 0x8afe: 40 key codes.
- 0x8b26: 4 sense lines as (port, pin) byte pairs.
- 0x8b2e: 10 drive lines as (port, pin) byte pairs.

| Drive line ("column") | port.pin | IOCON |
|---|---|---|
| C0 | PIO2_7 | 0x020 |
| C1 | PIO2_8 | 0x024 |
| C2 | PIO2_1 | 0x028 |
| C3 | PIO0_3 | 0x02C |
| C4 | PIO1_9 | 0x038 |
| C5 | PIO3_4 | 0x03C |
| C6 | PIO2_4 | 0x040 |
| C7 | PIO2_5 | 0x044 |
| C8 | PIO3_5 | 0x048 |
| C9 | PIO0_6 | 0x04C |

| Sense line ("row") | port.pin | physical row | start-logic bit |
|---|---|---|---|
| R1 | PIO0_7 | top row (A..÷) | 7 |
| R2 | PIO0_8 | 2nd row (GSB..×) | 8 |
| R3 | PIO0_10 | 3rd row (R/S..−) | 10 |
| R4 | PIO1_0 | bottom row (ON..+) | 12 |
| ON | PIO0_11 | ON key (dedicated) | 11 |

All sense lines are wake-capable PIO0_x/PIO1_0 start-logic pins. This is deliberate: any key can wake the chip from deep-sleep.

Note: the firmware never writes IOCON for PIO0_10 (default function SWCLK) or PIO1_0 (default function R/TMS). It reads them through GPIO DATA and relies on the reset-default pull-ups. A new firmware should explicitly set these pins to GPIO function with pull-up. This also means SWD clock and keyboard row 3 share a pin.

### Scan algorithm (`0x2744`, `scan(&k1,&k2)`)
1. `0x2668` (once, guarded by flag 0x1000048c) sets all 10 drive lines to input with DATA = 1 and enables their IOCON pull-ups (`0x2644(0x10)`). Idle state is therefore all lines high-Z with pull-ups.
2. `0x26e8(1)` handles ON. It drives C0/PIO2_7 as output low, reads PIO0_11 (low = pressed, read up to twice), then returns PIO2_7 to input/high. If ON is pressed, the first slot is set to code 0x18.
3. The column loop runs C0..C9:
   - Set the column as output and drive it low.
   - Read the sense lines in the order R4 (PIO1_0), R3, R2, R1.
   - A key counts as pressed if any of up to 3 consecutive reads is low. This is a glitch filter, not a time-based debounce. There is no settling delay.
   - Restore the column to input with DATA = 1.
4. At most 2 keys are reported (k1, k2), with ON counting as one. This is enough for the `ON`+key setup combos.
- Key code = `table[0x8afe + col*4 + j]`, where j = 0..3 for R4, R3, R2, R1.
- Scanning is **polled** from the main loop (`0x34c` → `0x2744`, wrapped by `0x3d8`). There is no timer IRQ for keys. Pin interrupts (start logic) are used **only for wake-up from deep sleep**.
- `0x3d8` also injects keys queued by the serial-console "Key press" command (globals 0x10000268/26c/270).
- `0x34c` does an undriven read of PIO0_11 first (`0x26e8(0)`). If that line is low without any column driven, it calls `0x3f48`, which reads I²C RTC status bits (`0x2b30`, mask 0x0C) and resets the idle counter. Hypothesis: the RTC's open-drain INT output is wired-OR onto PIO0_11, so an RTC alarm can wake the unit just like ON.

### Key codes → physical keys (HP Nut/Voyager key codes)
Each cell shows the hex key code and the key label.

| Row line | C0 | C1 | C2 | C3 | C4 | C5 | C6 | C7 | C8 | C9 |
|---|---|---|---|---|---|---|---|---|---|---|
| R1 PIO0_7 | 13 A | 33 B | 73 C | C3 D | 83 E | 82 F | C2 7 | 72 8 | 32 9 | 12 ÷ |
| R2 PIO0_8 | 10 GSB | 30 GTO | 70 HEX | C0 DEC | 80 OCT | 87 BIN | C7 4 | 77 5 | 37 6 | 17 × |
| R3 PIO0_10 | 11 R/S | 31 SST | 71 R↓ | C1 x≷y | 81 ← | 84 ENTER | C4 1 | 74 2 | 34 3 | 14 − |
| R4 PIO1_0 | 18 ON | 38 f | 78 g | C8 STO | 88 RCL | 84 ENTER | C5 0 | 75 . | 35 CHS | 15 + |

Evidence for the column order:
- C5 has the same code (0x84) in R3 and R4. This is the tall ENTER key, which sits in physical column 6.
- 0x18 at C0/R4 matches the ON position, and 0x18 is the code the firmware assigns to ON (`0x275c`).

The column order is therefore physical left to right. ON is decoded both as matrix C0×R4 (PIO2_7×PIO1_0) and as PIO2_7-low → PIO0_11. Only PIO0_11 is enabled for wake-up when the unit is off, so the real ON switch is almost certainly on PIO0_11. The 0x18 at C0/R4 is likely a leftover or alternate wiring.

## 2. Pin setup at boot (main init `0x1b60`)
- PIO1_11 (IOCON 0x98) and PIO3_2 (0x9C): pull disabled. PIO1_11 is AD7, the battery ADC pin; see `0x2854`, ADC analysis is out of scope.
- **PIO0_11** (IOCON 0x74): FUNC = 1 (GPIO), input, **pull-up**. This is the ON line.
- PIO1_4 (IOCON 0x94): FUNC = 0, input, no pull. Purpose unknown; possibly VBUS/charger sense.
- PIO0_1 (IOCON 0x10) is set to pull-down later (`0x1c56`). This is the ISP-entry pin.

## 3. Sleep / power-down

### Clocks and tick
- SysTick ISR `0x335c` increments the 1 ms tick 0x100005c8 and the idle counter 0x100005c0.
- `0x33b8(ms)` is a busy-wait delay on that tick.
- `0x3388` programs SysTick.

### Power modes
The LPC111x deep-power-down mode is never used. Only deep-sleep is used, and only the PMU "sleep" mode (0x3064) is used in the console.
- `0x3064`: PMU PCON (0x40038000) DPDEN cleared, SCR.SLEEPDEEP cleared, PDAWAKECFG = PDRUNCFG, `WFI`. This is plain sleep, used by the serial-console loop `0x59c` together with a 100 ms SysTick.
- `0x3180(units, wakemask)` (via the wrapper `0x3354`) enters **deep-sleep**:
  1. `cpsid i`, then `0x30a0` prepares the pins:
     - `0x3528`: disable the I²C IRQ, clear I2CONCLR, set PIO0_4/PIO0_5 (SCL/SDA) to GPIO output high, set PRESETCTRL = 0 (I²C held in reset).
     - `0x26ac`: **all 10 columns become outputs driven LOW**, and their pull-ups are switched off. Any key press then pulls its row low, which is the wake source.
     - Pull resistors are switched off on PIO2_0, PIO2_6, PIO3_3, PIO1_8 and PIO0_2. These are probably LCD/other lines; they are not part of the keyboard.
  2. PDAWAKECFG (0x234) = PDRUNCFG (0x238).
  3. PDSLEEPCFG (0x230): **0x18FF** for wake on pins only (everything off: IRC, flash, BOD, ADC, SYSOSC, WDTOSC, PLL). **0x18BF** (WDT oscillator kept on) when a timed wake is requested.
  4. SCR.SLEEPDEEP = 1.
  5. Wake sources:
     - STARTAPRP0 bits 7, 8, 9, 11, 12 are cleared, so these pins wake on a falling edge.
     - `wakemask & 2` ("any key"): STARTERP0 |= 0x1D80 (PIO0_7, PIO0_8, PIO0_10, PIO0_11, PIO1_0). NVIC IRQs 7, 8, 10, 11 and 12 are enabled.
     - `wakemask == 1` ("ON only"): STARTERP0 |= 0x800 (PIO0_11 only).
  6. Timed wake (`units` ≠ 0) uses the classic LPC111x trick of a timer match pin feeding the start logic:
     - All four timers are stopped.
     - PIO0_9 is set to IOCON FUNC 2 (CT16B0_MAT1).
     - CT16B0 MR0 = units·7812/10, MCR = 3, EMR = MAT1 set-high on match.
     - Start logic bit 9 is enabled on a rising edge and NVIC IRQ9 is enabled.
     - The main clock is switched to the WDT oscillator (MAINCLKSEL = 2, AHB divider 1). WDTOSCCTRL = 0x3F, set in `0x3028`, gives about 0.6 MHz/64 ≈ 9.4 kHz, so 1 unit ≈ 0.08–0.1 s.
     - STARTRSRP0CLR = all ones, `cpsie i`, `WFI`.
- Start-logic ISR `0x315c` (IRQ 0–12):
  - increments wake counter 0x100005b8
  - restores the system clock (`0x2bb8`, value in 0x10000020; this also selects CPU speed)
  - calls `0x30f4`, which disables NVIC IRQs 7–12, sets STARTERP0 = 0, clears start logic, stops CT16B0, and runs `0x254(1)` to re-init I²C.
- After `WFI`, if the counter did not change (woken by something else), the flag 0x100005b4 is set to 1.

### Policy in the main loop (`0x190c`)
- **Calculator ON and idle**: `sleep(6, 3)`. The chip deep-sleeps and wakes on any key or after about 0.5 s (6 units).
  - Each wake without a key and without a running program subtracts 500 from a counter preset to 0x2BF20 (180 000).
  - **Auto-off after 360 idle wakes ≈ 180 s, nominally 3 minutes** (`0x1a28`–`0x1a5e`). The state then becomes off (r4 = 2).
  - While a program is running (emulator flag r5+0x98), the loop does not sleep and instead executes emulator slices.
- **Calculator OFF** (emulated display off, or auto-off): `sleep(0, 1)` is deep-sleep with **only the ON line (PIO0_11) as a wake source**. There is no timer.
  - After waking, the unit turns on only if ON (0x18) is read (`0x1964`–`0x198a`, console message "turned on"). Otherwise it goes back to sleep.
- Other waits use the same primitive:
  - `0x552`/`0x1488`: menu "press a key" waits, `sleep(6, 3)` loops with a timeout of 0x77 iterations ≈ 60 s.
  - `0x488`: 12 × `sleep(1, 3)`.
  - `0x318`: sleep until key.
- RAM stays powered in deep-sleep, so calculator state survives without flash writes.

### Out of scope, noticed in passing
- CT32B1 ISR `0x3628` toggles PIO1_1/PIO1_2 in antiphase, which looks like a differential piezo beeper.
- `0x768` "Start bootloader" calls IAP command 57 (re-invoke ISP) through 0x1FFF1FF1.

---
# DM16L firmware: I2C bus, RTC, persistent storage

## I2C peripheral
- The driver is a variant of the NXP LPC11xx sample `i2c.c`, interrupt-driven. ISR at 0x00C0 (vector IRQ15) is a state machine on I2STAT values 0x08/0x10/0x18/0x20/0x28/0x30/0x38/0x40/0x48/0x50/0x58.
- `I2CInit(mode)` at 0x0254:
  - SYSCON PRESETCTRL |= 2 (I2C out of reset), SYSAHBCLKCTRL |= 0x20 (I2C clock on).
  - IOCON 0x30 (PIO0_4) and 0x34 (PIO0_5) set to FUNC=1, i.e. **SCL = PIO0_4, SDA = PIO0_5**. The I2CMODE bits (9:8) are cleared, so the pads run in standard/fast-mode I2C.
  - I2CONCLR = 0x6C, then SCLH = SCLL = SystemCoreClock(0x10000498) / 200000, which gives **100 kHz** at any CPU clock.
  - mode==2 (slave, ADR0=0xA0) is dead code: every caller (0x1C02, 0x30EC, 0x3D0C) passes 1, i.e. master.
  - NVIC enables IRQ15, then I2CONSET = I2EN.
- Transfer engine `I2CEngine` at 0x359C:
  - It sets STA and busy-waits on a timeout of 0xFFF plus a ms-tick (0x33EC). The result 0x101 means OK.
  - The tx buffer is at 0x100005D4, the rx buffer at 0x100005EC, the write length at 0x100005FC and the read length at 0x100005E8.
  - On a timeout it calls 0x3504, which resets the I2C engine.
  - Wrappers retry 3 times: read at 0x29F4, write at 0x2A14.
- Before sleep, `I2C_off` at 0x3528:
  - disables IRQ15 and clears CON;
  - switches PIO0_4/PIO0_5 back to GPIO, driven as **outputs HIGH** so the RTC sees an idle bus;
  - writes PRESETCTRL = 0.
  - It is called from the power-down prep routine at 0x30A0.

## I2C devices
- **Only one slave is used: 7-bit address 0x51 (0xA2 write / 0xA3 read)**. No other address byte is written into the tx buffer; the only writers are at 0x29A0/0x29CC.
- **The LCD is NOT on I2C.**
- The chip is an **NXP PCF8563 (or a BM8563 clone)**. Evidence:
  - the address;
  - time is read as 7 bytes from register 2, with masks 7F 7F 3F 3F 07 1F (0x3FE0);
  - VL is in seconds bit 7;
  - alarms are at 0x09–0x0C, with bit 7 = AE_n;
  - the timer is at 0x0E/0x0F;
  - STOP is bit 5 of register 0.

### RTC register usage
| Reg | Use | Evidence |
|---|---|---|
| 0x00 ctrl_status1 | Writes 0x20 (STOP) before setting the time and 0x00 after. The set sequence is 0x4180: STOP, write the time, wait 300 ms, un-STOP. | 0x416E/0x4172 |
| 0x01 ctrl_status2 | `RTC_init` (0x2B30) reads it. A successful read sets the RTC-present flag (0x10000494 = 1). If TF/AF/TIE/AIE are set it writes back `val & 3`, which clears TF/AF. It is later written as `0x0C \| AIE<<1 \| TIE` (0x3E40), and polled for TF/AF at 0x3F28/0x3F48. | 0x2B30, 0x3E40, 0x3F28 |
| 0x02–0x08 time/date (BCD) | Read with 0x2ACC (7 bytes) and decoded at 0x3FE0. Year = RAM century base (0x100006AC, default 2000) + BCD year. Written by 0x4100 (BCD conversion at 0x3E0A, 7 bytes from register 2). | 0x2ACC, 0x3FE0, 0x4100 |
| 0x02 seconds | Read alone by 0x3EA8, which is used for 1 s tick detection (0x41B4) and for benchmark/time sync. | 0x3EA8 |
| 0x09–0x0C alarm | Written as 80 80 80 80 (all alarms disabled) at boot. | 0x2BA0 |
| 0x0E/0x0F timer | Written as 0x82, 0x01: timer enabled, source 1 Hz, count 1, so **TF fires every 1 s** while enabled. | 0x3E24 |

- Boot sequence (0x2B74, called from 0x1B9E, 0x1C06 and 0x3D10 after wake):
  1. Init.
  2. Read the seconds register. If VL (bit 7) is set, meaning the clock lost power, set the default time 00 00 08 01 02 01 14, i.e. 2014-01-01 08:00:00 (weekday code 2).
  3. Disable the alarms.
- "RTC READ FAIL" (0x72EA) is printed when the 7-byte read at 0x2ACC fails.
- The menu items "RTC ±1 hour" and the console command "Update time <offset_in_minutes>" use read → add offset → set (0x41B4 / 0x4180). The adjust syncs to a seconds rollover before writing.
- RTC /INT pin, **likely PIO0_11**:
  - 0x26E8(0) reads PIO0_11 (low = active) *without* driving any keyboard row. If it is low, 0x3F64 polls the RTC flags.
  - 0x26E8(1) first drives PIO2_7 low, so it reads the ON key in the matrix.
  - So the RTC open-drain INT appears to be wired-OR onto the PIO0_11 key line.
  - PIO0_11 is also a deep-sleep start-logic wake pin (falling edge, STARTERP0 bit 11, set at 0x31D2/0x32D8).
  - The keyboard/display analysis should confirm this.

## Persistent storage
- **The firmware never writes flash.** The only IAP call (entry 0x1FFF1FF1, literal at 0x7D8) is at 0x0768. It issues command **57 (0x39) = Reinvoke ISP** ("READY FOR FLASHING" / "Start bootloader"). Before that it:
  - switches the main clock to the IRC (MAINCLKSEL = 0, AHB div 1);
  - enables the IOCON clock;
  - powers down the PLL (PDRUNCFG |= 0x80).
  - The bootloader is the LPC ROM UART ISP.
- No other path uses IAP: no other 0x1FFF1FFx literal, and no prepare (50), copy (51) or erase (52) sequences exist.
- **Calculator state, programs and settings (LCD contrast, etc.) live only in SRAM, which is retained across "off".** "Off" is Cortex-M0 **deep-sleep**, not deep power-down (routine 0x3180):
  - SCR.SLEEPDEEP = 1;
  - PCON.DPDEN is not set (0x3064);
  - PDSLEEPCFG = 0x18FF (all analog off), or 0x18BF (WDT oscillator kept on) for a timed wake;
  - PDAWAKECFG = PDRUNCFG;
  - wake pins are on the start logic.
  - For a timed wake (power fork):
    1. The main clock is switched to the WDT oscillator, which was configured at 0x3028 to WDTOSCCTRL = 0x3F, about 9.4 kHz.
    2. CT16B0 toggles MAT1 on PIO0_9.
    3. That edge feeds start logic bit 9 and wakes the chip.
- Consequences:
  - A battery removal or "Power reset" loses everything.
  - The RTC keeps time only if it has its own supply (the VL check handles the loss case).
  - Console "Dump memory" / "Load memory" over UART is the only backup path.

---
# DM16L firmware V34 – UART/USB, battery ADC, clocks, timers, bootloader

MCU: NXP LPC111x (IOCON layout and IRQ map match LPC1114/1115; IAP entry 0x1FFF1FF1). No USB peripheral is referenced anywhere (no 0x4008xxxx / USB IRQ use).

## UART / "USB"
- Init `uart_init(baud)` @0x37E8, called at boot @0x1BE8 and @0x3D06 with 0x9600 = **38400 baud**.
  - IOCON+0xA4 (PIO1_6) FUNC=1 → **RXD**; IOCON+0xA8 (PIO1_7) FUNC=1 → **TXD** (0x380A–0x3826).
  - SYSAHBCLKCTRL bit12 (UART) on; UARTCLKDIV (SYSCON+0x98)=1.
  - DL = SystemCoreClock(0x10000498) * SYSAHBCLKDIV / UARTCLKDIV / 16 / baud; FDR=0x10 (no fraction); LCR=3 (8N1); FCR=7; IER=5 (RBR+RLS); NVIC IRQ21.
  - 12 MHz → DL=19 (≈39474 baud, +2.8 %); 48 MHz → DL=78 (≈38462 baud, +0.16 %).
- RX ISR @0x3770 → 128-byte ring buffer (struct @0x1000061C); `uart_rx_avail` @0x391C, `uart_getc` @0x38D8.
- TX polled: `uart_putc` @0x300 (waits for LSR.THRE, writes THR). printf-style output @0x3BBA.
- No modem/handshake lines and no VBUS/USB-detect GPIO were found. So the USB socket must go to an external USB-UART bridge on PIO1_6/PIO1_7 (these are also the LPC ISP UART pins). The console only starts when a character arrives.

## Serial console ("SERIAL CONSOLE", prompt "VOYAGER >> ")
- Entry @0x59C. Ways to enter: the setup menu "Serial console" (0x1384), or any UART byte during the ~0.5 s boot window (loop @0x1C42–0x1C52, which calls 0x59C(0)). While the console runs, the CPU drops to 12 MHz (0x5B4) and sleeps with WFI between bytes (0x62C → 0x3064). A timeout counter defaults to 600 ms ticks; "ct" changes it.
- Command table @0x8CC0, 12 entries × 20 bytes {name, nargs, handler, help, params}:
  `?` help · `p <key1> <key2>…` inject key presses · `s` dump memory · `l` load memory · `ct <minutes>` console timeout · `bootloader` (must be said 3 times) · `t` get time · `ts YYYYMMDD HHMMSS` set time · `td <offset_min>` adjust time · `kb` toggle key echo · `d` toggle display echo · `b` battery ("BAT: %imV").
- `s` dump (0x1E8C): prints "DM16", then one line `%02x  <14 hex nibbles>` per non-zero emulated register (256 × 7-byte registers at 0x10000754). `l` load (0x1F30): prints "Waiting for data...". The first line must start with "DM16", otherwise it prints "Bad model". After that come lines `A:`/`B:`/`C:`/`S:`/`M:`/`N:` (CPU registers), `G:` and `xx <hex>` register lines. It answers "Read OK"/"Read FAILED". This is calculator state, not hardware.

## Bootloader (LPC ROM ISP)
- `enter_isp` @0x768: shows "READY FOR FLASHING" / "RESET TO ABORT" and waits 100 ms. It then disables IRQs, switches MAINCLKSEL to IRC (12 MHz), sets SYSAHBCLKDIV=1, enables the IOCON clock, powers down the PLL (PDRUNCFG bit7), and calls IAP command **57 (Reinvoke ISP)** via 0x1FFF1FF1. ISP then autobauds on UART PIO1_6/7, so the flashing tool talks through the same USB-UART bridge.
- Callers: the setup menu "Start bootloader?" (ENTER confirms) @0x12BE, and the console `bootloader` command @0x2150. The counter at 0x10000014 starts at 3 ("Say bootloader two/one more time") and is reset to 3 by any other command (0x2E48).
- Main init sets a pull-down on PIO0_1 (the hardware ISP-entry pin) via IOCON+0x10 MODE=8 @0x1C56.

## Battery measurement
- `adc_read(en)` @0x2854: powers up the ADC (PDRUNCFG bit4), enables its clock (bit13), and makes PIO1_11 an input with IOCON+0x98 in analog mode = **AD7**. ADC clock = core/(core/4 MHz) → ≤4 MHz. It sets **PIO3_2 = en**, waits 10 ms (en=0) or 1 ms (en=1), runs a software START with SEL=ch7, and polls AD7DR (ADC+0x2C) DONE. The result is 10 bits. Afterwards (0x2828) PIO1_11 and PIO3_2 go back to output low (discharge / no quiescent current).
- Reading: PIO3_2 high powers a **fixed voltage reference (≈1.234 V)** on AD7, and the ADC reference is VDD = the battery. So **Vbat_mV = 1 263 405 / ADC** (0x294C; 1263405/1024 = 1233.8 mV).
- Hardware presence check @0x2910, cached in 0x1000001C: with en=0 the reading must be ≤10, and with en=1 it must be 320…768 (≈1.65–3.95 V). Otherwise battery measurement is treated as absent.
- Low-battery flag @0x2978 → 0x10000490 = (Vbat ≤ **2100 mV**). It is called from the main loop @0x1AB6. "System info" shows "%i.%03iV".

## Clocks
- Boot (0x2C88): sets all GPIO DIR low, SYSPLLCLKSEL = IRC (SYSCON+0x40=0, UEN toggled), and **SYSPLLCTRL = 0x23** (M=4, P=2 → 48 MHz from the 12 MHz IRC, FCCO=192 MHz). It then calls `set_speed(0)`.
- `set_speed(mode)` @0x2BB8. mode≠0: power the PLL, wait for lock, MAINCLKSEL=3 (PLL out) → **48 MHz**. mode=0: MAINCLKSEL=0 (IRC) and PLL powered down → **12 MHz**. The setting goes to 0x10000020. The routine then recomputes SystemCoreClock (0x10000498), clk/1000×30 (0x100004A0), clk/6400 (0x1000049C) and re-inits SysTick and the UART (0x3CF0).
- "Toggle CPU speed" @0x138A: mode = 1 − current. The boot default is 12 MHz.
- Deep sleep @0x3180: main clock = WDT osc (WDTOSCCTRL=0x3F ≈ 9.4 kHz, 0x3028) and SLEEPDEEP. PDSLEEPCFG = 0x18FF, or 0x18BF when a timed wake is needed (WDT osc kept on). Wake sources are start-logic pins (STARTERP0 bits 7,8,10,11,12 – keyboard, see the keyboard analysis) and, for a timed wake, **CT16B0 MAT1 on PIO0_9** (IOCON+0x64 FUNC=2) feeding start-logic input 9 (MR1 = 7812·ms/1000). The start-logic ISR @0x315C restores the speed mode.

## Timers
- SysTick: 1 ms tick (reload clk/1000, 0x3388); ISR 0x335C increments the ms counter 0x100005C8. `delay_ms` @0x33B8.
- CT16B0: deep-sleep timed wake via MAT1→PIO0_9 (above). ISR 0x33F8 counts.
- CT16B1: MR0 = clk/6400 → **6.4 kHz** periodic IRQ (0x440/0x3488). ISR 0x3410 counts into 0x100005D0, which the emulator uses as its cycle-pacing reference.
- CT32B0: MR0 = clk×30/1000 → **30 ms** periodic IRQ (0x44E). ISR 0x3610 counts into 0x10000614, which the main loop uses (passed with 0x100005D0 to the emulator scheduler @0x1C76).
- CT32B1: **piezo beeper**. `beep(rate, count)` @0x43D0 sets PIO1_1 (IOCON+0x7C) and PIO1_2 (IOCON+0x80) as GPIO outputs, with MR0 = clk/rate. The ISR @0x3628 toggles PIO1_1 and PIO1_2 in antiphase (push-pull drive of a piezo), `count` times. The pins are then left high and switched to inputs (0x439C). At boot it plays 2× beep(990, 40) → about a 495 Hz tone, ≈40 ms each (0x1C26–0x1C38).

## System info (0x694)
Shows "DM16 V34 <MHz>MHZ", then "x.yyyV" battery (if the reference circuit is present), then "RTC" if the I2C RTC (address 0xA2, PCF8563-style, flag 0x10000494) answered. The Benchmark item (menu dispatch around 0x1340–0x1380) was not analysed (I assume it only exercises the emulator; not verified).
