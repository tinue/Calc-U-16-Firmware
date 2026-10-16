//! End-to-end tests: CPU self-test program and firmware scripts.
//!
//! Scripts live in tests/scripts/*.vs; their first line names the firmware image,
//! relative to the repository root: `# firmware: vendor/DM16_34.hex` (fetched with
//! tools/fetch-vendor-firmware.sh) or `# firmware: firmware/target/calcu16-fw.hex`
//! (built with firmware/build.sh).
//! Golden screens are in tests/scripts/golden/ (UPDATE_SNAPSHOTS=1 rewrites them).

use std::path::{Path, PathBuf};
use std::process::Command;
use voyager::cpu::Halt;
use voyager::frontend::script::Runner;
use voyager::models::Model;
use voyager::{Config, Machine};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fixed_config() -> Config {
    Config {
        rtc_base: chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap().and_hms_opt(8, 0, 0).unwrap(),
        ..Config::default()
    }
}

#[test]
fn cpu_selftest_program() {
    let dir = std::env::temp_dir().join(format!("voyager-selftest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = root().join("tests/asm/selftest.s");
    let (obj, elf, bin) = (dir.join("t.o"), dir.join("t.elf"), dir.join("t.bin"));
    let ok = |c: &mut Command| c.status().map(|s| s.success()).unwrap_or(false);
    if !ok(Command::new("arm-none-eabi-as").arg("--version").stdout(std::process::Stdio::null())) {
        eprintln!("arm-none-eabi-as not available: skipping");
        return;
    }
    assert!(ok(Command::new("arm-none-eabi-as").args(["-mcpu=cortex-m0", "-mthumb"]).arg(&src).arg("-o").arg(&obj)));
    assert!(ok(Command::new("arm-none-eabi-ld").arg("-Ttext=0").arg(&obj).arg("-o").arg(&elf)));
    assert!(ok(Command::new("arm-none-eabi-objcopy").args(["-O", "binary"]).arg(&elf).arg(&bin)));
    let mut flash = std::fs::read(&bin).unwrap();
    flash.resize(voyager::bus::FLASH_SIZE, 0xFF);
    let mut m = Machine::new(flash, &fixed_config());
    m.run_for(0.01);
    match m.halted() {
        Some(Halt::Breakpoint(pc)) => {
            let imm = m.bus.flash[*pc as usize];
            assert_eq!(imm, 0, "self-test check #{imm} failed at {pc:#x}");
        }
        other => panic!("self-test did not finish: {other:?} pc={:#x}", m.cpu.r[15]),
    }
}

fn run_script(name: &str) {
    let path = root().join("tests/scripts").join(name);
    let text = std::fs::read_to_string(&path).unwrap();
    let fw = text
        .lines()
        .find_map(|l| l.strip_prefix("# firmware:"))
        .map(|s| root().join("..").join(s.trim()))
        .expect("script needs a '# firmware:' line");
    if !fw.exists() {
        eprintln!(
            "{} not found (run tools/fetch-vendor-firmware.sh or firmware/build.sh): skipping {name}",
            fw.display()
        );
        return;
    }
    let mut m = Machine::from_hex(&fw, &fixed_config()).unwrap();
    let model = Model::detect(&m.bus.flash);
    let mut out = Vec::new();
    let failures = {
        let mut r = Runner::new(&mut m, model, Box::new(&mut out), &root().join("tests/scripts"));
        r.run_script(&text).unwrap()
    };
    let out = String::from_utf8_lossy(&out);
    assert_eq!(failures, 0, "{name} failed:\n{out}");
}

#[test]
fn dm16_arithmetic() {
    run_script("dm16_arithmetic.vs");
}

#[test]
fn dm16_console() {
    run_script("dm16_console.vs");
}

#[test]
fn dm16_setup() {
    run_script("dm16_setup.vs");
}

#[test]
fn dm16_power() {
    run_script("dm16_power.vs");
}

#[test]
fn dm16_bootloader() {
    run_script("dm16_bootloader.vs");
}

#[test]
fn dm16_mc0() {
    run_script("dm16_mc0.vs");
}

#[test]
fn dm15() {
    run_script("dm15.vs");
}

#[test]
fn dm12() {
    run_script("dm12.vs");
}

#[test]
fn fw_examples() {
    run_script("fw_examples.vs");
}

#[test]
fn fw_bases() {
    run_script("fw_bases.vs");
}

#[test]
fn fw_power_and_isp() {
    run_script("fw_power_and_isp.vs");
}

#[test]
fn fw_escape_reset() {
    run_script("fw_escape_reset.vs");
}

#[test]
fn fw_escape_on() {
    run_script("fw_escape_on.vs");
}
