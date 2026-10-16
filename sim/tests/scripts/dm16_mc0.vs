# firmware: vendor/DM16_MC0_34.hex
wait 3s
keys 1
keys 2 ENTER 3 +
expect-screen golden/dm16_5h.txt
wait 100ms
reset
wait 100ms
serial "\r"
expect-serial "VOYAGER >> "
serial "?\r"
expect-serial "Firmware DM16_MC0_V34"
