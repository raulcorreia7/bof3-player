// SPDX-License-Identifier: MIT
//! PS-X EXE files: a 2 KB header with the entry point, the global pointer, the load address and the stack, then the
//! code and data (psx-spx "CDROM File Formats", "PS-X EXE").

/// The magic at the start of the header.
pub const MAGIC: &[u8; 8] = b"PS-X EXE";
/// Bytes before the code.
pub const HEADER_SIZE: usize = 0x800;

/// A parsed executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Executable<'a> {
    /// Initial program counter.
    pub pc: u32,
    /// Initial global pointer (`$gp`).
    pub gp: u32,
    /// Where the code and data load.
    pub load_address: u32,
    /// Zero-filled area start.
    pub bss: u32,
    /// Zero-filled area size.
    pub bss_size: u32,
    /// Stack base; 0 means the stack the boot configuration names.
    pub stack: u32,
    /// Added to the stack base.
    pub stack_size: u32,
    /// The code and data.
    pub body: &'a [u8],
}

impl<'a> Executable<'a> {
    /// Parses `bytes`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, &'static str> {
        if bytes.len() < HEADER_SIZE || &bytes[..8] != MAGIC {
            return Err("not a PS-X EXE");
        }
        let word = |offset: usize| {
            u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
        };
        let size = word(0x1C) as usize;
        let body = bytes.get(HEADER_SIZE..HEADER_SIZE + size).ok_or("PS-X EXE body is shorter than its header says")?;
        Ok(Self {
            pc: word(0x10),
            gp: word(0x14),
            load_address: word(0x18),
            bss: word(0x28),
            bss_size: word(0x2C),
            stack: word(0x30),
            stack_size: word(0x34),
            body,
        })
    }
}

/// The boot settings of `SYSTEM.CNF`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemConfig {
    /// The boot file path below the disc root, such as `SLUS_004.22`.
    pub boot: String,
    /// The stack top, `STACK`.
    pub stack: Option<u32>,
}

impl SystemConfig {
    /// Parses the text of `SYSTEM.CNF`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim().to_ascii_uppercase().as_str() {
                "BOOT" => {
                    let path = value.split_once(':').map_or(value, |(_, path)| path);
                    let path = path.trim_start_matches('\\').trim_start_matches('/');
                    let path = path.split_once(';').map_or(path, |(path, _)| path);
                    config.boot = path.replace('\\', "/");
                }
                "STACK" => {
                    let digits = value.trim_start_matches("0x").trim_start_matches("0X");
                    config.stack = u32::from_str_radix(digits, 16).ok();
                }
                _ => {}
            }
        }
        config
    }
}
