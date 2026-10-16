//! Minimal Intel HEX loader producing a flat image starting at address 0.

use anyhow::{bail, Context, Result};

/// Parses Intel HEX text into a flat image. Unwritten bytes are 0xFF (erased flash).
pub fn parse(text: &str, size: usize) -> Result<Vec<u8>> {
    let mut image = vec![0xFFu8; size];
    let mut upper: u32 = 0;
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(hex) = line.strip_prefix(':') else {
            bail!("line {}: missing ':'", lineno + 1);
        };
        let bytes = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .with_context(|| format!("line {}: bad hex", lineno + 1))?;
        if bytes.len() < 5 || bytes.len() != bytes[0] as usize + 5 {
            bail!("line {}: bad record length", lineno + 1);
        }
        if bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0 {
            bail!("line {}: checksum mismatch", lineno + 1);
        }
        let len = bytes[0] as usize;
        let addr = u16::from_be_bytes([bytes[1], bytes[2]]) as u32;
        let data = &bytes[4..4 + len];
        match bytes[3] {
            0x00 => {
                let base = (upper + addr) as usize;
                if base + len > size {
                    bail!("line {}: data at {:#x} outside image", lineno + 1, base);
                }
                image[base..base + len].copy_from_slice(data);
            }
            0x01 => break,
            0x02 => upper = (u16::from_be_bytes([data[0], data[1]]) as u32) << 4,
            0x04 => upper = (u16::from_be_bytes([data[0], data[1]]) as u32) << 16,
            0x03 | 0x05 => {}
            t => bail!("line {}: unsupported record type {t:#04x}", lineno + 1),
        }
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_records() {
        let img = super::parse(":0400100001020304E2\n:00000001FF\n", 0x20).unwrap();
        assert_eq!(&img[0x10..0x14], &[1, 2, 3, 4]);
        assert_eq!(img[0], 0xFF);
    }
}
