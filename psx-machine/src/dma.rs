// SPDX-License-Identifier: MIT
//! The DMA controller: seven channels, `DPCR` and `DICR` (psx-spx "DMA Channels"). A transfer runs
//! to its end as soon as it starts, except that MDEC output waits until the decoder holds the
//! whole transfer.

use crate::bus::Bus;
use crate::irq::Line;

/// The channel numbers.
pub mod channel {
    /// MDEC input.
    pub const MDEC_IN: usize = 0;
    /// MDEC output.
    pub const MDEC_OUT: usize = 1;
    /// GPU: command lists and image data.
    pub const GPU: usize = 2;
    /// CD-ROM sector data.
    pub const CDROM: usize = 3;
    /// SPU RAM.
    pub const SPU: usize = 4;
    /// Expansion port.
    pub const PIO: usize = 5;
    /// Ordering-table clear.
    pub const OTC: usize = 6;
}

/// Linked-list nodes one GPU transfer follows before the machine treats the list as a loop.
const MAX_LIST_NODES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Default)]
struct Channel {
    address: u32,
    block: u32,
    control: u32,
}

/// The DMA registers.
#[derive(Clone, Debug)]
pub struct Dma {
    channels: [Channel; 7],
    control: u32,
    interrupt: u32,
    /// Transfers started, per channel.
    pub transfers: [u64; 7],
}

impl Default for Dma {
    fn default() -> Self {
        Self { channels: [Channel::default(); 7], control: 0x0765_4321, interrupt: 0, transfers: [0; 7] }
    }
}

impl Dma {
    /// Reads the register at `offset` from 0x1F801080.
    #[must_use]
    pub fn read(&self, offset: u32) -> u32 {
        let index = (offset >> 4) as usize;
        if index == 7 {
            return match offset & 0xF {
                0x0 => self.control,
                0x4 => self.interrupt,
                _ => 0,
            };
        }
        let channel = &self.channels[index];
        match offset & 0xF {
            0x0 => channel.address,
            0x4 => channel.block,
            0x8 => channel.control,
            _ => 0,
        }
    }

    fn update_master_flag(&mut self) -> bool {
        let before = self.interrupt & 0x8000_0000 != 0;
        let enabled = (self.interrupt >> 16) & 0x7F;
        let flags = (self.interrupt >> 24) & 0x7F;
        let master = self.interrupt & (1 << 15) != 0 || (self.interrupt & (1 << 23) != 0 && enabled & flags != 0);
        self.interrupt = (self.interrupt & 0x7FFF_FFFF) | (u32::from(master) << 31);
        master && !before
    }

    /// Words a block transfer on `index` moves.
    fn words(&self, index: usize) -> u32 {
        let channel = &self.channels[index];
        if (channel.control >> 9) & 3 == 0 {
            match channel.block & 0xFFFF {
                0 => 0x1_0000,
                count => count,
            }
        } else {
            (channel.block & 0xFFFF) * (channel.block >> 16)
        }
    }

    fn ready(&self, index: usize) -> bool {
        let control = self.channels[index].control;
        let sync = (control >> 9) & 3;
        let enabled = self.control & (8 << (4 * index)) != 0;
        enabled && control & (1 << 24) != 0 && (sync != 0 || control & (1 << 28) != 0)
    }
}

impl Bus {
    /// A write to the DMA register at `offset` from 0x1F801080, merged into the word for narrow
    /// writes.
    pub(crate) fn dma_write(&mut self, offset: u32, value: u32, mask: u32) {
        let word = offset & !3;
        let merged = (self.dma.read(word) & !mask) | (value & mask);
        let index = (word >> 4) as usize;
        if index == 7 {
            match word & 0xF {
                0x0 => {
                    self.dma.control = merged;
                    for channel in 0..7 {
                        self.start_dma(channel);
                    }
                }
                0x4 => {
                    let acknowledged = merged & 0x7F00_0000;
                    let kept = self.dma.interrupt & 0x7F00_0000 & !acknowledged;
                    self.dma.interrupt = (merged & 0x00FF_803F) | kept;
                    if self.dma.update_master_flag() {
                        self.irq.raise(Line::Dma);
                    }
                }
                _ => {}
            }
            return;
        }
        let channel = &mut self.dma.channels[index];
        match word & 0xF {
            0x0 => channel.address = merged & 0x00FF_FFFF,
            0x4 => channel.block = merged,
            0x8 => {
                channel.control = if index == channel::OTC { (merged & 0x5100_0000) | 0x2 } else { merged & 0x7177_0703 };
                self.start_dma(index);
            }
            _ => {}
        }
    }

    pub(crate) fn start_dma(&mut self, index: usize) {
        if !self.dma.ready(index) {
            return;
        }
        if index == channel::MDEC_OUT && self.mdec.output_len() < self.dma.words(index) as usize {
            return;
        }
        self.dma.transfers[index] += 1;
        let control = self.dma.channels[index].control;
        self.dma.channels[index].control = control & !(1 << 28);
        match (control >> 9) & 3 {
            2 => self.dma_list(index),
            sync => self.dma_block(index, sync),
        }
        self.dma.channels[index].control &= !(1 << 24);
        if self.dma.interrupt & (1 << (16 + index)) != 0 {
            self.dma.interrupt |= 1 << (24 + index);
        }
        if self.dma.update_master_flag() {
            self.irq.raise(Line::Dma);
        }
        if index == channel::MDEC_IN {
            self.start_dma(channel::MDEC_OUT);
        }
    }

    fn dma_block(&mut self, index: usize, sync: u32) {
        let channel = self.dma.channels[index];
        let words = self.dma.words(index);
        let from_ram = channel.control & 1 != 0;
        let step: u32 = if channel.control & 2 != 0 { 4u32.wrapping_neg() } else { 4 };
        let mut address = channel.address & 0x1F_FFFC;
        for remaining in (0..words).rev() {
            if index == channel::OTC {
                let value = if remaining == 0 { 0x00FF_FFFF } else { address.wrapping_sub(4) & 0x1F_FFFF };
                self.ram_store(address, value);
            } else if from_ram {
                let value = self.ram_load(address);
                self.dma_to_device(index, value);
            } else {
                let value = self.dma_from_device(index);
                self.ram_store(address, value);
            }
            address = address.wrapping_add(step) & 0x1F_FFFC;
        }
        if sync == 1 {
            self.dma.channels[index].address = address;
            self.dma.channels[index].block &= 0xFFFF;
        }
    }

    fn dma_list(&mut self, index: usize) {
        let mut address = self.dma.channels[index].address & 0x1F_FFFC;
        for _ in 0..MAX_LIST_NODES {
            let header = self.ram_load(address);
            for word in 1..=(header >> 24) {
                let value = self.ram_load(address.wrapping_add(4 * word) & 0x1F_FFFC);
                self.dma_to_device(index, value);
            }
            if header & 0x0080_0000 != 0 {
                self.dma.channels[index].address = 0x00FF_FFFF;
                return;
            }
            address = header & 0x1F_FFFC;
        }
        self.dma_list_overruns += 1;
    }

    fn ram_load(&self, address: u32) -> u32 {
        let at = (address & 0x1F_FFFC) as usize;
        u32::from_le_bytes([self.ram[at], self.ram[at + 1], self.ram[at + 2], self.ram[at + 3]])
    }

    fn ram_store(&mut self, address: u32, value: u32) {
        let at = (address & 0x1F_FFFC) as usize;
        self.ram[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
}
