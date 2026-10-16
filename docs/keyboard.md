# DM16L keyboard: labels and matrix

These are the labels read off `images/dm16l-photo.png`, checked against the *HP-16C Owner's Handbook* (April 1982, `reference/hp16c-owners-handbook.pdf`): the Function Summary on pp. 120–124, Clearing on p. 17 and the self-tests on pp. 106–107.

The DM16L keeps the HP-16C label scheme. Its "yellow" is HP's gold and its "blue" is HP's blue:

| Label | Position on the DM16L | HP-16C equivalent | Selected by |
|---|---|---|---|
| **Base** | large white legend on the key face | primary function | the key alone |
| **Yellow (f)** | printed on the faceplate *above* the key | gold | yellow **f** key, then the key |
| **Blue (g)** | small light-blue/white legend on the *lower front* of the key | blue | light-blue **g** key, then the key |

Grid position is `row-col`, with rows 1–4 top to bottom and columns 1–10 left to right; column 10 is written as `0`. These are also the HP program key codes (p. 77). Digit keys 0–9 and A–F appear in program listings as their value, not as `row-col`, so `7` is listed as `7`, not `17`. The DM16L setup menu uses the same grid, for example `1-3`. The bracket names are covered in "Yellow grouping brackets" below; the table lists only the function name, e.g. PRGM rather than CLEAR PRGM.

Matrix columns: C0 = PIO2_7, C1 = PIO2_8, C2 = PIO2_1, C3 = PIO0_3, C4 = PIO1_9, C5 = PIO3_4, C6 = PIO2_4, C7 = PIO2_5, C8 = PIO3_5, C9 = PIO0_6. Matrix rows: R1 = PIO0_7, R2 = PIO0_8, R3 = PIO0_10, R4 = PIO1_0. The "Nut code" column is the key code the vendor firmware passes to the emulated HP CPU; it comes from the table at 0x8afe (see `hardware.md`).

## Row 1

| Grid | Matrix | Nut code | Base | Yellow (f) | Blue (g) |
|---|---|---|---|---|---|
| 1-1 | C0·R1 | 0x13 | A | SL | LJ |
| 1-2 | C1·R1 | 0x33 | B | SR | ASR |
| 1-3 | C2·R1 | 0x73 | C | RL | RLC |
| 1-4 | C3·R1 | 0xC3 | D | RR | RRC |
| 1-5 | C4·R1 | 0x83 | E | RLn | RLCn |
| 1-6 | C5·R1 | 0x82 | F | RRn | RRCn |
| 1-7 | C6·R1 | 0xC2 | 7 | MASKL | #B |
| 1-8 | C7·R1 | 0x72 | 8 | MASKR | ABS |
| 1-9 | C8·R1 | 0x32 | 9 | RMD | DBLR |
| 1-0 | C9·R1 | 0x12 | ÷ | XOR | DBL÷ |

## Row 2

| Grid | Matrix | Nut code | Base | Yellow (f) | Blue (g) |
|---|---|---|---|---|---|
| 2-1 | C0·R2 | 0x10 | GSB | x≷(i) | RTN |
| 2-2 | C1·R2 | 0x30 | GTO | x≷I | LBL |
| 2-3 | C2·R2 | 0x70 | HEX | *(SHOW)* HEX | DSZ |
| 2-4 | C3·R2 | 0xC0 | DEC | *(SHOW)* DEC | ISZ |
| 2-5 | C4·R2 | 0x80 | OCT | *(SHOW)* OCT | √x |
| 2-6 | C5·R2 | 0x87 | BIN | *(SHOW)* BIN | 1/x |
| 2-7 | C6·R2 | 0xC7 | 4 | SB | SF |
| 2-8 | C7·R2 | 0x77 | 5 | CB | CF |
| 2-9 | C8·R2 | 0x37 | 6 | B? | F? |
| 2-0 | C9·R2 | 0x17 | × | AND | DBL× |

## Row 3

| Grid | Matrix | Nut code | Base | Yellow (f) | Blue (g) |
|---|---|---|---|---|---|
| 3-1 | C0·R3 | 0x11 | R/S | (i) | P/R |
| 3-2 | C1·R3 | 0x31 | SST | I | BST |
| 3-3 | C2·R3 | 0x71 | R↓ | *(CLEAR)* PRGM | R↑ |
| 3-4 | C3·R3 | 0xC1 | x≷y | *(CLEAR)* REG | PSE |
| 3-5 | C4·R3 | 0x81 | ← (BSP, backspace) | *(CLEAR)* PREFIX | CLx |
| 3-6 | C5·R3 | 0x84 | ENTER (upper half) | WINDOW | LSTx |
| 3-7 | C6·R3 | 0xC4 | 1 | *(SET COMPL)* 1'S | x≤y |
| 3-8 | C7·R3 | 0x74 | 2 | *(SET COMPL)* 2'S | x<0 |
| 3-9 | C8·R3 | 0x34 | 3 | *(SET COMPL)* UNSGN | x>y |
| 3-0 | C9·R3 | 0x14 | − | NOT | x>0 |

## Row 4

| Grid | Matrix | Nut code | Base | Yellow (f) | Blue (g) |
|---|---|---|---|---|---|
| 4-1 | C0·R4 and the separate ON line PIO0_11 | 0x18 | ON | — | — |
| 4-2 | C1·R4 | 0x38 | **f** (yellow key) | — | — |
| 4-3 | C2·R4 | 0x78 | **g** (light-blue key) | — | — |
| 4-4 | C3·R4 | 0xC8 | STO | WSIZE | < (scroll left) |
| 4-5 | C4·R4 | 0x88 | RCL | FLOAT | > (scroll right) |
| (3-6) | C5·R4 | 0x84 | ENTER (lower half) | — | — |
| 4-7 | C6·R4 | 0xC5 | 0 | MEM | x≠y |
| 4-8 | C7·R4 | 0x75 | . | STATUS | x≠0 |
| 4-9 | C8·R4 | 0x35 | CHS | EEX | x=y |
| 4-0 | C9·R4 | 0x15 | + | OR | x=0 |

ENTER is one tall key that closes both C5·R3 and C5·R4; the firmware gives both positions the same code. The HP grid code is 36, so the key in grid position 4-6 does not exist.

## Yellow grouping brackets

A bracket puts its word in front of each yellow label underneath it, so the yellow label on its own does not name the function. The handbook writes these as "f CLEAR PRGM", "f SET COMPL 2'S" and "f SHOW HEX" (footnote on p. 16).

| Bracket | Keys covered | Functions | Handbook meaning |
|---|---|---|---|
| **SHOW** | 2-3 HEX, 2-4 DEC, 2-5 OCT, 2-6 BIN | SHOW HEX / DEC / OCT / BIN | Shows X in another base temporarily (p. 29) |
| **CLEAR** | 3-3 R↓, 3-4 x≷y, 3-5 ← | CLEAR PRGM / REG / PREFIX | Clears program memory / all registers / a pending prefix (p. 17) |
| **SET COMPL** | 3-7 1, 3-8 2, 3-9 3 | SET COMPL 1'S / 2'S / UNSGN | Sets 1's or 2's complement, or unsigned mode (p. 30) |

On the faceplate:
- **SHOW:** one bracket line runs over all four keys (HEX to BIN), with the word in its middle.
- **CLEAR** and **SET COMPL:** each has two short brackets over the outer keys (PRGM and PREFIX; 1'S and UNSGN). The word sits stacked above the middle key's yellow label ("CLEAR" over REG, "SET COMPL" over 2'S).

## Notes for new firmware

- **Prefix keys:**
  - f (4-2) and g (4-3) are the shift keys.
  - STO, RCL, GTO, GSB, LBL, FLOAT, WINDOW and the bit and flag functions also expect a following digit or register.
  - CLEAR PREFIX (f + ←) cancels any pending prefix.
- **Key combinations with ON** (handbook pp. 20, 61, 106–107): hold ON, press the second key, then release ON first.

  | Combination | HP-16C | DM16L firmware |
  |---|---|---|
  | ON + × | self test | Self test |
  | ON + + | endless self test | Endless test |
  | ON + ÷ | keyboard test | Keyboard test |
  | ON + − | reset Continuous Memory | Power reset |
  | ON + . | swap digit separator (dot/comma) | Toggle dot/comma |
  | ON + D | wakes a calculator that does not respond to keys (p. 106) | — |

  The DM16L setup menu items sit on the same grid positions (`hardware.md`, manual table).
- **Keys with no f or g function:** ON, f, g and the lower half of ENTER.
