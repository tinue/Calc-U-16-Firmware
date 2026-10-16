# firmware: vendor/DM16_34.hex
wait 100ms
serial "\r"
expect-serial "VOYAGER >> "
serial "bootloader\r"
expect-serial "two more times"
serial "bootloader\r"
expect-serial "one more time"
serial "bootloader\r"
expect-serial "Launching bootloader"
expect-halt bootloader 2s
expect-screen golden/dm16_ready_for_flashing.txt
