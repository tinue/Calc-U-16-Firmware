#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial>=3.5"]
# ///
"""Flash a firmware image onto a SwissMicros Voyager (DM-L) calculator.

Guides you through putting the calculator into its ROM bootloader, then flashes
with lpc21isp. The bootloader accepts a single handshake per reset, so nothing
else talks to it before lpc21isp.

  tools/flash.py                       # flash firmware/target/calcu16-fw.hex
  tools/flash.py --vendor DM16_MC0     # restore vendor/DM16_MC0_34.hex
  tools/flash.py path/to/image.hex --port /dev/cu.usbserial-0001
"""

import argparse
import shutil
import subprocess
import sys
import time
from pathlib import Path

import serial
from serial.tools import list_ports

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_IMAGE = ROOT / "firmware" / "target" / "calcu16-fw.hex"
CP2102 = (0x10C4, 0xEA60)
ISP_BAUD = "115200"
XTAL_KHZ = "12000"  # the bootloader runs from the 12 MHz internal oscillator


def say(text: str = "") -> None:
    print(text, flush=True)


def find_port(wanted: str | None) -> str:
    if wanted:
        return wanted
    announced = False
    while True:
        ports = [p.device for p in list_ports.comports() if (p.vid, p.pid) == CP2102 and "/cu." in p.device]
        ports = ports or [p.device for p in list_ports.comports() if (p.vid, p.pid) == CP2102]
        if ports:
            say(f"Calculator found on {ports[0]}")
            return ports[0]
        if not announced:
            say("Connect the calculator with its USB cable (waiting for the CP2102 serial bridge, Ctrl-C aborts)...")
            announced = True
        time.sleep(0.5)


def try_serial_bootloader(port: str) -> bool:
    """Asks a running firmware to enter the bootloader over the serial console."""
    try:
        with serial.Serial(port, 38400, timeout=0.3) as s:
            s.reset_input_buffer()
            seen = b""
            for _ in range(3):  # the vendor console wants the command three times
                s.write(b"bootloader\r")
                time.sleep(0.4)
                seen += s.read(4096)
                if b"Launching bootloader" in seen or b"entering bootloader" in seen:
                    time.sleep(0.3)
                    return True
                if b"bootloader" not in seen:
                    break  # nobody listening (vendor firmware not in console mode, or asleep)
    except serial.SerialException as e:
        say(f"  (serial port busy or unavailable: {e})")
    return False


INSTRUCTIONS = """
Put the calculator into its ROM bootloader. What to press depends on the firmware it runs now:

  SwissMicros firmware:
    1. Press ON to switch the calculator off.
    2. Hold D, press and release ON, then release D.
    3. The display shows "READY FOR FLASHING" / "RESET TO ABORT".

  calcu16-fw (Calc-U-16):
    1. Press ON to switch the calculator off.
    2. Hold D and press ON.
    3. The display stays blank: that is expected, the bootloader does not drive the LCD.

  Calculator unresponsive:
    Hold D and press the reset button (back of the PCB, lower left).
"""


RISK = """
WARNING: flashing is entirely at your own risk.

Flashing can brick your calculator, temporarily or for good, and may void its warranty.
This is an unofficial project, not affiliated with or supported by SwissMicros or HP.
The author accepts no responsibility or liability for any damage or data loss, will
not help you recover the calculator and will not pay for repair or replacement.
SwissMicros has no obligation to help with a calculator that ran third-party
firmware, and will most likely refuse. Flashing has only been tested from a Mac
running macOS; anything else is completely untested. If you are not prepared to
lose your calculator, stop here.
"""


RECOVERY = """
The bootloader did not answer.

If the calculator shows "READY FOR FLASHING", its bootloader has already been
contacted since the last reset: it measures the baud rate from the first
characters it receives and accepts only one handshake. It recovers only through
a reset:
  1. Press the reset button (back of the PCB, lower left). The calculator restarts
     its normal firmware.
  2. Enter the bootloader again (see below) and do not run any other serial program.
""" + INSTRUCTIONS


def wait_for_banner(port: str, seconds: float = 4.0) -> str:
    end = time.time() + seconds
    data = b""
    try:
        with serial.Serial(port, 38400, timeout=0.2) as s:
            while time.time() < end:
                data += s.read(4096)
    except serial.SerialException:
        pass
    return data.decode(errors="replace").strip()


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("image", nargs="?", type=Path, help=f"Intel HEX image (default: {DEFAULT_IMAGE.relative_to(ROOT)})")
    ap.add_argument("--vendor", metavar="MODEL", help="flash vendor/<MODEL>_34.hex instead, e.g. DM16_MC0 or DM16")
    ap.add_argument("--port", help="serial port (default: auto-detect the CP2102)")
    ap.add_argument("--yes", action="store_true", help="do not ask for confirmation")
    ap.add_argument("--accept-risk", action="store_true", help="skip the risk warning (you have read it and accept it)")
    args = ap.parse_args()

    if not args.accept_risk:
        say(RISK)
        if input('Type "I accept the risk" to continue: ').strip().lower() != "i accept the risk":
            say("Not flashed.")
            return 1
        say()

    if args.vendor:
        image = ROOT / "vendor" / f"{args.vendor}_34.hex"
        if not image.exists():
            say(f"{image} not found; run tools/fetch-vendor-firmware.sh")
            return 1
    else:
        image = args.image or DEFAULT_IMAGE
        if not image.exists():
            say(f"{image} not found; build it with firmware/build.sh")
            return 1
    if not shutil.which("lpc21isp"):
        say("lpc21isp not found (brew install lpc21isp)")
        return 1

    say(f"Image: {image}")
    say("Flashing erases the calculator's memory (stack, registers, programs).")
    say("For the SwissMicros firmware, save it first: serial console 's' (see firmware/README.md).")
    say()
    port = find_port(args.port)

    # The bootloader sets its baud rate from the first characters it receives, so
    # nothing but lpc21isp may talk to it. Only send the serial "bootloader"
    # command while a normal firmware is running.
    already = input("Is the calculator already in the bootloader (\"READY FOR FLASHING\")? [y/N] ")
    if already.strip().lower() != "y":
        say("Trying to start the bootloader over the serial port...")
        if try_serial_bootloader(port):
            say("  the running firmware launched the bootloader.")
        else:
            say("  no answer (normal unless a console is active).")
            say(INSTRUCTIONS)
            input("Press Enter when the calculator is in the bootloader... ")

    if not args.yes and input(f"Flash {image.name} now? [y/N] ").strip().lower() != "y":
        say("Aborted; press the reset button to leave the bootloader.")
        return 1

    # No separate detection step: an LPC bootloader accepts only one handshake per
    # reset, so lpc21isp's own synchronisation must be the first contact.
    while True:
        say("Flashing (about 10-20 seconds)...")
        r = subprocess.run(["lpc21isp", "-verify", str(image), port, ISP_BAUD, XTAL_KHZ])
        if r.returncode == 0:
            break
        say(RECOVERY)
        if input("Press Enter to retry, or q to quit: ").strip().lower() == "q":
            return 1

    say("\nFlashed and started. Listening on the serial port for the start-up message...")
    banner = wait_for_banner(port)
    if banner:
        say(f"  calculator says: {banner!r}")
    if image.name.startswith("calcu16-fw"):
        say("Expected: the display shows 'CALC-U-16', then '0 h' above and '0 d' below.")
        say("If it stays blank but the message above appeared, adjust the contrast with f + / f -.")
    else:
        say("Expected: the SwissMicros start screen, then MEMORY CLEAR. Restore memory with the console 'l' command.")
    say("If nothing happens, press the reset button.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyboardInterrupt, EOFError):
        say("\nAborted.")
        sys.exit(130)
