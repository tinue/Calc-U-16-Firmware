//! Minimal LPC111x register access (addresses from UM10398, see docs/hardware.md).

#[inline(always)]
pub fn read(addr: u32) -> u32 {
    // SAFETY: callers pass valid, aligned peripheral register addresses.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

#[inline(always)]
pub fn write(addr: u32, v: u32) {
    // SAFETY: as for `read`.
    unsafe { core::ptr::write_volatile(addr as *mut u32, v) }
}

#[inline(always)]
pub fn modify(addr: u32, f: impl FnOnce(u32) -> u32) {
    write(addr, f(read(addr)));
}

pub mod syscon {
    const BASE: u32 = 0x4004_8000;
    /// Vector table at 0: 0 = boot ROM, 1 = RAM, 2 = user flash.
    pub const SYSMEMREMAP: u32 = BASE;
    pub const PRESETCTRL: u32 = BASE + 0x004;
    pub const SYSAHBCLKCTRL: u32 = BASE + 0x080;
    pub const UARTCLKDIV: u32 = BASE + 0x098;
    pub const STARTAPRP0: u32 = BASE + 0x200;
    pub const STARTERP0: u32 = BASE + 0x204;
    pub const STARTRSRP0CLR: u32 = BASE + 0x208;
    pub const PDSLEEPCFG: u32 = BASE + 0x230;
    pub const PDAWAKECFG: u32 = BASE + 0x234;
    pub const PDRUNCFG: u32 = BASE + 0x238;
    pub const MAINCLKSEL: u32 = BASE + 0x070;
    pub const MAINCLKUEN: u32 = BASE + 0x074;
    pub const SYSAHBCLKDIV: u32 = BASE + 0x078;

    /// SYSAHBCLKCTRL bits.
    pub const CLK_I2C: u32 = 1 << 5;
    pub const CLK_GPIO: u32 = 1 << 6;
    pub const CLK_CT32B0: u32 = 1 << 9;
    pub const CLK_UART: u32 = 1 << 12;
    pub const CLK_IOCON: u32 = 1 << 16;
}

pub mod uart {
    const BASE: u32 = 0x4000_8000;
    /// RBR (read) / THR (write) / DLL (DLAB=1)
    pub const DATA: u32 = BASE;
    /// DLM when DLAB=1, IER otherwise
    pub const DLM: u32 = BASE + 0x04;
    pub const FCR: u32 = BASE + 0x08;
    pub const LCR: u32 = BASE + 0x0C;
    pub const LSR: u32 = BASE + 0x14;
    pub const FDR: u32 = BASE + 0x28;
}

pub mod nvic {
    pub const ISER: u32 = 0xE000_E100;
    pub const ICER: u32 = 0xE000_E180;
    pub const ICPR: u32 = 0xE000_E280;
}

pub mod scb {
    pub const ICSR: u32 = 0xE000_ED04;
    pub const SCR: u32 = 0xE000_ED10;
}

/// A GPIO pin `PIOport_pin`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Pin(pub u8, pub u8);

/// IOCON register offset per pin (48-pin LPC111x).
const IOCON_MAP: [(u8, u8, u16); 42] = [
    (2, 6, 0x000),
    (2, 0, 0x008),
    (0, 0, 0x00C),
    (0, 1, 0x010),
    (1, 8, 0x014),
    (0, 2, 0x01C),
    (2, 7, 0x020),
    (2, 8, 0x024),
    (2, 1, 0x028),
    (0, 3, 0x02C),
    (0, 4, 0x030),
    (0, 5, 0x034),
    (1, 9, 0x038),
    (3, 4, 0x03C),
    (2, 4, 0x040),
    (2, 5, 0x044),
    (3, 5, 0x048),
    (0, 6, 0x04C),
    (0, 7, 0x050),
    (2, 9, 0x054),
    (2, 10, 0x058),
    (2, 2, 0x05C),
    (0, 8, 0x060),
    (0, 9, 0x064),
    (0, 10, 0x068),
    (1, 10, 0x06C),
    (2, 11, 0x070),
    (0, 11, 0x074),
    (1, 0, 0x078),
    (1, 1, 0x07C),
    (1, 2, 0x080),
    (3, 0, 0x084),
    (3, 1, 0x088),
    (2, 3, 0x08C),
    (1, 3, 0x090),
    (1, 4, 0x094),
    (1, 11, 0x098),
    (3, 2, 0x09C),
    (1, 5, 0x0A0),
    (1, 6, 0x0A4),
    (1, 7, 0x0A8),
    (3, 3, 0x0AC),
];

/// IOCON MODE field values (bits 4:3).
pub const PULL_NONE: u32 = 0 << 3;
pub const PULL_DOWN: u32 = 1 << 3;
pub const PULL_UP: u32 = 2 << 3;

impl Pin {
    fn gpio_base(self) -> u32 {
        0x5000_0000 + ((self.0 as u32) << 16)
    }

    fn mask(self) -> u32 {
        1 << self.1
    }

    pub fn iocon_addr(self) -> u32 {
        let off = IOCON_MAP
            .iter()
            .find(|e| e.0 == self.0 && e.1 == self.1)
            .map_or(0, |e| e.2 as u32);
        0x4004_4000 + off
    }

    /// FUNC value selecting GPIO (some pins default to SWD/reserved functions).
    fn gpio_func(self) -> u32 {
        match (self.0, self.1) {
            (0, 0) | (0, 10) | (0, 11) | (1, 0) | (1, 1) | (1, 2) | (1, 3) => 1,
            _ => 0,
        }
    }

    /// Selects GPIO function with the given pull mode.
    pub fn configure(self, pull: u32) {
        write(self.iocon_addr(), self.gpio_func() | pull | 0xC0);
    }

    /// Makes the pin an output at the given level. DATA writes to an input pin are
    /// ignored by the hardware, so the direction must be set first.
    pub fn output(self, high: bool) {
        modify(self.gpio_base() + 0x8000, |d| d | self.mask());
        self.set(high);
    }

    pub fn input(self) {
        modify(self.gpio_base() + 0x8000, |d| d & !self.mask());
    }

    /// Masked write: only this pin's bit changes.
    #[inline(always)]
    pub fn set(self, high: bool) {
        write(
            self.gpio_base() + (self.mask() << 2),
            if high { self.mask() } else { 0 },
        );
    }

    #[inline(always)]
    pub fn read(self) -> bool {
        read(self.gpio_base() + (self.mask() << 2)) != 0
    }
}
