# Requirements

Requirements for calcu16-fw, the Calc-U-16 firmware for the SwissMicros DM16L.
This is a living document: assumptions become requirements once we have tried
them out.

## Calculator model

- **R-1 No RPN.** The calculator will not use RPN and needs no stack. The HP
  layout of two display lines for one stack level does not apply.
- **R-1a Algebraic entry in strict order of entry** (no operator precedence):
  `5 + 4 × 3 ENTER` gives 27, and pressing `×` already shows the intermediate 9.
  ENTER works as `=`.
- **R-2 Word sizes.** 8, 16, 32 and 64 bit, plus a custom word size of up to 64 bits.
  Set with WSIZE (f STO) as a **postfix** key: `32 WSIZE`. X is the size; `0`
  means 64 (as on the HP-16C, the way out of word sizes too small to type a larger
  size in); above 64 is Error 2. The number shown before the size was typed comes
  back, truncated to the new size.

## Display

The glass is 132 × 16 pixels (about 69 × 9 mm), with no fixed segments. Everything,
including indicators, is drawn in pixels (see `hardware.md`).

- **R-3 HEX, DEC and OCT fit on the display.** A number in these bases is shown in
  full, without scrolling, for every word size.
- **R-4 Main screen: two lines.** The **lower line is the entry line**: the value in
  the base selected with the DEC/HEX/OCT keys. The **upper line shows DEC**, or **HEX**
  when the entry base is DEC. One-line layouts are ruled out.
- **R-5 Base marking** is only a small lowercase suffix after each value (h d o b),
  bottom-aligned with the digits. No other base label anywhere.
- **R-6 The entry line is never compromised by indicators.** It may shrink for its
  own length: grouping is dropped first, then the font goes from 5×7 to 4×7. This can
  happen while typing (redrawn smaller as soon as the next digit no longer fits).
  If space is needed for indicators, the **upper line** gives way (narrower font).
- **R-7 Indicators on the main screen** are limited to carry (C) and overflow (V), at
  the left of the upper line. No word size or sign mode on the main screen.
- **R-8 Battery warning** is a 2×2 dot in the top-left corner, lit only when the
  battery is low. It is rarely shown and is treated like an indicator (R-6).
- **R-9 Status display.** While a status key (e.g. R/S) is held, the display shows
  the status instead of the numbers; on release it returns to the number screen.
  Compact from the start, to leave room for more indicators later:
  - values in plain 3×5: word size ("64"), sign mode (2C/1C/UN), battery voltage
    (inverse when low)
  - indicators (HP "flags") as small inverse 3×5 chips, shown only when on. As long
    as all indicators fit, **each has its own fixed slot**, so its position never
    changes (packing the lit ones to the left was tried and rejected)
  - candidate indicators, see below
- **R-10 BIN has a screen of its own**, without DEC or HEX. Real 0/1 digits only
  (simplified forms look strained):
  - up to 16 bits: one line in 5×7, nibble positions numbered above
  - up to 32 bits: two lines of 16 in 5×7
  - up to 64 bits: two lines of 32 in 3×5, every other nibble marked with an overline
  - more than 64 bits (a possible 128-bit mode, see R-14): pages of 64 bits, with a position
    mark in the left margin
- **R-11 Repeating digits** (float mode, if any) could be marked with an underline;
  this is why BIN uses an overline.
- **R-12 All-caps HEX.** Digit 0 is round and letter D square; at 5×7 and 4×7 also
  8 round and B square; at 3×5, B is notched on the right.

### Indicator candidates (from the HP-16C)

The HP-16C handbook (section 3, "Flags", "Machine Status", "Annunciators") has:

| HP-16C | Meaning | Here |
|---|---|---|
| annunciator `*` | low battery, flashing, lower left | battery dot (R-8), voltage on the status display |
| annunciators `f`, `g` | prefix key pending | main screen indicator, like C and V |
| flag 4, annunciator `C` | carry | C |
| flag 5, annunciator `G` | out of range ("greater"), i.e. overflow | V |
| annunciator `PRGM` | program mode | only if we support programs |
| flag 3 | show leading zeros | indicator LZ (status display) |
| flags 0–2 | user flags, for programs | F0–F2 (status display), only with programs |
| STATUS display `2-16-0000` | complement mode, word size, flags 3..0 | the status display |
| period beside the suffix | more digits outside the 8-digit window | not needed: we show all digits |
| Float mode | decimal point, no base suffix | open (float mode) |

Our additions: memory registers in use (M1, M2, ... ≠ 0).

## Safety

- **R-15 The bootloader escapes must never break.** On the tested unit the PGM
  button does not reach the ISP pin, so D held at reset (checked before anything
  else), D + ON and the serial `bootloader` command are the only ways to reflash
  without opening the calculator. Simulator tests guard all three.

## Display architecture

- **R-13 State → display model → renderer.** The calculator state is first turned
  into a display model that says what to show (screen kind, lines with text and
  base, active indicators), with no pixels or fonts. A renderer turns the model into
  pixels, controlled by a few style options (e.g. digit grouping). Prototype:
  `tools/display_lab/lab.py`; firmware: `firmware/calc/src/display.rs`, with fonts
  generated from the lab (`tools/display_lab/gen_fonts.py` → `fonts.rs`).

### Worst-case lengths

Digits needed for the largest value of each word size. Signed DEC includes the
minus sign.

| Word size | HEX | DEC unsigned | DEC signed | OCT | BIN |
|---:|---:|---:|---:|---:|---:|
| 8 | 2 | 3 | 4 | 3 | 8 |
| 16 | 4 | 5 | 6 | 6 | 16 |
| 32 | 8 | 10 | 11 | 11 | 32 |
| 64 | 16 | 20 | 20 | 22 | 64 |

So the line must hold **22 characters** (64-bit OCT), plus any digit-group spacing
and indicators, in 132 pixels. That is a pitch of at most 6 px with no room left,
or 5 px with about 20 px left for indicators.

- **R-14 Possible 128-bit mode.** Word sizes up to 128 bits could be supported at
  a cost: in this mode only, the number being entered or shown uses **both display
  lines**, and there is no second base on the upper line. 128 bits need 32 HEX,
  39 DEC or 43 OCT digits, too many for one line in the regular font.

## To do

- Prototype the 128-bit HEX, DEC and OCT display over both lines (R-14) in the
  display lab. The BIN pages for more than 64 bits already exist.

## Open questions

- Floating-point mode (as on the HP-16C): needed?
- Digit grouping: spacing only, or separators?
- Which key shows the status display (the firmware currently uses R/S).
- Where carry/overflow show on the BIN screen (status display only?).
- Battery dot on the BIN pages with more than 64 bits (the left margin is the position mark).

Display layout experiments: `tools/display_lab/` (`python3 tools/display_lab/lab.py`).
