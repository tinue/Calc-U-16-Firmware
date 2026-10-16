//! ARMv6-M (Cortex-M0) Thumb interpreter.
//!
//! The CPU owns the register file and exception state. The memory map,
//! including the NVIC/SysTick/SCB registers, lives in [`crate::bus::Bus`].

pub mod disasm;
pub mod exceptions;

use crate::bus::Bus;
use serde::{Deserialize, Serialize};

pub const SP: usize = 13;
pub const LR: usize = 14;
pub const PC: usize = 15;

/// Why `step` did not execute an instruction normally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Halt {
    /// The ROM bootloader (IAP 57, Reinvoke ISP) was called.
    Bootloader,
    /// Unrecoverable fault: undefined instruction, bad memory access, ...
    Fault(String),
    /// BKPT instruction.
    Breakpoint(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cpu {
    pub r: [u32; 16],
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
    pub primask: bool,
    /// CONTROL.SPSEL (thread mode uses PSP when set).
    pub spsel: bool,
    /// Banked stack pointer that is not currently in r13.
    pub other_sp: u32,
    /// Current exception number (IPSR), 0 = thread mode.
    pub ipsr: u32,
    /// Stack of active exception numbers (innermost last).
    pub active: Vec<u32>,
    /// Set by WFI/WFE; cleared when an interrupt becomes pending.
    pub sleeping: bool,
    pub halted: Option<Halt>,
    /// Number of instructions executed (statistics only).
    pub instret: u64,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            r: [0; 16],
            n: false,
            z: false,
            c: false,
            v: false,
            primask: false,
            spsel: false,
            other_sp: 0,
            ipsr: 0,
            active: Vec::new(),
            sleeping: false,
            halted: None,
            instret: 0,
        }
    }

    /// Core reset: load SP and PC from the vector table.
    pub fn reset(&mut self, bus: &mut Bus) {
        let instret = self.instret;
        *self = Cpu::new();
        self.instret = instret;
        self.r[SP] = bus.read32(0) & !3;
        self.r[PC] = bus.read32(4) & !1;
        self.r[LR] = 0xFFFF_FFFF;
    }

    pub fn xpsr(&self) -> u32 {
        (self.n as u32) << 31
            | (self.z as u32) << 30
            | (self.c as u32) << 29
            | (self.v as u32) << 28
            | 1 << 24
            | self.ipsr
    }

    fn set_nz(&mut self, v: u32) {
        self.n = v & 0x8000_0000 != 0;
        self.z = v == 0;
    }

    fn add_with_carry(&mut self, a: u32, b: u32, carry: bool) -> u32 {
        let wide = a as u64 + b as u64 + carry as u64;
        let res = wide as u32;
        self.set_nz(res);
        self.c = wide > 0xFFFF_FFFF;
        self.v = ((a ^ res) & (b ^ res) & 0x8000_0000) != 0;
        res
    }

    fn fault(&mut self, msg: String) -> u32 {
        self.halted = Some(Halt::Fault(msg));
        1
    }

    /// Value of a register as an operand (PC reads as instruction address + 4).
    #[inline]
    fn rr(&self, n: usize) -> u32 {
        if n == PC {
            self.r[PC].wrapping_add(4)
        } else {
            self.r[n]
        }
    }

    /// Executes one instruction, or takes a pending exception. Returns the cycles used.
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        if self.halted.is_some() {
            return 1;
        }
        if bus.scs.any_pending() {
            if self.sleeping && bus.scs.wakeup_pending() {
                self.sleeping = false;
                bus.on_wake();
                if bus.trace.irq {
                    let msg = format!(
                        "wake: irq pending&enabled={:#x} systick={} pendsv={}",
                        bus.scs.irq_pending & bus.scs.irq_enabled,
                        bus.scs.systick_pending,
                        bus.scs.pendsv_pending
                    );
                    bus.trace_line(msg);
                }
            }
            if let Some(exc) = self.preempting_exception(bus) {
                self.sleeping = false;
                self.exception_entry(bus, exc, self.r[PC]);
                return 12;
            }
        }
        if self.sleeping {
            return 1;
        }
        let pc = self.r[PC];
        if (0x1FFF_0000..0x2000_0000).contains(&pc) {
            // Boot ROM entry (IAP). The bus performs the call; return to LR.
            if let Some(h) = bus.rom_call(&mut self.r) {
                self.halted = Some(h);
                return 1;
            }
            let lr = self.r[LR];
            self.branch_exchange(bus, lr);
            return 4;
        }
        let op = bus.fetch16(pc);
        self.instret += 1;
        let cycles = self.execute(bus, op, pc);
        if let Some(addr) = bus.take_fault() {
            self.halted = Some(Halt::Fault(format!("bus fault at {addr:#010x}, pc={pc:#06x}")));
        }
        cycles
    }

    /// Interworking branch (BX, BLX, POP {pc}): handles EXC_RETURN.
    fn branch_exchange(&mut self, bus: &mut Bus, target: u32) {
        if self.ipsr != 0 && target & 0xF000_0000 == 0xF000_0000 {
            self.exception_return(bus, target);
        } else {
            self.r[PC] = target & !1;
        }
    }

    fn write_sp_reg(&mut self, n: usize, v: u32) {
        self.r[n] = if n == SP { v & !3 } else { v };
    }

    fn execute(&mut self, bus: &mut Bus, op: u16, pc: u32) -> u32 {
        let op32 = op as u32;
        let rd = (op & 7) as usize;
        let rn = ((op >> 3) & 7) as usize;
        let mut next = pc.wrapping_add(2);
        let mut cycles = 1;
        match op >> 11 {
            0b00000..=0b00010 => {
                // LSL/LSR/ASR immediate
                let imm = (op32 >> 6) & 31;
                let m = self.r[rn];
                let res = match op >> 11 {
                    0 => {
                        if imm != 0 {
                            self.c = (m >> (32 - imm)) & 1 != 0;
                        }
                        m << imm
                    }
                    1 => {
                        let s = if imm == 0 { 32 } else { imm };
                        self.c = (m >> (s - 1)) & 1 != 0;
                        if s == 32 { 0 } else { m >> s }
                    }
                    _ => {
                        let s = if imm == 0 { 32 } else { imm };
                        self.c = (m >> (s - 1)) & 1 != 0;
                        ((m as i32) >> s.min(31)) as u32
                    }
                };
                self.set_nz(res);
                self.r[rd] = res;
            }
            0b00011 => {
                let a = self.r[rn];
                let b = if op & 0x400 != 0 { (op32 >> 6) & 7 } else { self.r[((op >> 6) & 7) as usize] };
                self.r[rd] = if op & 0x200 != 0 {
                    self.add_with_carry(a, !b, true)
                } else {
                    self.add_with_carry(a, b, false)
                };
            }
            0b00100..=0b00111 => {
                let d = ((op >> 8) & 7) as usize;
                let imm = op32 & 0xFF;
                match (op >> 11) & 3 {
                    0 => {
                        self.r[d] = imm;
                        self.set_nz(imm);
                    }
                    1 => {
                        self.add_with_carry(self.r[d], !imm, true);
                    }
                    2 => self.r[d] = self.add_with_carry(self.r[d], imm, false),
                    _ => self.r[d] = self.add_with_carry(self.r[d], !imm, true),
                }
            }
            0b01000 => {
                if op & 0x400 == 0 {
                    self.data_processing(op, rd, rn);
                } else {
                    // Hi register operations / branch exchange
                    let dn = rd | ((op as usize >> 4) & 8);
                    let m = ((op >> 3) & 15) as usize;
                    match (op >> 8) & 3 {
                        0 => {
                            let v = self.rr(dn).wrapping_add(self.rr(m));
                            if dn == PC {
                                next = v & !1;
                                cycles = 3;
                            } else {
                                self.write_sp_reg(dn, v);
                            }
                        }
                        1 => {
                            let (a, b) = (self.rr(dn), self.rr(m));
                            self.add_with_carry(a, !b, true);
                        }
                        2 => {
                            let v = self.rr(m);
                            if dn == PC {
                                next = v & !1;
                                cycles = 3;
                            } else {
                                self.write_sp_reg(dn, v);
                            }
                        }
                        _ => {
                            let target = self.rr(m);
                            if op & 0x80 != 0 {
                                self.r[LR] = next | 1;
                            }
                            self.branch_exchange(bus, target);
                            return 3;
                        }
                    }
                }
            }
            0b01001 => {
                let d = ((op >> 8) & 7) as usize;
                let addr = (pc.wrapping_add(4) & !3).wrapping_add((op32 & 0xFF) << 2);
                self.r[d] = bus.read32(addr);
                cycles = 2;
            }
            0b01010 | 0b01011 => {
                let addr = self.r[rn].wrapping_add(self.r[((op >> 6) & 7) as usize]);
                cycles = 2;
                match (op >> 9) & 7 {
                    0 => bus.write32(addr, self.r[rd]),
                    1 => bus.write16(addr, self.r[rd] as u16),
                    2 => bus.write8(addr, self.r[rd] as u8),
                    3 => self.r[rd] = bus.read8(addr) as i8 as i32 as u32,
                    4 => self.r[rd] = bus.read32(addr),
                    5 => self.r[rd] = bus.read16(addr) as u32,
                    6 => self.r[rd] = bus.read8(addr) as u32,
                    _ => self.r[rd] = bus.read16(addr) as i16 as i32 as u32,
                }
            }
            0b01100..=0b10001 => {
                let imm = (op32 >> 6) & 31;
                let base = self.r[rn];
                cycles = 2;
                match op >> 11 {
                    0b01100 => bus.write32(base.wrapping_add(imm << 2), self.r[rd]),
                    0b01101 => self.r[rd] = bus.read32(base.wrapping_add(imm << 2)),
                    0b01110 => bus.write8(base.wrapping_add(imm), self.r[rd] as u8),
                    0b01111 => self.r[rd] = bus.read8(base.wrapping_add(imm)) as u32,
                    0b10000 => bus.write16(base.wrapping_add(imm << 1), self.r[rd] as u16),
                    _ => self.r[rd] = bus.read16(base.wrapping_add(imm << 1)) as u32,
                }
            }
            0b10010 | 0b10011 => {
                let d = ((op >> 8) & 7) as usize;
                let addr = self.r[SP].wrapping_add((op32 & 0xFF) << 2);
                cycles = 2;
                if op & 0x800 != 0 {
                    self.r[d] = bus.read32(addr);
                } else {
                    bus.write32(addr, self.r[d]);
                }
            }
            0b10100 => {
                let d = ((op >> 8) & 7) as usize;
                self.r[d] = (pc.wrapping_add(4) & !3).wrapping_add((op32 & 0xFF) << 2);
            }
            0b10101 => {
                let d = ((op >> 8) & 7) as usize;
                self.r[d] = self.r[SP].wrapping_add((op32 & 0xFF) << 2);
            }
            0b10110 | 0b10111 => return self.misc(bus, op, pc),
            0b11000 => {
                // STMIA Rn!, {list}
                let n = ((op >> 8) & 7) as usize;
                let mut addr = self.r[n];
                for i in 0..8 {
                    if op & (1 << i) != 0 {
                        bus.write32(addr, self.r[i]);
                        addr = addr.wrapping_add(4);
                        cycles += 1;
                    }
                }
                self.r[n] = addr;
            }
            0b11001 => {
                // LDMIA Rn!, {list}
                let n = ((op >> 8) & 7) as usize;
                let mut addr = self.r[n];
                for i in 0..8 {
                    if op & (1 << i) != 0 {
                        self.r[i] = bus.read32(addr);
                        addr = addr.wrapping_add(4);
                        cycles += 1;
                    }
                }
                if op & (1 << n) == 0 {
                    self.r[n] = addr;
                }
            }
            0b11010 | 0b11011 => {
                let cond = (op >> 8) & 15;
                match cond {
                    14 => return self.fault(format!("UDF at {pc:#06x}")),
                    15 => {
                        // SVC: take exception 11 with return to next instruction.
                        self.exception_entry(bus, 11, next);
                        return 3;
                    }
                    _ => {
                        if self.condition(cond) {
                            next = pc.wrapping_add(4).wrapping_add(((op as i8) as i32 as u32) << 1);
                            cycles = 3;
                        }
                    }
                }
            }
            0b11100 => {
                let off = (((op32 & 0x7FF) << 21) as i32 >> 20) as u32;
                next = pc.wrapping_add(4).wrapping_add(off);
                cycles = 3;
            }
            _ => return self.execute32(bus, op, pc),
        }
        self.r[PC] = next;
        cycles
    }

    fn data_processing(&mut self, op: u16, rd: usize, rm: usize) {
        let a = self.r[rd];
        let b = self.r[rm];
        match (op >> 6) & 15 {
            0 => {
                self.r[rd] = a & b;
                self.set_nz(a & b);
            }
            1 => {
                self.r[rd] = a ^ b;
                self.set_nz(a ^ b);
            }
            2 => {
                let s = b & 0xFF;
                let res = if s == 0 {
                    a
                } else if s < 32 {
                    self.c = (a >> (32 - s)) & 1 != 0;
                    a << s
                } else {
                    self.c = s == 32 && a & 1 != 0;
                    0
                };
                self.r[rd] = res;
                self.set_nz(res);
            }
            3 => {
                let s = b & 0xFF;
                let res = if s == 0 {
                    a
                } else if s < 32 {
                    self.c = (a >> (s - 1)) & 1 != 0;
                    a >> s
                } else {
                    self.c = s == 32 && a & 0x8000_0000 != 0;
                    0
                };
                self.r[rd] = res;
                self.set_nz(res);
            }
            4 => {
                let s = b & 0xFF;
                let res = if s == 0 {
                    a
                } else if s < 32 {
                    self.c = (a >> (s - 1)) & 1 != 0;
                    ((a as i32) >> s) as u32
                } else {
                    self.c = a & 0x8000_0000 != 0;
                    ((a as i32) >> 31) as u32
                };
                self.r[rd] = res;
                self.set_nz(res);
            }
            5 => self.r[rd] = self.add_with_carry(a, b, self.c),
            6 => self.r[rd] = self.add_with_carry(a, !b, self.c),
            7 => {
                let s = b & 0xFF;
                let res = if s == 0 {
                    a
                } else {
                    let r = a.rotate_right(s & 31);
                    self.c = r & 0x8000_0000 != 0;
                    r
                };
                self.r[rd] = res;
                self.set_nz(res);
            }
            8 => self.set_nz(a & b),
            9 => self.r[rd] = self.add_with_carry(!b, 0, true),
            10 => {
                self.add_with_carry(a, !b, true);
            }
            11 => {
                self.add_with_carry(a, b, false);
            }
            12 => {
                self.r[rd] = a | b;
                self.set_nz(a | b);
            }
            13 => {
                let res = a.wrapping_mul(b);
                self.r[rd] = res;
                self.set_nz(res);
            }
            14 => {
                self.r[rd] = a & !b;
                self.set_nz(a & !b);
            }
            _ => {
                self.r[rd] = !b;
                self.set_nz(!b);
            }
        }
    }

    fn misc(&mut self, bus: &mut Bus, op: u16, pc: u32) -> u32 {
        let next = pc.wrapping_add(2);
        let rd = (op & 7) as usize;
        let rm = ((op >> 3) & 7) as usize;
        let mut cycles = 1;
        match (op >> 8) & 0xF {
            0x0 => {
                let imm = ((op & 0x7F) as u32) << 2;
                self.r[SP] = if op & 0x80 != 0 {
                    self.r[SP].wrapping_sub(imm)
                } else {
                    self.r[SP].wrapping_add(imm)
                };
            }
            0x2 => {
                let m = self.r[rm];
                self.r[rd] = match (op >> 6) & 3 {
                    0 => m as i16 as i32 as u32,
                    1 => m as i8 as i32 as u32,
                    2 => m & 0xFFFF,
                    _ => m & 0xFF,
                };
            }
            0x4 | 0x5 => {
                // PUSH
                let count = (op & 0x1FF).count_ones();
                let mut addr = self.r[SP].wrapping_sub(count * 4);
                self.r[SP] = addr;
                for i in 0..8 {
                    if op & (1 << i) != 0 {
                        bus.write32(addr, self.r[i]);
                        addr = addr.wrapping_add(4);
                    }
                }
                if op & 0x100 != 0 {
                    bus.write32(addr, self.r[LR]);
                }
                cycles = 1 + count;
            }
            0x6 => {
                if op & 0xFFEF == 0xB662 {
                    self.primask = op & 0x10 != 0;
                } else {
                    return self.fault(format!("undefined {op:#06x} at {pc:#06x}"));
                }
            }
            0xA => {
                let m = self.r[rm];
                self.r[rd] = match (op >> 6) & 3 {
                    0 => m.swap_bytes(),
                    1 => ((m & 0x00FF_00FF) << 8) | ((m >> 8) & 0x00FF_00FF),
                    3 => ((m as u16).swap_bytes()) as i16 as i32 as u32,
                    _ => return self.fault(format!("undefined {op:#06x} at {pc:#06x}")),
                };
            }
            0xC | 0xD => {
                // POP
                let mut addr = self.r[SP];
                for i in 0..8 {
                    if op & (1 << i) != 0 {
                        self.r[i] = bus.read32(addr);
                        addr = addr.wrapping_add(4);
                    }
                }
                cycles = 1 + (op & 0x1FF).count_ones();
                if op & 0x100 != 0 {
                    let target = bus.read32(addr);
                    self.r[SP] = addr.wrapping_add(4);
                    self.branch_exchange(bus, target);
                    return cycles + 3;
                }
                self.r[SP] = addr;
            }
            0xE => {
                self.halted = Some(Halt::Breakpoint(pc));
                return 1;
            }
            0xF => {
                if op & 0xF == 0 {
                    match (op >> 4) & 0xF {
                        2 | 3 => {
                            // WFE / WFI. WFE is treated like WFI (no event register).
                            self.r[PC] = next;
                            if !bus.scs.wakeup_pending() {
                                self.sleeping = true;
                                bus.on_sleep();
                            }
                            return 2;
                        }
                        _ => {}
                    }
                }
            }
            _ => return self.fault(format!("undefined {op:#06x} at {pc:#06x}")),
        }
        self.r[PC] = next;
        cycles
    }

    fn execute32(&mut self, bus: &mut Bus, h1: u16, pc: u32) -> u32 {
        let h2 = bus.fetch16(pc.wrapping_add(2));
        let next = pc.wrapping_add(4);
        if h1 & 0xF800 == 0xF000 && h2 & 0xD000 == 0xD000 {
            // BL
            let s = ((h1 >> 10) & 1) as u32;
            let j1 = ((h2 >> 13) & 1) as u32;
            let j2 = ((h2 >> 11) & 1) as u32;
            let i1 = (!(j1 ^ s)) & 1;
            let i2 = (!(j2 ^ s)) & 1;
            let imm = (s << 24) | (i1 << 23) | (i2 << 22) | (((h1 & 0x3FF) as u32) << 12) | (((h2 & 0x7FF) as u32) << 1);
            let off = ((imm << 7) as i32 >> 7) as u32;
            self.r[LR] = next | 1;
            self.r[PC] = next.wrapping_add(off);
            return 4;
        }
        if h1 & 0xFFF0 == 0xF380 && h2 & 0xFF00 == 0x8800 {
            // MSR
            let v = self.r[(h1 & 15) as usize];
            match h2 & 0xFF {
                0..=3 => {
                    self.n = v & 0x8000_0000 != 0;
                    self.z = v & 0x4000_0000 != 0;
                    self.c = v & 0x2000_0000 != 0;
                    self.v = v & 0x1000_0000 != 0;
                }
                8 => {
                    if self.uses_psp() {
                        self.other_sp = v & !3
                    } else {
                        self.r[SP] = v & !3
                    }
                }
                9 => {
                    if self.uses_psp() {
                        self.r[SP] = v & !3
                    } else {
                        self.other_sp = v & !3
                    }
                }
                16 => self.primask = v & 1 != 0,
                20
                    if self.ipsr == 0 => {
                        let new = v & 2 != 0;
                        if new != self.spsel {
                            std::mem::swap(&mut self.r[SP], &mut self.other_sp);
                            self.spsel = new;
                        }
                    }
                _ => {}
            }
            self.r[PC] = next;
            return 4;
        }
        if h1 == 0xF3EF && h2 & 0xF000 == 0x8000 {
            // MRS
            let d = ((h2 >> 8) & 15) as usize;
            self.r[d] = match h2 & 0xFF {
                0..=3 => self.xpsr() & 0xF000_0000 | if h2 & 1 != 0 { self.ipsr } else { 0 },
                5..=7 => self.ipsr,
                8 => if self.uses_psp() { self.other_sp } else { self.r[SP] },
                9 => if self.uses_psp() { self.r[SP] } else { self.other_sp },
                16 => self.primask as u32,
                20 => (self.spsel as u32) << 1,
                _ => 0,
            };
            self.r[PC] = next;
            return 4;
        }
        if h1 == 0xF3BF && h2 & 0xFF00 == 0x8F00 {
            // DMB / DSB / ISB
            self.r[PC] = next;
            return 4;
        }
        self.fault(format!("undefined 32-bit {h1:#06x} {h2:#06x} at {pc:#06x}"))
    }

    fn uses_psp(&self) -> bool {
        self.ipsr == 0 && self.spsel
    }

    fn condition(&self, cond: u16) -> bool {
        match cond {
            0 => self.z,
            1 => !self.z,
            2 => self.c,
            3 => !self.c,
            4 => self.n,
            5 => !self.n,
            6 => self.v,
            7 => !self.v,
            8 => self.c && !self.z,
            9 => !self.c || self.z,
            10 => self.n == self.v,
            11 => self.n != self.v,
            12 => !self.z && self.n == self.v,
            13 => self.z || self.n != self.v,
            _ => true,
        }
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
