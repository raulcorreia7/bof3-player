// SPDX-License-Identifier: MIT
//! The SPU as the code sees it: its registers, 512 KB of sound RAM filled by the transfer port and DMA channel 4, and a
//! log of key-on events (psx-spx "SPU Voice Registers", "SPU Control and Status Register", "SPU Memory Access").

/// Bytes of sound RAM.
pub const SPU_RAM_SIZE: usize = 512 * 1024;

const KON_LOW: usize = 0x188 / 2;
const KON_HIGH: usize = 0x18A / 2;
const KOFF_LOW: usize = 0x18C / 2;
const KOFF_HIGH: usize = 0x18E / 2;
const ENDX_LOW: usize = 0x19C / 2;
const ENDX_HIGH: usize = 0x19E / 2;
const TRANSFER_ADDRESS: usize = 0x1A6 / 2;
const TRANSFER_FIFO: usize = 0x1A8 / 2;
const CONTROL: usize = 0x1AA / 2;
const STATUS: usize = 0x1AE / 2;

/// A voice keyed on, with its registers at that moment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyOn {
    /// Vertical blanks counted before the key-on.
    pub frame: u64,
    /// The voice, 0 to 23.
    pub voice: u8,
    /// Sample start, bytes in sound RAM.
    pub start: u32,
    /// Loop address, bytes in sound RAM.
    pub repeat: u32,
    /// `VxPitch` (0x1000 is 44.1 kHz).
    pub pitch: u16,
    /// `ADSR1`.
    pub adsr1: u16,
    /// `ADSR2`.
    pub adsr2: u16,
    /// Left volume register.
    pub volume_left: u16,
    /// Right volume register.
    pub volume_right: u16,
}

/// A block written to sound RAM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Upload {
    /// Vertical blanks counted before the transfer.
    pub frame: u64,
    /// First byte written.
    pub address: u32,
    /// Bytes written.
    pub bytes: u32,
}

/// The SPU state.
pub struct Spu {
    regs: Vec<u16>,
    ram: Vec<u8>,
    transfer: u32,
    key_ons: Vec<KeyOn>,
    uploads: Vec<Upload>,
    /// The frame number stamped on new log entries.
    pub frame: u64,
}

impl Default for Spu {
    fn default() -> Self {
        Self::new()
    }
}

impl Spu {
    /// An SPU with cleared registers and sound RAM.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: vec![0; 0x200],
            ram: vec![0; SPU_RAM_SIZE],
            transfer: 0,
            key_ons: Vec::new(),
            uploads: Vec::new(),
            frame: 0,
        }
    }

    /// Sound RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Key-on events since the last [`Spu::take_key_ons`].
    #[must_use]
    pub fn key_ons(&self) -> &[KeyOn] {
        &self.key_ons
    }

    /// Returns the key-on log and starts a new one.
    pub fn take_key_ons(&mut self) -> Vec<KeyOn> {
        std::mem::take(&mut self.key_ons)
    }

    /// Writes to sound RAM, in order.
    #[must_use]
    pub fn uploads(&self) -> &[Upload] {
        &self.uploads
    }

    /// The halfword register at `offset` from 0x1F801C00.
    #[must_use]
    pub fn read(&self, offset: u32) -> u16 {
        let index = (offset as usize / 2) & 0x1FF;
        match index {
            STATUS => self.regs[CONTROL] & 0x3F,
            TRANSFER_FIFO => 0,
            _ => self.regs[index],
        }
    }

    /// Writes the halfword register at `offset` from 0x1F801C00.
    pub fn write(&mut self, offset: u32, value: u16) {
        let index = (offset as usize / 2) & 0x1FF;
        match index {
            KON_LOW | KON_HIGH => {
                let first = if index == KON_LOW { 0 } else { 16 };
                for bit in 0..16 {
                    let voice = first + bit;
                    if value & (1 << bit) != 0 && voice < 24 {
                        self.key_on(voice);
                    }
                }
                let endx = if index == KON_LOW { ENDX_LOW } else { ENDX_HIGH };
                self.regs[endx] &= !value;
                self.regs[index] = value;
            }
            KOFF_LOW | KOFF_HIGH => self.regs[index] = value,
            TRANSFER_ADDRESS => {
                self.regs[index] = value;
                self.transfer = u32::from(value) * 8;
            }
            TRANSFER_FIFO => self.write_ram(&value.to_le_bytes()),
            STATUS => {}
            _ => self.regs[index] = value,
        }
    }

    fn key_on(&mut self, voice: usize) {
        let reg = |offset: usize| self.regs[voice * 8 + offset];
        self.key_ons.push(KeyOn {
            frame: self.frame,
            voice: voice as u8,
            start: u32::from(reg(3)) * 8,
            repeat: u32::from(reg(7)) * 8,
            pitch: reg(2),
            adsr1: reg(4),
            adsr2: reg(5),
            volume_left: reg(0),
            volume_right: reg(1),
        });
    }

    /// Bytes from the transfer port or DMA, written at the transfer address.
    pub(crate) fn write_ram(&mut self, bytes: &[u8]) {
        let address = self.transfer;
        for &byte in bytes {
            self.ram[self.transfer as usize & (SPU_RAM_SIZE - 1)] = byte;
            self.transfer = (self.transfer + 1) & (SPU_RAM_SIZE as u32 - 1);
        }
        match self.uploads.last_mut() {
            Some(last) if last.address + last.bytes == address && last.frame == self.frame => {
                last.bytes += bytes.len() as u32;
            }
            _ => self.uploads.push(Upload { frame: self.frame, address, bytes: bytes.len() as u32 }),
        }
    }

    /// A word DMA channel 4 reads from sound RAM.
    pub(crate) fn read_word(&mut self) -> u32 {
        let mut word = [0; 4];
        for byte in &mut word {
            *byte = self.ram[self.transfer as usize & (SPU_RAM_SIZE - 1)];
            self.transfer = (self.transfer + 1) & (SPU_RAM_SIZE as u32 - 1);
        }
        u32::from_le_bytes(word)
    }
}
