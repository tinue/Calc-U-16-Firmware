# firmware: firmware/target/calcu16-fw.hex
wait 1s
# 2 - 3 = -1 in all four bases (32-bit 2's complement)
keys 2 − 3 ENTER
expect-screen golden/fw_minus1_dec.txt
keys HEX
expect-screen golden/fw_minus1_hex.txt
keys OCT
expect-screen golden/fw_minus1_oct.txt
keys BIN
expect-screen golden/fw_minus1_bin.txt
# Digits outside the base are ignored: 2 in BIN, 9 in OCT
keys ← ← 1 2 0 1
expect-screen golden/fw_bin_101.txt
keys OCT 9 7 × 5 ENTER
expect-screen golden/fw_oct_43.txt
# R/S held shows the status display (word size, sign mode); release returns to the numbers
hold R/S
wait 100ms
expect-screen golden/fw_status.txt
release all
expect-screen golden/fw_oct_43.txt
# A pending f prefix is an indicator on the upper line
keys f
expect-screen golden/fw_f_pending.txt
keys f
# WSIZE (f STO) takes X as the new word size; the number from before comes back,
# truncated to the new size (HP-16C handbook example: 32767 at 16 bits is -1 at 8 bits)
keys DEC 3 2 7 6 7 ENTER 1 6 WSIZE
expect-screen golden/fw_wsize16.txt
keys 8 WSIZE
expect-screen golden/fw_wsize8.txt
keys BIN
expect-screen golden/fw_wsize8_bin.txt
# 0 WSIZE means 64 bits
keys 0 WSIZE
expect-screen golden/fw_wsize64_bin.txt
hold R/S
wait 100ms
expect-screen golden/fw_status64.txt
release all
