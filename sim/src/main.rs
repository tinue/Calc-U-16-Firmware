use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use voyager::bus::Trace;
use voyager::cpu::disasm::disasm;
use voyager::frontend::glass;
use voyager::frontend::script::Runner;
use voyager::frontend::tui::{self, TuiOptions};
use voyager::models::Model;
use voyager::{Config, Machine};

#[derive(Parser)]
#[command(name = "voyager-sim", about = "SwissMicros Voyager (DM-L) hardware simulator: runs the vendor firmware on an LPC111x board model")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct Common {
    /// Firmware image (Intel HEX), e.g. ../vendor/DM16_34.hex
    firmware: PathBuf,
    /// Trace categories: periph,irq,i2c,lcd,iap,pins,all
    #[arg(long, default_value = "")]
    trace: String,
    /// Key legends: dm12, dm15, dm16 (default: detected from the firmware)
    #[arg(long)]
    model: Option<String>,
    /// Battery voltage in mV
    #[arg(long, default_value_t = 3000.0)]
    vbat: f64,
    /// Start the RTC with the "voltage low" flag set (as after a battery change)
    #[arg(long)]
    rtc_lost: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Interactive terminal UI; the UART appears as a pseudo-terminal.
    Run {
        #[command(flatten)]
        common: Common,
        /// Emulation speed: factor of real time, or "max"
        #[arg(long, default_value = "1")]
        speed: String,
        /// Machine state file, loaded at start (if present) and saved at exit.
        /// Default: state/<firmware>.state next to the current directory.
        #[arg(long)]
        state: Option<PathBuf>,
        /// Do not load or save machine state.
        #[arg(long)]
        no_state: bool,
        /// Also create a symlink to the pty, e.g. /tmp/voyager-tty
        #[arg(long)]
        pty_link: Option<PathBuf>,
        /// Do not ring the terminal bell for beeps
        #[arg(long)]
        mute: bool,
    },
    /// Run a key/serial script headless; exit code 1 if an expectation fails.
    Script {
        #[command(flatten)]
        common: Common,
        script: PathBuf,
        /// Start from a saved machine state.
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// Run headless for a while and print the screen, serial output and trace.
    Boot {
        #[command(flatten)]
        common: Common,
        /// Emulated seconds to run
        #[arg(long, default_value_t = 3.0)]
        secs: f64,
    },
    /// Render text-art screens (16 rows of '#'/'.', e.g. golden files) as photo-like SVGs.
    /// Each 16-row frame of an input becomes <out>/<name>[-n].svg.
    Render {
        inputs: Vec<PathBuf>,
        /// Output directory
        #[arg(long, short, default_value = ".")]
        out: PathBuf,
        /// Screen pixels per millimetre
        #[arg(long, default_value_t = 12.0)]
        scale: f64,
    },
    /// Disassemble a firmware address range (hex addresses).
    Dis { firmware: PathBuf, start: String, end: String },
}

fn load(c: &Common) -> Result<(Machine, Model)> {
    let cfg = Config { trace: Trace::parse(&c.trace), vbat_mv: c.vbat, rtc_power_lost: c.rtc_lost, ..Config::default() };
    let m = Machine::from_hex(&c.firmware, &cfg)?;
    let model = match &c.model {
        Some(s) => Model::parse(s).ok_or_else(|| anyhow!("unknown model {s}"))?,
        None => Model::detect(&m.bus.flash),
    };
    Ok((m, model))
}

fn parse_hex(s: &str) -> Result<u32> {
    Ok(u32::from_str_radix(s.trim_start_matches("0x"), 16)?)
}

fn default_state(fw: &Path) -> PathBuf {
    let stem = fw.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "firmware".into());
    PathBuf::from("state").join(format!("{stem}.state"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run { common, speed, state, no_state, pty_link, mute } => {
            let (mut m, model) = load(&common)?;
            let state_path = if no_state { None } else { Some(state.unwrap_or_else(|| default_state(&common.firmware))) };
            if let Some(p) = &state_path {
                if p.exists() {
                    match m.load_state(p) {
                        Ok(()) => eprintln!("loaded state from {}", p.display()),
                        Err(e) => eprintln!("warning: {e:#}; starting fresh"),
                    }
                }
            }
            let opts = TuiOptions {
                speed: tui::parse_speed(&speed)?,
                pty_link,
                state_path,
                mute,
                script_dir: std::env::current_dir()?,
            };
            tui::run(&mut m, model, opts)?;
        }
        Cmd::Script { common, script, state } => {
            let (mut m, model) = load(&common)?;
            if let Some(s) = state {
                m.load_state(&s)?;
            }
            let text = std::fs::read_to_string(&script)?;
            let dir = script.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let mut r = Runner::new(&mut m, model, Box::new(std::io::stdout()), &dir);
            let failures = r.run_script(&text)?;
            if failures > 0 {
                eprintln!("{failures} expectation(s) failed");
                std::process::exit(1);
            }
        }
        Cmd::Boot { common, secs } => {
            let (mut m, _) = load(&common)?;
            let start = std::time::Instant::now();
            m.run_for(secs);
            let wall = start.elapsed().as_secs_f64();
            for l in m.take_log() {
                println!("{l}");
            }
            println!("serial: {:?}", String::from_utf8_lossy(&m.serial_take()));
            println!("{}", m.screen_text());
            println!(
                "pc={:#06x} halted={:?} t={:.3}s instructions={} wall={:.3}s ({:.0} MIPS) clock={:.3}MHz sleeps={} (deep {}) beeps={}",
                m.cpu.r[15],
                m.halted(),
                m.now(),
                m.cpu.instret,
                wall,
                m.cpu.instret as f64 / wall / 1e6,
                m.bus.hz() / 1e6,
                m.bus.sleeps,
                m.bus.deep_sleeps,
                m.bus.beeper.beeps.len()
            );
            let sum = (0..8).fold(0u32, |a, i| {
                a.wrapping_add(u32::from_le_bytes(m.bus.flash[4 * i..4 * i + 4].try_into().unwrap()))
            });
            println!(
                "vector checksum: {}",
                if sum == 0 { "valid" } else { "not set (the boot ROM needs it; flash tools such as lpc21isp patch it)" }
            );
            if !m.bus.unknown_seen.is_empty() {
                let regs: Vec<String> = m.bus.unknown_seen.iter().map(|a| format!("{a:#010x}")).collect();
                println!("unmodelled registers accessed: {}", regs.join(" "));
            }
        }
        Cmd::Render { inputs, out, scale } => {
            std::fs::create_dir_all(&out)?;
            for input in &inputs {
                let frames = glass::parse_text_art(&std::fs::read_to_string(input)?);
                let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                for (i, px) in frames.iter().enumerate() {
                    let name = if frames.len() == 1 { format!("{stem}.svg") } else { format!("{stem}-{}.svg", i + 1) };
                    let path = out.join(name);
                    std::fs::write(&path, glass::svg(px, scale))?;
                    println!("{}", path.display());
                }
            }
        }
        Cmd::Dis { firmware, start, end } => {
            let text = std::fs::read_to_string(&firmware)?;
            let img = voyager::ihex::parse(&text, voyager::bus::FLASH_SIZE)?;
            let (mut pc, end) = (parse_hex(&start)?, parse_hex(&end)?);
            let h = |a: u32| u16::from_le_bytes([img[a as usize], img[a as usize + 1]]);
            while pc < end && (pc as usize) + 3 < img.len() {
                let (s, size, lit) = disasm(pc, h(pc), h(pc + 2));
                let extra = lit
                    .filter(|&a| (a as usize) + 4 <= img.len())
                    .map(|a| format!("  ; ={:#x}", u32::from_le_bytes(img[a as usize..a as usize + 4].try_into().unwrap())))
                    .unwrap_or_default();
                println!("{pc:06x}: {s}{extra}");
                pc += size;
            }
        }
    }
    Ok(())
}
