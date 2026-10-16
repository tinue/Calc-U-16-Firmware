//! Compact ARMv6-M Thumb disassembler (for traces and the `dis` subcommand).

fn reg(n: u32) -> &'static str {
    ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr", "pc"][n as usize & 15]
}

fn rlist(bits: u32, extra: Option<&str>) -> String {
    let mut v: Vec<String> = (0..8).filter(|i| bits & (1 << i) != 0).map(|i| reg(i).to_string()).collect();
    if let Some(e) = extra {
        v.push(e.to_string());
    }
    format!("{{{}}}", v.join(", "))
}

const COND: [&str; 16] = ["eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "", ""];

/// Disassembles the instruction at `pc`. Returns (text, size in bytes, literal address if any).
pub fn disasm(pc: u32, h1: u16, h2: u16) -> (String, u32, Option<u32>) {
    let op = h1 as u32;
    let rd = op & 7;
    let rn = (op >> 3) & 7;
    let mut lit = None;
    let s = match op >> 11 {
        0 if (op >> 6) & 31 == 0 => format!("movs {}, {}", reg(rd), reg(rn)),
        0..=2 => format!("{} {}, {}, #{}", ["lsls", "lsrs", "asrs"][(op >> 11) as usize], reg(rd), reg(rn), (op >> 6) & 31),
        3 => {
            let m = if op & 0x400 != 0 { format!("#{}", (op >> 6) & 7) } else { reg((op >> 6) & 7).to_string() };
            format!("{} {}, {}, {}", if op & 0x200 != 0 { "subs" } else { "adds" }, reg(rd), reg(rn), m)
        }
        4..=7 => format!("{} {}, #{:#x}", ["movs", "cmp", "adds", "subs"][((op >> 11) & 3) as usize], reg((op >> 8) & 7), op & 0xFF),
        8 if op & 0x400 == 0 => {
            const N: [&str; 16] = ["ands", "eors", "lsls", "lsrs", "asrs", "adcs", "sbcs", "rors", "tst", "rsbs", "cmp", "cmn", "orrs", "muls", "bics", "mvns"];
            let o = (op >> 6) & 15;
            if o == 9 {
                format!("rsbs {}, {}, #0", reg(rd), reg(rn))
            } else {
                format!("{} {}, {}", N[o as usize], reg(rd), reg(rn))
            }
        }
        8 => {
            let dn = rd | ((op >> 4) & 8);
            let m = (op >> 3) & 15;
            match (op >> 8) & 3 {
                0 => format!("add {}, {}", reg(dn), reg(m)),
                1 => format!("cmp {}, {}", reg(dn), reg(m)),
                2 => format!("mov {}, {}", reg(dn), reg(m)),
                _ => format!("{} {}", if op & 0x80 != 0 { "blx" } else { "bx" }, reg(m)),
            }
        }
        9 => {
            let a = (pc.wrapping_add(4) & !3) + ((op & 0xFF) << 2);
            lit = Some(a);
            format!("ldr {}, [pc, #{:#x}]", reg((op >> 8) & 7), (op & 0xFF) << 2)
        }
        10 | 11 => {
            const N: [&str; 8] = ["str", "strh", "strb", "ldrsb", "ldr", "ldrh", "ldrb", "ldrsh"];
            format!("{} {}, [{}, {}]", N[((op >> 9) & 7) as usize], reg(rd), reg(rn), reg((op >> 6) & 7))
        }
        12..=17 => {
            let (n, sc) = match op >> 11 {
                12 => ("str", 4),
                13 => ("ldr", 4),
                14 => ("strb", 1),
                15 => ("ldrb", 1),
                16 => ("strh", 2),
                _ => ("ldrh", 2),
            };
            format!("{} {}, [{}, #{:#x}]", n, reg(rd), reg(rn), ((op >> 6) & 31) * sc)
        }
        18 | 19 => format!("{} {}, [sp, #{:#x}]", if op & 0x800 != 0 { "ldr" } else { "str" }, reg((op >> 8) & 7), (op & 0xFF) << 2),
        20 => format!("adr {}, {:#x}", reg((op >> 8) & 7), (pc.wrapping_add(4) & !3) + ((op & 0xFF) << 2)),
        21 => format!("add {}, sp, #{:#x}", reg((op >> 8) & 7), (op & 0xFF) << 2),
        22 | 23 => match (op >> 8) & 15 {
            0 => format!("{} sp, #{:#x}", if op & 0x80 != 0 { "sub" } else { "add" }, (op & 0x7F) << 2),
            2 => format!("{} {}, {}", ["sxth", "sxtb", "uxth", "uxtb"][((op >> 6) & 3) as usize], reg(rd), reg(rn)),
            4 | 5 => format!("push {}", rlist(op, (op & 0x100 != 0).then_some("lr"))),
            6 => format!("cps{} i", if op & 0x10 != 0 { "id" } else { "ie" }),
            0xA => format!("{} {}, {}", ["rev", "rev16", "?", "revsh"][((op >> 6) & 3) as usize], reg(rd), reg(rn)),
            0xC | 0xD => format!("pop {}", rlist(op, (op & 0x100 != 0).then_some("pc"))),
            0xE => format!("bkpt #{:#x}", op & 0xFF),
            0xF => ["nop", "yield", "wfe", "wfi", "sev"].get(((op >> 4) & 15) as usize).unwrap_or(&"hint?").to_string(),
            _ => format!(".short {op:#06x}"),
        },
        24 => format!("stm {}!, {}", reg((op >> 8) & 7), rlist(op, None)),
        25 => format!("ldm {}!, {}", reg((op >> 8) & 7), rlist(op, None)),
        26 | 27 => match (op >> 8) & 15 {
            14 => format!("udf #{}", op & 0xFF),
            15 => format!("svc #{}", op & 0xFF),
            c => format!("b{} {:#x}", COND[c as usize], pc.wrapping_add(4).wrapping_add(((op as u8 as i8) as i32 as u32) << 1)),
        },
        28 => format!("b {:#x}", pc.wrapping_add(4).wrapping_add((((op & 0x7FF) << 21) as i32 >> 20) as u32)),
        _ => {
            let h2 = h2 as u32;
            if op & 0xF800 == 0xF000 && h2 & 0xD000 == 0xD000 {
                let s = (op >> 10) & 1;
                let i1 = !(((h2 >> 13) & 1) ^ s) & 1;
                let i2 = !(((h2 >> 11) & 1) ^ s) & 1;
                let imm = (s << 24) | (i1 << 23) | (i2 << 22) | ((op & 0x3FF) << 12) | ((h2 & 0x7FF) << 1);
                let off = ((imm << 7) as i32 >> 7) as u32;
                return (format!("bl {:#x}", pc.wrapping_add(4).wrapping_add(off)), 4, None);
            }
            if op & 0xFFF0 == 0xF380 {
                return (format!("msr {:#x}, {}", h2 & 0xFF, reg(op & 15)), 4, None);
            }
            if op == 0xF3EF {
                return (format!("mrs {}, {:#x}", reg((h2 >> 8) & 15), h2 & 0xFF), 4, None);
            }
            if op == 0xF3BF {
                return (["dsb", "dmb", "isb"].get(((h2 >> 4) & 15).wrapping_sub(4) as usize).unwrap_or(&"barrier").to_string(), 4, None);
            }
            return (format!(".word {op:#06x} {h2:#06x}"), 4, None);
        }
    };
    (s, 2, lit)
}
