# firmware: firmware/target/calcu16-fw.hex
# Bootloader escape 2: calculator off, hold D, press ON (see fw_escape_reset.vs).
wait 1s
keys ON
expect-display off
hold D
keys ON
expect-halt bootloader 1s
