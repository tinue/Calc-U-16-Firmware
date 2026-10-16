# firmware: vendor/DM16_34.hex
wait 3s
keys 1
keys 4 2
# Auto-off after about 3 minutes without input
wait 170s
expect-display on
wait 30s
expect-display off
# Other keys do not wake it; ON does, and the stack survives
keys 5
expect-display off
keys ON
wait 500ms
expect-display on
expect-screen golden/dm16_after_wake.txt
