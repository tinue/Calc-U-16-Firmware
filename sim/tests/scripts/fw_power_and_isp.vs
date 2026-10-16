# firmware: firmware/target/calcu16-fw.hex
wait 1s
keys 4 2
# ON switches off; other keys do nothing; ON switches on with X intact
keys ON
expect-display off
keys 7
expect-display off
# A short glitch on ON (contact bounce) wakes the chip, but it goes back to sleep
press ON 5ms
wait 1s
expect-display off
keys ON
wait 500ms
expect-display on
expect-screen golden/fw_42_after_wake.txt
# Contrast: f + raises EV by one
keys f +
expect-screen golden/fw_contrast.txt
# The serial command enters the ROM bootloader
serial "bootloader\r"
expect-serial "entering bootloader"
expect-halt bootloader 1s
