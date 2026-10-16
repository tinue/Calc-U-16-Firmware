//! Instruction-level tests using hand-assembled Thumb code.

use super::*;
use crate::board::rtc::Rtc;
use crate::bus::{Bus, FLASH_SIZE};

/// Builds a machine whose flash holds a vector table and `code` at 0x100.
pub(crate) fn run_code(code: &[u16], max_steps: usize) -> (Cpu, Bus) {
    let mut flash = vec![0xFFu8; FLASH_SIZE];
    flash[0..4].copy_from_slice(&0x1000_1FF0u32.to_le_bytes());
    flash[4..8].copy_from_slice(&0x101u32.to_le_bytes());
    for (i, h) in code.iter().enumerate() {
        flash[0x100 + 2 * i..0x102 + 2 * i].copy_from_slice(&h.to_le_bytes());
    }
    let base = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
    let mut bus = Bus::new(flash, Rtc::new(base, false));
    let mut cpu = Cpu::new();
    cpu.reset(&mut bus);
    for _ in 0..max_steps {
        if cpu.halted.is_some() {
            break;
        }
        bus.cycles += cpu.step(&mut bus) as u64;
    }
    (cpu, bus)
}

#[test]
fn arithmetic_and_flags() {
    // movs r0,#5; movs r1,#7; subs r2,r0,r1; adds r3,r0,r1; bkpt
    let (cpu, _) = run_code(&[0x2005, 0x2107, 0x1A42, 0x1843, 0xBE00], 10);
    assert_eq!(cpu.r[2], (-2i32) as u32);
    assert_eq!(cpu.r[3], 12);
    assert!(!cpu.n && !cpu.z && !cpu.c);
    assert_eq!(cpu.halted, Some(Halt::Breakpoint(0x108)));
}

#[test]
fn push_pop_and_bl() {
    // 0x100: bl 0x108 ; bkpt
    // 0x108: push {r4,lr}; movs r4,#9; mov r0,r4; pop {r4,pc}
    let (cpu, _) = run_code(&[0xF000, 0xF802, 0xBE00, 0xBF00, 0xB510, 0x2409, 0x0020, 0xBD10], 20);
    assert_eq!(cpu.r[0], 9);
    assert_eq!(cpu.r[SP], 0x1000_1FF0);
    assert_eq!(cpu.halted, Some(Halt::Breakpoint(0x104)));
}
