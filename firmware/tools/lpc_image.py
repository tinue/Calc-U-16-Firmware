#!/usr/bin/env python3
"""Makes a raw LPC111x firmware image flash-ready and writes it as Intel HEX.

- Patches the vector checksum: the boot ROM only starts user code if the first
  eight vector words sum to zero (word 7, offset 0x1C, is reserved for this).
- Refuses images whose word at 0x2FC is a Code Read Protection pattern, which
  could lock the chip against ISP access.

Usage: lpc_image.py firmware.bin firmware.hex
"""
import struct
import sys

CRP_PATTERNS = {
    0x4E697370: "NO_ISP",
    0x12345678: "CRP1",
    0x87654321: "CRP2",
    0x43218765: "CRP3",
}


def to_ihex(data: bytes) -> str:
    lines = []
    for off in range(0, len(data), 16):
        chunk = data[off : off + 16]
        if off and off % 0x10000 == 0:
            rec = bytes([2, 0, 0, 4]) + struct.pack(">H", off >> 16)
            lines.append(":" + rec.hex().upper() + "%02X" % (-sum(rec) & 0xFF))
        rec = bytes([len(chunk)]) + struct.pack(">H", off & 0xFFFF) + b"\x00" + chunk
        lines.append(":" + rec.hex().upper() + "%02X" % (-sum(rec) & 0xFF))
    lines.append(":00000001FF")
    return "\n".join(lines) + "\n"


def main() -> None:
    src, dst = sys.argv[1], sys.argv[2]
    img = bytearray(open(src, "rb").read())
    if len(img) < 0x300:
        img.extend(b"\xff" * (0x300 - len(img)))
    words = list(struct.unpack_from("<7I", img, 0))
    struct.pack_into("<I", img, 0x1C, (-sum(words)) & 0xFFFFFFFF)
    crp = struct.unpack_from("<I", img, 0x2FC)[0]
    if crp in CRP_PATTERNS:
        sys.exit(f"refusing image: word at 0x2FC is {CRP_PATTERNS[crp]} ({crp:#010x})")
    if len(img) > 64 * 1024:
        sys.exit(f"image too large: {len(img)} bytes")
    open(dst, "w").write(to_ihex(bytes(img)))
    print(f"{dst}: {len(img)} bytes, checksum patched, CRP word {crp:#010x} ok")


if __name__ == "__main__":
    main()
