# firmware: vendor/DM16_34.hex
# Setup items: with the calculator off, hold the item key and press ON.
wait 3s
keys 1
keys ON
wait 1s
vbat 2800
# E (1-5): System info: model, version, CPU clock, battery, RTC present
hold E
press ON
release all
wait 300ms
expect-screen golden/dm16_sysinfo.txt
