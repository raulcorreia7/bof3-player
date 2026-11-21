//! The PS-X EXE header (psx-spx "CDROM File Formats", "PS-X EXE"): a 2 KB header, then the code.

use crate::bytes::u32_le;
use crate::{Error, Result};

/// The magic at the start of the header.
pub const MAGIC: &[u8; 8] = b"PS-X EXE";
/// Bytes before the code.
pub const HEADER_SIZE: usize = 0x800;

/// A parsed executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Executable<'a> {
    /// Initial program counter.
    pub pc: u32,
    /// Initial global pointer.
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
    /// The code and data the header sizes.
    pub body: &'a [u8],
    file: &'a [u8],
}

impl<'a> Executable<'a> {
    /// Reads the header of `file`.
    ///
    /// # Errors
    ///
    /// [`Error::Exe`] when the magic is missing or the body is shorter than the header says.
    pub fn parse(file: &'a [u8]) -> Result<Self> {
        if file.len() < HEADER_SIZE || &file[..MAGIC.len()] != MAGIC {
            return Err(Error::Exe("not a PS-X EXE"));
        }
        let size = u32_le(file, 0x1C) as usize;
        let body = file.get(HEADER_SIZE..HEADER_SIZE + size).ok_or(Error::Exe("the body is shorter than the header says"))?;
        let word = |at| u32_le(file, at);
        Ok(Self { pc: word(0x10), gp: word(0x14), load_address: word(0x18), bss: word(0x28), bss_size: word(0x2C), stack: word(0x30), stack_size: word(0x34), body, file })
    }

    /// The `length` bytes of the file at RAM `address`, or `None` outside it.
    pub fn bytes_at(&self, address: u32, length: usize) -> Option<&'a [u8]> {
        let start = HEADER_SIZE + address.checked_sub(self.load_address)? as usize;
        self.file.get(start..start.checked_add(length)?)
    }

    /// The word at RAM `address`.
    pub fn word_at(&self, address: u32) -> Option<u32> {
        self.bytes_at(address, 4).map(|word| u32_le(word, 0))
    }
}
