#!/bin/sh
# Builds the DM16L firmware: target/calcu16-fw.hex (checksum-patched, ready for lpc21isp).
set -eu
cd "$(dirname "$0")"
# Use rustup's cargo (keg-only Homebrew rustup) so rust-toolchain.toml applies.
[ -d /opt/homebrew/opt/rustup/bin ] && PATH=/opt/homebrew/opt/rustup/bin:$PATH

(cd dm16l && cargo build --release)
elf=target/thumbv6m-none-eabi/release/dm16l
arm-none-eabi-objcopy -O binary "$elf" target/calcu16-fw.bin
python3 tools/lpc_image.py target/calcu16-fw.bin target/calcu16-fw.hex
arm-none-eabi-size "$elf"
