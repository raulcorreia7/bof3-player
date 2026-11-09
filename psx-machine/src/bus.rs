// SPDX-License-Identifier: MIT
//! The memory map: 2 MB of RAM and its mirrors, the 1 KB scratchpad, the I/O ports, the (empty)
//! BIOS region and the cache control register, reached through KUSEG, KSEG0 and KSEG1
//! (psx-spx "Memory Map").

use crate::irq::Irq;

/// Bytes of main RAM.
pub const RAM_SIZE: usize = 2 * 1024 * 1024;
/// Bytes of the data-cache scratchpad.
pub const SCRATCHPAD_SIZE: usize = 1024;

const RAM_WINDOW: u32 = 0x0080_0000;
const EXPANSION1: u32 = 0x1F00_0000;
const SCRATCHPAD: u32 = 0x1F80_0000;
const IO: u32 = 0x1F80_1000;
const IO_END: u32 = 0x1F80_3000;
const BIOS: u32 = 0x1FC0_0000;
const BIOS_END: u32 = 0x1FC8_0000;
const CACHE_CONTROL: u32 = 0xFFFE_0130;

/// Width of one bus access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Width {
    /// 8 bits.
    Byte,
    /// 16 bits.
    Half,
    /// 32 bits.
    Word,
}

/// Physical address of a CPU address: KSEG2 stays as it is, the other segments drop their top
/// three bits.
#[must_use]
pub fn physical(address: u32) -> u32 {
    if address >= 0xC000_0000 { address } else { address & 0x1FFF_FFFF }
}

/// Everything the CPU reaches through loads and stores.
pub struct Bus {
    pub(crate) ram: Vec<u8>,
    pub(crate) scratchpad: Vec<u8>,
    /// Interrupt status and mask.
    pub irq: Irq,
    io: Vec<u8>,
    cache_control: u32,
    /// CPU cycles run since power-on.
    pub cycles: u64,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    /// A bus with cleared RAM and no device state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ram: vec![0; RAM_SIZE],
            scratchpad: vec![0; SCRATCHPAD_SIZE],
            irq: Irq::default(),
            io: vec![0; (IO_END - IO) as usize],
            cache_control: 0,
            cycles: 0,
        }
    }

    /// Main RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Main RAM, writable.
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// The scratchpad.
    #[must_use]
    pub fn scratchpad(&self) -> &[u8] {
        &self.scratchpad
    }

    /// A word of RAM or scratchpad without side effects; `None` elsewhere.
    #[must_use]
    pub fn peek32(&self, address: u32) -> Option<u32> {
        let p = physical(address);
        if p < RAM_WINDOW {
            let at = (p as usize) & (RAM_SIZE - 1) & !3;
            return Some(u32::from_le_bytes(self.ram[at..at + 4].try_into().ok()?));
        }
        if (SCRATCHPAD..SCRATCHPAD + SCRATCHPAD_SIZE as u32).contains(&p) {
            let at = ((p - SCRATCHPAD) as usize) & !3;
            return Some(u32::from_le_bytes(self.scratchpad[at..at + 4].try_into().ok()?));
        }
        None
    }

    /// Reads a byte.
    pub fn read8(&mut self, address: u32) -> u8 {
        self.read(address, Width::Byte) as u8
    }

    /// Reads a halfword; the address is aligned.
    pub fn read16(&mut self, address: u32) -> u16 {
        self.read(address, Width::Half) as u16
    }

    /// Reads a word; the address is aligned.
    pub fn read32(&mut self, address: u32) -> u32 {
        self.read(address, Width::Word)
    }

    /// Writes a byte.
    pub fn write8(&mut self, address: u32, value: u8) {
        self.write(address, u32::from(value), Width::Byte);
    }

    /// Writes a halfword; the address is aligned.
    pub fn write16(&mut self, address: u32, value: u16) {
        self.write(address, u32::from(value), Width::Half);
    }

    /// Writes a word; the address is aligned.
    pub fn write32(&mut self, address: u32, value: u32) {
        self.write(address, value, Width::Word);
    }

    fn read(&mut self, address: u32, width: Width) -> u32 {
        let p = physical(address);
        if p < RAM_WINDOW {
            return read_slice(&self.ram, (p as usize) & (RAM_SIZE - 1), width);
        }
        if (SCRATCHPAD..SCRATCHPAD + SCRATCHPAD_SIZE as u32).contains(&p) {
            return read_slice(&self.scratchpad, (p - SCRATCHPAD) as usize, width);
        }
        if (IO..IO_END).contains(&p) {
            return self.read_io(p, width);
        }
        if (EXPANSION1..SCRATCHPAD).contains(&p) {
            // No expansion device: the bus floats high.
            return match width {
                Width::Byte => 0xFF,
                Width::Half => 0xFFFF,
                Width::Word => 0xFFFF_FFFF,
            };
        }
        if (BIOS..BIOS_END).contains(&p) {
            return 0;
        }
        if p == CACHE_CONTROL {
            return self.cache_control;
        }
        0
    }

    fn write(&mut self, address: u32, value: u32, width: Width) {
        let p = physical(address);
        if p < RAM_WINDOW {
            write_slice(&mut self.ram, (p as usize) & (RAM_SIZE - 1), value, width);
        } else if (SCRATCHPAD..SCRATCHPAD + SCRATCHPAD_SIZE as u32).contains(&p) {
            write_slice(&mut self.scratchpad, (p - SCRATCHPAD) as usize, value, width);
        } else if (IO..IO_END).contains(&p) {
            self.write_io(p, value, width);
        } else if p == CACHE_CONTROL {
            self.cache_control = value;
        }
    }

    fn read_io(&mut self, p: u32, width: Width) -> u32 {
        match p {
            0x1F80_1070 => u32::from(self.irq.status()),
            0x1F80_1074 => u32::from(self.irq.mask()),
            _ => read_slice(&self.io, (p - IO) as usize, width),
        }
    }

    fn write_io(&mut self, p: u32, value: u32, width: Width) {
        match p {
            0x1F80_1070 => self.irq.acknowledge(value as u16),
            0x1F80_1074 => self.irq.set_mask(value as u16),
            _ => write_slice(&mut self.io, (p - IO) as usize, value, width),
        }
    }
}

fn read_slice(bytes: &[u8], at: usize, width: Width) -> u32 {
    match width {
        Width::Byte => u32::from(bytes[at]),
        Width::Half => u32::from(u16::from_le_bytes([bytes[at], bytes[at + 1]])),
        Width::Word => u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]),
    }
}

fn write_slice(bytes: &mut [u8], at: usize, value: u32, width: Width) {
    match width {
        Width::Byte => bytes[at] = value as u8,
        Width::Half => bytes[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes()),
        Width::Word => bytes[at..at + 4].copy_from_slice(&value.to_le_bytes()),
    }
}
