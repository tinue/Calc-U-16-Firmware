# firmware: firmware/target/calcu16-fw.hex
# Bootloader escape 1: D held at reset enters the bootloader before anything else runs.
# On the tested unit the PGM button does not reach the ISP pin, so the firmware's own
# escapes are the only way back into the bootloader: they must never break.
hold D
reset
expect-halt bootloader 1s
