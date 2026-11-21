// SPDX-License-Identifier: MIT
//! The PS-X EXE header and the boot settings of `SYSTEM.CNF`.

pub use psx_formats::exe::{Executable, HEADER_SIZE, MAGIC};

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
