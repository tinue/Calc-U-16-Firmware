# firmware: vendor/DM16_34.hex
# A byte within ~0.5 s of reset starts the serial console.
wait 100ms
serial "\r"
expect-serial "VOYAGER >> "
serial "?\r"
expect-serial "Firmware DM16_V34"
expect-serial "b          Read battery voltage"
vbat 2950
serial "b\r"
expect-serial "BAT: 2951mV"
serial "ts 20250101 120000\r"
wait 1s
serial "t\r"
expect-serial "2025-01-01 12:00:01 WED"
serial "s\r"
expect-serial "DM16\r\n"
expect-serial "A: 00000000000000"
# Load memory (lines end in LF, as a terminal paste sends them)
serial "l\r"
expect-serial "Waiting for data..."
serial "DM16\nA: 00000000001234  B: 00000000000000  C: 00000000000000\nS: 00000000000000\nM: 00000000000000  N: 00000000000000  G: 00\n"
expect-serial "Read OK"
serial "s\r"
expect-serial "A: 00000000001234"
