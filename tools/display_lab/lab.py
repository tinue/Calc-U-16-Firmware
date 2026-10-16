#!/usr/bin/env python3
"""Display layout lab: prototype of the display pipeline, rendered with the
simulator's glass renderer.

Three layers, as planned for the firmware:
  1. State          what the calculator knows (value, word size, base, flags)
  2. display model  what to show: screen kind, lines with text and base, indicators
  3. renderer       model + Style options -> 132x16 pixels (fonts, fitting, placement)

    python3 tools/display_lab/lab.py            # writes tools/display_lab/out/
    open tools/display_lab/out/index.html

Needs the simulator built: (cd sim && cargo build --release).
Screens are text art (one '#'/'.' row per pixel row), the format `voyager-sim render` reads.
"""
from __future__ import annotations

import html
from dataclasses import dataclass, field
import pathlib
import subprocess
import sys

W, H = 132, 16
HERE = pathlib.Path(__file__).resolve().parent
SIM = HERE.parent.parent / "sim" / "target" / "release" / "voyager-sim"

# --- fonts: dict char -> list of row strings ('#' = on) --------------------


def font(spec):
    """Parse 'c: row row row ...' lines into a glyph dict."""
    out = {}
    for line in spec.strip().splitlines():
        line = line.strip()
        out[line[0]] = line[2:].split()
    return out


# 3x5: smallest legible size, for indicators and secondary values.
F3 = font("""
0: ### #.# #.# #.# ###
1: .#. ##. .#. .#. ###
2: ##. ..# .#. #.. ###
3: ##. ..# .#. ..# ##.
4: #.# #.# ### ..# ..#
5: ### #.. ##. ..# ##.
6: .## #.. ### #.# ###
7: ### ..# .#. .#. .#.
8: ### #.# ### #.# ###
9: ### #.# ### ..# ##.
A: .#. #.# ### #.# #.#
b: #.. #.. ##. #.# ##.
C: .## #.. #.. #.. .##
d: ..# ..# .## #.# .##
E: ### #.. ##. #.. ###
F: ### #.. ##. #.. #..
G: .## #.. #.# #.# .##
H: #.# #.# ### #.# #.#
L: #.. #.. #.. #.. ###
N: #.# ### ### ### #.#
O: ### #.# #.# #.# ###
P: ##. #.# ##. #.. #..
S: .## #.. .#. ..# ##.
U: #.# #.# #.# #.# ###
V: #.# #.# #.# #.# .#.
X: #.# #.# .#. #.# #.#
h: #.. #.. ##. #.# #.#
o: ... ... ### #.# ###
-: ... ... ### ... ...
:: . # . # .
': # # . . .
""")
F3[" "] = ["."] * 5
F3["f"] = [".##", "#..", "##.", "#..", "#.."]
F3["g"] = [".##", "#.#", ".##", "..#", "##."]
F3["."] = ["."] * 4 + ["#"]

# 4x7 condensed: 22 OCT digits fit in 110 px.
F4 = font("""
0: .##. #..# #..# #..# #..# #..# .##.
1: .#.. ##.. .#.. .#.. .#.. .#.. ###.
2: .##. #..# ...# ..#. .#.. #... ####
3: ###. ...# ...# .##. ...# ...# ###.
4: ..#. .##. #.#. #.#. #### ..#. ..#.
5: #### #... ###. ...# ...# #..# .##.
6: .##. #... #... ###. #..# #..# .##.
7: #### ...# ..#. ..#. .#.. .#.. .#..
8: .##. #..# #..# .##. #..# #..# .##.
9: .##. #..# #..# .### ...# ...# .##.
A: .##. #..# #..# #### #..# #..# #..#
b: #... #... ###. #..# #..# #..# ###.
C: .##. #..# #... #... #... #..# .##.
d: ...# ...# .### #..# #..# #..# .###
E: #### #... #... ###. #... #... ####
F: #### #... #... ###. #... #... #...
-: .... .... .... #### .... .... ....
""")

# 5x7 classic dot matrix.
F5 = font("""
0: .###. #...# #..## #.#.# ##..# #...# .###.
1: ..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###.
2: .###. #...# ....# ...#. ..#.. .#... #####
3: #####. ...#. ..#.. ...#. ....# #...# .###.
4: ...#. ..##. .#.#. #..#. ##### ...#. ...#.
5: ##### #.... ####. ....# ....# #...# .###.
6: ..##. .#... #.... ####. #...# #...# .###.
7: ##### ....# ...#. ..#.. .#... .#... .#...
8: .###. #...# #...# .###. #...# #...# .###.
9: .###. #...# #...# .#### ....# ...#. .##..
A: .###. #...# #...# ##### #...# #...# #...#
b: #.... #.... ####. #...# #...# #...# ####.
C: .###. #...# #.... #.... #.... #...# .###.
d: ....# ....# .#### #...# #...# #...# .####
E: ##### #.... #.... ####. #.... #.... #####
F: ##### #.... #.... ####. #.... #.... #....
-: ..... ..... ..... ##### ..... ..... .....
""")
F5["3"] = [r[:5] for r in F5["3"]]
F4[" "] = F5[" "] = [".."] * 7


# --- canvas -------------------------------------------------------------------


class Screen:
    def __init__(self):
        self.px = [[0] * W for _ in range(H)]

    def set(self, x, y, on=1):
        if 0 <= x < W and 0 <= y < H:
            self.px[y][x] = on

    def rect(self, x0, y0, x1, y1, fill=True):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                if fill or y in (y0, y1) or x in (x0, x1):
                    self.set(x, y)

    def invert(self, x0, y0, x1, y1):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                if 0 <= x < W and 0 <= y < H:
                    self.px[y][x] ^= 1

    def text(self, x, y, s, f, group=0, gap=0, right=False, space=1):
        """Draw s; with group/gap, add `gap` extra px between digit groups of
        `group` counted from the right. Returns the x after the text."""
        if right:
            x = x - width(s, f, group, gap, space) + 1
        n = len(s)
        for i, c in enumerate(s):
            g = f[c]
            for r, row in enumerate(g):
                for k, b in enumerate(row):
                    if b == "#":
                        self.set(x + k, y + r)
            x += len(g[0]) + space
            if group and (n - 1 - i) % group == 0 and i < n - 1:
                x += gap
        return x

    def art(self):
        return "".join("".join("#" if p else "." for p in r) + "\n" for r in self.px)


def width(s, f, group=0, gap=0, space=1):
    n = len(s)
    w = sum(len(f[c][0]) + space for c in s) - space
    if group:
        w += ((n - 1) // group) * gap
    return w


# --- font variants ------------------------------------------------------------

# All-caps HEX: letters B and D get square corners, digits 0 and 8 stay round.
F5C = dict(F5)
F5C.update(font("""
B: ####. #...# #...# ####. #...# #...# ####.
D: ###.. #..#. #...# #...# #...# #..#. ###..
"""))
F5C["."] = [".."] * 5 + ["##"] * 2
F4C = dict(F4)
F4C.update(font("""
B: ###. #..# #..# ###. #..# #..# ###.
D: ###. #..# #..# #..# #..# #..# ###.
"""))
F3C = dict(F3)
F3C.update(font("""
B: ##. #.# ##. #.# ##.
D: ##. #.# #.# #.# ##.
0: .#. #.# #.# #.# .#.
"""))
F3C["."] = ["."] * 4 + ["#"]

# Letters for the indicator display.
F5A = dict(F5C)
F5A.update(font("""
A: .###. #...# #...# ##### #...# #...# #...#
G: .###. #...# #.... #.### #...# #...# .####
H: #...# #...# #...# ##### #...# #...# #...#
I: .###. ..#.. ..#.. ..#.. ..#.. ..#.. .###.
J: ..### ...#. ...#. ...#. ...#. #..#. .##..
K: #...# #..#. #.#.. ##... #.#.. #..#. #...#
L: #.... #.... #.... #.... #.... #.... #####
M: #...# ##.## #.#.# #.#.# #...# #...# #...#
N: #...# #...# ##..# #.#.# #..## #...# #...#
O: .###. #...# #...# #...# #...# #...# .###.
P: ####. #...# #...# ####. #.... #.... #....
Q: .###. #...# #...# #...# #.#.# #..#. .##.#
R: ####. #...# #...# ####. #.#.. #..#. #...#
S: .#### #.... #.... .###. ....# ....# ####.
T: ##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#..
U: #...# #...# #...# #...# #...# #...# .###.
V: #...# #...# #...# #...# #...# .#.#. ..#..
W: #...# #...# #...# #.#.# #.#.# #.#.# .#.#.
X: #...# #...# .#.#. ..#.. .#.#. #...# #...#
Y: #...# #...# .#.#. ..#.. ..#.. ..#.. ..#..
Z: ##### ....# ...#. ..#.. .#... #.... #####
:: . . # . . # .
': # # . . . . .
"""))
F3A = dict(F3C)
F3A[" "] = [".."] * 5
F3A.update(font("""
B: ##. #.# ##. #.# ##.
I: ### .#. .#. .#. ###
K: #.# #.# ##. #.# #.#
M: #.# ### ### #.# #.#
O: ### #.# #.# #.# ###
Q: .#. #.# #.# ##. .##
R: ##. #.# ##. #.# #.#
T: ### .#. .#. .#. .#.
W: #.# #.# ### ### #.#
Y: #.# #.# .#. .#. .#.
Z: ### ..# .#. #.. ###
"""))

# --- 1. calculator state -------------------------------------------------------------


@dataclass
class State:
    value: int                 # bit pattern, 0 <= value < 2**word
    word: int = 64
    sign: str = "2C"           # 2C, 1C, UN
    base: str = "DEC"          # entry base: DEC HEX OCT BIN
    typing: str | None = None  # digits typed so far, during entry
    carry: bool = False
    overflow: bool = False
    low_batt: bool = False
    vbat: float = 2.94
    shift: str | None = None   # "f" or "g" prefix pending
    flags: set = field(default_factory=set)  # further indicators that are on, e.g. "F0", "LZ", "M1"
    status_key: bool = False   # a status key (e.g. R/S) is held
    bin_page: int = 0


# --- 2. display model ------------------------------------------------------------------


@dataclass
class Line:
    text: str
    base: str  # DEC HEX OCT: rendered as the suffix d h o


@dataclass
class Numbers:
    """Main screen: the entry line (lower) and the other base (upper)."""
    entry: Line
    other: Line
    indicators: list = field(default_factory=list)  # any of "BATT", "f", "g", "C", "V"


@dataclass
class Bits:
    """BIN screen: one page of up to 64 bits, most significant first."""
    bits: str
    page: int
    pages: int
    low_batt: bool


@dataclass
class Status:
    """Status display, shown while the status key is held."""
    word: int
    sign: str
    indicators: list  # names of the indicators that are on (see STATUS_SLOTS)
    vbat: float
    low_batt: bool


RADIX = {"DEC": 10, "HEX": 16, "OCT": 8, "BIN": 2}


def to_text(v, base, s):
    mask = (1 << s.word) - 1
    v &= mask
    if base == "HEX":
        return f"{v:X}"
    if base == "OCT":
        return f"{v:o}"
    if base == "BIN":
        return f"{v:0{s.word}b}"
    neg = s.sign != "UN" and v >> (s.word - 1)
    if not neg:
        return str(v)
    return str(v - (1 << s.word)) if s.sign == "2C" else f"-{~v & mask}"


def model(s):
    if s.status_key:
        on = {n for n, b in (("C", s.carry), ("V", s.overflow)) if b} | s.flags
        return Status(s.word, s.sign, [n for n in STATUS_SLOTS if n in on], s.vbat, s.low_batt)
    if s.base == "BIN":
        b = to_text(s.value, "BIN", s)
        pages = (len(b) + 63) // 64
        end = len(b) - 64 * s.bin_page  # page 0 = the lowest 64 bits
        return Bits(b[max(0, end - 64):end], s.bin_page, pages, s.low_batt)
    v = int(s.typing, RADIX[s.base]) if s.typing else s.value
    entry = Line(s.typing or to_text(v, s.base, s), s.base)
    other_base = "HEX" if s.base == "DEC" else "DEC"
    ind = [n for n, on in (("BATT", s.low_batt), (s.shift, s.shift), ("C", s.carry), ("V", s.overflow)) if on]
    return Numbers(entry, Line(to_text(v, other_base, s), other_base), ind)


# --- 3. renderer -------------------------------------------------------------------------


@dataclass
class Style:
    group: bool = True             # digit grouping by spacing


SUFFIX = {"DEC": "d", "HEX": "h", "OCT": "o", "BIN": "b"}


def grouping(base, f):
    """(digits per group, extra px between groups)."""
    return (4, 3 if f is F5C else 2) if base == "HEX" else (3, 1)


def value_width(line, f, group):
    g, gap = grouping(line.base, f)
    return width(line.text, f, g if group else 0, gap) + 5


def put_line(sc, y, line, f, group):
    """Value right-aligned, its suffix after it, bottom-aligned with the digits."""
    g, gap = grouping(line.base, f)
    sc.text(131, y + 2, SUFFIX[line.base], F3, right=True)
    sc.text(126, y + 7 - len(f["0"]), line.text, f, group=g if group else 0, gap=gap, right=True)


def fit(line, room, fonts, style):
    """First (font, grouped) that fits in `room` px; grouping is dropped before the font shrinks."""
    for f in fonts:
        for group in ((True, False) if style.group else (False,)):
            if value_width(line, f, group) <= room:
                return f, group
    return fonts[-1], False


def render_numbers(m, style):
    sc = Screen()
    # Entry line, rows 9..15. It shrinks only for its own length, never for indicators.
    f, group = fit(m.entry, W, [F5C, F4C], style)
    put_line(sc, 9, m.entry, f, group)
    # Indicators on the left of the upper line.
    x = 0
    for name in m.indicators:
        if name == "BATT":
            sc.rect(0, 0, 1, 1)
            x = 4
        else:
            x = sc.text(x, 2, name, F3) + 2
    # Upper line, rows 0..6: gives way to the indicators.
    room = W - (x + 2 if x else 0)
    f, group = fit(m.other, room, [F5C, F4C, F3C], style)
    put_line(sc, 0, m.other, f, group)
    return sc


def render_bits(m, style):
    sc = Screen()
    b, n = m.bits, len(m.bits)
    if m.low_batt and m.pages == 1:
        sc.rect(0, 0, 1, 1)
    if n <= 16:  # one line in 5x7, nibble starts numbered above
        x = 131 - width(b, F5C, 4, 4) + 1
        sc.text(131, 9, b, F5C, group=4, gap=4, right=True)
        for i in range(n):
            pos = n - 1 - i
            if pos % 4 == 3 or i == 0:
                sc.text(x, 2, str(pos), F3)
            x += 6 + (4 if pos % 4 == 0 else 0)
    elif n <= 32:  # two lines of 16 in 5x7
        sc.text(131, 0, b[:-16], F5C, group=4, gap=4, right=True)
        sc.text(131, 9, b[-16:], F5C, group=4, gap=4, right=True)
    else:  # two lines of 32 in 3x5, nibbles overlined
        for y, part in ((0, b[:-32]), (9, b[-32:])):
            x = 132 - 4 * len(part)
            sc.text(x, y + 2, part, F3C)
            for k in range((len(part) + 3) // 4):  # k = nibble number from the right
                if k % 2 == 1:
                    x1 = 130 - 16 * k
                    sc.rect(max(x, x1 - 14), y, x1, y)
        if m.pages > 1:  # position in the left margin: dotted track, visible page solid
            h = H // m.pages
            top = (m.pages - 1 - m.page) * h
            bot = top + h - 1
            sc.rect(0, top, 1, bot)
            for y in list(range(top - 2, -1, -2)) + list(range(bot + 2, H, 2)):
                sc.set(0, y)  # dotted track, kept one row away from the bar
    return sc


def chip(sc, x, y, s, on, f=F5A):
    """A word; when on, inverse in a box (one px around). Returns x after it."""
    w, h = width(s, f), len(f["A"])
    if on:
        sc.rect(x - 1, y - 1, x + w, y + h)
        sc.invert(x, y, x + w - 1, y + h - 1)
    sc.text(x, y, s, f)
    if on:
        sc.invert(x, y, x + w - 1, y + h - 1)
    return x + w + (6 if f is F5A else 4)


# Status indicators in their fixed order. Values (word size, sign mode, battery
# voltage) are plain; indicators are small inverse chips, absent when off.
STATUS_SLOTS = ["C", "V", "LZ", "F0", "F1", "F2", "M1", "M2", "M3", "M4"]
SIGN = {"2C": "2C", "1C": "1C", "UN": "UN"}


def slot_x():
    """x of each slot when all are shown: every indicator keeps its place."""
    xs, x = {}, 1
    for n in STATUS_SLOTS:
        xs[n] = x
        x += width(n, F3A) + 5
    return xs


def render_status(m, style):
    sc = Screen()
    # Line 1: values.
    x = sc.text(1, 1, str(m.word), F3A) + 4
    sc.text(x, 1, SIGN[m.sign], F3A)
    volts = f"{m.vbat:.2f}V"
    if m.low_batt:
        chip(sc, 130 - width(volts, F3A) + 1, 1, volts, True, F3A)
    else:
        sc.text(131, 1, volts, F3A, right=True)
    # Line 2: indicators that are on.
    xs = slot_x()
    for n in m.indicators:
        chip(sc, xs[n], 10, n, True, F3A)
    return sc


def render(m, style=Style()):
    return {Numbers: render_numbers, Bits: render_bits, Status: render_status}[type(m)](m, style)


# --- scenarios -------------------------------------------------------------------------------

V = 0xFEDCBA9876543210
V128 = (V << 64) | 0x0123456789ABCDEF

SCENARIOS = [
    ("Main screen", [
        ("m_dec", "Entry DEC, HEX above", "Unsigned: the 20-digit worst case.", State(V, sign="UN")),
        ("m_hex", "Entry HEX, DEC above", "64-bit DEC fits in 5×7 on the upper line (130 px) when no indicator is lit.",
         State(V, sign="UN", base="HEX")),
        ("m_hex_cv", "Entry HEX, carry and overflow set",
         "C and V take the left of the upper line; the 20-digit DEC gives way and drops to 4×7. "
         "The entry line is untouched.", State(V, sign="UN", base="HEX", carry=True, overflow=True)),
        ("m_dec_c", "Entry DEC, carry set", "16 HEX digits leave enough room: the upper line stays 5×7.",
         State(V, sign="UN", carry=True)),
        ("m_signed", "Signed minimum, overflow, low battery", "Battery dot top-left, then V.",
         State(1 << 63, overflow=True, low_batt=True)),
        ("m_shift", "f prefix pending, carry set", "The f/g prefix is an indicator like C and V.",
         State(V, shift="f", carry=True)),
        ("m_neg", "Entry DEC, 2's complement, negative", "", State(V)),
        ("m_8bit", "8 bit, entry DEC", "", State(0x80, word=8)),
        ("m_16bit", "16 bit unsigned, entry HEX, carry set", "", State(0xBEEF, word=16, sign="UN", base="HEX", carry=True)),
    ]),
    ("Entry line shrinks with its own length (OCT, 64 bit)", [
        ("e_oct20", "Typing: 20 digits", "Fits in 5×7 with grouping.",
         State(0, sign="UN", base="OCT", typing="17773456514166250310")),
        ("e_oct21", "Typing: 21 digits", "Still 5×7; grouping is dropped first.",
         State(0, sign="UN", base="OCT", typing="177734565141662503102")),
        ("e_oct22", "Typing: 22 digits", "Now the entry line is redrawn in 4×7, grouped again.",
         State(0, sign="UN", base="OCT", typing="1773345651416625031020")),
    ]),
    ("BIN screen (nothing else shown)", [
        ("b_16", "16 bit", "One line in 5×7; the upper line numbers the nibbles.", State(0xBEEF, word=16, base="BIN")),
        ("b_32", "32 bit", "Two lines of 16 in 5×7.", State(V & 0xFFFFFFFF, word=32, base="BIN")),
        ("b_64", "64 bit", "Two lines of 32 in 3×5, every other nibble overlined (from the right).",
         State(V, base="BIN")),
        ("b_128_1", "128 bit, page 1 of 2 (bits 127–64)",
         "The left margin shows the position: the visible half of the track is solid.",
         State(V128, word=128, base="BIN", bin_page=1)),
        ("b_128_0", "128 bit, page 2 of 2 (bits 63–0)", "", State(V128, word=128, base="BIN", bin_page=0)),
        ("b_40", "40 bit (custom word size)", "Short last line, aligned right like the bits it holds.",
         State(V & ((1 << 40) - 1), word=40, base="BIN")),
    ]),
    ("Status display (while a status key is held)", [
        ("s_fixed", "Carry, user flags 0 and 2, memory 1 in use",
         "Line 1: word size, sign mode (2C/1C/UN), battery voltage. Line 2: indicators that are on, as small "
         "inverse chips, each in its own fixed place.",
         State(V, carry=True, flags={"F0", "F2", "M1"}, status_key=True)),
        ("s_all", "Capacity check: every indicator on",
         "All ten slots: C, V, LZ (leading zeros), F0–F2 (user flags), M1–M4. About a third of line 2 "
         "and most of line 1 are still free.",
         State(V, word=32, sign="UN", carry=True, overflow=True,
               flags={"LZ", "F0", "F1", "F2", "M1", "M2", "M3", "M4"}, status_key=True)),
        ("s_batt", "Low battery", "The voltage is shown inverse when low.",
         State(V, overflow=True, low_batt=True, vbat=2.41, status_key=True)),
    ]),
]


# --- driver ----------------------------------------------------------------------


def main():
    out = HERE / "out"
    out.mkdir(exist_ok=True)
    page = [
        "<!doctype html><meta charset=utf-8><title>Calc-U-16 display lab</title>",
        "<style>body{background:#2a2a2c;color:#ddd;font:14px -apple-system,sans-serif;margin:24px;max-width:1000px}"
        "figure{margin:0 0 32px}img.big{width:960px;max-width:100%;display:block}img.real{width:85mm;margin-top:8px}"
        "figcaption b{color:#fff}figcaption p{margin:4px 0 6px;color:#aaa}code{color:#9c9;font-size:12px;"
        "display:block;margin-bottom:8px;white-space:pre-wrap}h3{margin-top:40px}</style>",
        "<h2>Calc-U-16 display lab</h2>",
        "<p>Round 4: state → display model → renderer. Under each title: the display model the "
        "renderer got. The small copy under each screen is about real size.</p>",
    ]
    txts = []
    for section, items in SCENARIOS:
        page.append(f"<h3>{html.escape(section)}</h3>")
        for name, title, note, state, *style in items:
            m = model(state)
            p = out / f"{name}.txt"
            p.write_text(render(m, *style).art())
            txts.append(str(p))
            page.append(f"<figure><figcaption><b>{html.escape(title)}</b><p>{html.escape(note)}</p>"
                        f"<code>{html.escape(repr(m))}</code></figcaption>"
                        f"<img class=big src='{name}.svg'><img class=real src='{name}.svg'></figure>")
    if not SIM.exists():
        sys.exit(f"simulator not built: {SIM}")
    subprocess.run([str(SIM), "render", "-o", str(out), *txts], check=True, stdout=subprocess.DEVNULL)
    (out / "index.html").write_text("\n".join(page))
    print(out / "index.html")


if __name__ == "__main__":
    main()
