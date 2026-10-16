# firmware: vendor/DM12_34.hex
wait 3s
keys 1
keys 2 ENTER 3 +
expect-screen golden/dm12_5.txt
