# firmware: vendor/DM16_34.hex
# Cold start shows MEMORY CLEAR; the first key only dismisses it.
wait 3s
expect-beeps 2
expect-screen golden/dm16_memory_clear.txt
keys 1
# 2 ENTER 3 + = 5 h (handbook arithmetic)
keys 2 ENTER 3 +
expect-screen golden/dm16_5h.txt
# Handbook p.18: HEX 1234 ← gives 123 h; ENTER ← gives 0 h
keys CLx HEX 1 2 3 4 ←
expect-screen golden/dm16_123h.txt
keys ENTER ←
expect-screen golden/dm16_0h.txt
# NOT of 0 (16-bit word) = FFFF h
keys NOT
expect-screen golden/dm16_ffffh.txt
