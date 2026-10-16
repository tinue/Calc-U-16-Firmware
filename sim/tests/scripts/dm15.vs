# firmware: vendor/DM15_34.hex
wait 3s
keys 1
keys 2 ENTER 3 +
expect-screen golden/dm15_5.txt
