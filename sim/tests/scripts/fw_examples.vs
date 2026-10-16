# firmware: firmware/target/calcu16-fw.hex
# Basic examples (strict order of entry, ENTER is =).
wait 1s
expect-serial "Calc-U-16"
# Add two decimal numbers, then view the sum in hex: 12 + 30 = 42 = 2A h
keys 1 2 + 3 0 ENTER
expect-screen golden/fw_42d.txt
keys HEX
expect-screen golden/fw_2Ah.txt
# HEX FF + DEC 55 = 310
keys HEX F F + DEC 5 5 ENTER
expect-screen golden/fw_310d.txt
# Strict order of entry: 5 + 4 × 3 = 27; pressing × shows the intermediate 9
keys 5 + 4 ×
expect-screen golden/fw_9_intermediate.txt
keys 3 ENTER
expect-screen golden/fw_27d.txt
# Divide by zero shows Error 0; the next key only clears it
keys ÷ 0 ENTER
expect-screen golden/fw_error0.txt
keys 1
expect-screen golden/fw_after_error.txt
