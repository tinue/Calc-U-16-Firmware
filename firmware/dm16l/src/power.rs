//! Power states: off (deep sleep, woken by ON) and ISP bootloader entry.

use crate::keys;
use crate::regs::{self, nvic, scb, syscon, Pin, PULL_DOWN, PULL_NONE};

/// Start-logic input / NVIC interrupt of PIO0_11 (the ON line).
const ON_START_BIT: u32 = 1 << 11;

/// Puts board pins the firmware does not use into their lowest-current state, as the
/// vendor firmware does. Without this, the reset-default pull-ups feed the battery
/// measurement circuit continuously.
pub fn park_unused_pins() {
    // Battery measurement: PIO3_2 powers the voltage reference, PIO1_11 (AD7) reads it.
    // Both are parked low; PIO1_11 in analog mode (no digital input buffer).
    let reference = Pin(3, 2);
    reference.configure(PULL_NONE);
    reference.output(false);
    let ad7 = Pin(1, 11);
    regs::write(ad7.iocon_addr(), 0x40);
    ad7.output(false);
    // Purpose unknown (possibly USB VBUS sense): no pull, as in the vendor firmware.
    Pin(1, 4).configure(PULL_NONE);
    // ISP entry pin: pull-down, as in the vendor firmware.
    Pin(0, 1).configure(PULL_DOWN);
}

/// Deep sleep until ON is pressed. The caller re-initialises the LCD afterwards.
pub fn sleep_until_on() {
    cortex_m::interrupt::disable();
    regs::write(scb::ICSR, 1 << 25); // PENDSTCLR: drop a pending SysTick

    // ON connects PIO0_11 to column C0, which is held low so a press pulls the line low.
    keys::park_columns();

    regs::modify(syscon::STARTAPRP0, |v| v & !ON_START_BIT); // falling edge
    regs::write(syscon::STARTRSRP0CLR, ON_START_BIT);
    // A stale pending bit would end the sleep at once: clear it before enabling.
    regs::write(nvic::ICPR, ON_START_BIT);
    regs::modify(syscon::STARTERP0, |v| v | ON_START_BIT);
    regs::write(nvic::ISER, ON_START_BIT);

    regs::write(syscon::PDSLEEPCFG, 0x18FF);
    regs::write(syscon::PDAWAKECFG, regs::read(syscon::PDRUNCFG));
    regs::modify(scb::SCR, |v| v | 4); // SLEEPDEEP

    // With PRIMASK set, the pending start-logic IRQ wakes the core without running a handler.
    cortex_m::asm::dsb();
    cortex_m::asm::wfi();

    regs::modify(scb::SCR, |v| v & !4);
    regs::write(nvic::ICER, ON_START_BIT);
    regs::modify(syscon::STARTERP0, |v| v & !ON_START_BIT);
    regs::write(syscon::STARTRSRP0CLR, ON_START_BIT);
    regs::write(nvic::ICPR, ON_START_BIT);
    keys::unpark_columns();
    // SAFETY: re-enabling interrupts at the end of the sequence started above.
    unsafe { cortex_m::interrupt::enable() };
}

/// Re-enters the boot ROM's serial ISP (IAP command 57). Does not return.
pub fn enter_isp() -> ! {
    cortex_m::interrupt::disable();
    // ISP expects the 12 MHz IRC as main clock with AHB divider 1 (already the case here).
    regs::write(syscon::MAINCLKSEL, 0);
    regs::write(syscon::MAINCLKUEN, 0);
    regs::write(syscon::MAINCLKUEN, 1);
    regs::write(syscon::SYSAHBCLKDIV, 1);
    regs::modify(syscon::SYSAHBCLKCTRL, |v| {
        v | syscon::CLK_IOCON | syscon::CLK_UART
    });
    let command: [u32; 5] = [57, 0, 0, 0, 0];
    let mut result = [0u32; 5];
    // SAFETY: 0x1FFF1FF1 is the documented LPC111x IAP entry (Thumb).
    let iap: extern "C" fn(*const u32, *mut u32) =
        unsafe { core::mem::transmute(0x1FFF_1FF1usize) };
    iap(command.as_ptr(), result.as_mut_ptr());
    loop {
        cortex_m::asm::nop();
    }
}
