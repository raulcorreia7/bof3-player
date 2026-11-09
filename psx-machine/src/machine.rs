// SPDX-License-Identifier: MIT
//! The machine: the CPU, the bus and the run loop.

use crate::Halt;
use crate::bus::{Bus, physical};
use crate::cpu::Cpu;

/// Where a run stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    /// The program counter reached the requested address.
    Pc,
    /// The step budget ran out first.
    Budget,
}

/// A PlayStation: CPU, memory and devices.
pub struct Machine {
    cpu: Cpu,
    bus: Bus,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    /// A machine at power-on with cleared memory.
    #[must_use]
    pub fn new() -> Self {
        Self { cpu: Cpu::new(), bus: Bus::new() }
    }

    /// The CPU.
    #[must_use]
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// The CPU, writable.
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// The bus and the devices on it.
    #[must_use]
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// The bus, writable.
    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.bus
    }

    /// Main RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        self.bus.ram()
    }

    /// Copies `bytes` into RAM at `address` (any segment), wrapping at the 2 MB mirror.
    pub fn write_ram(&mut self, address: u32, bytes: &[u8]) {
        let ram = self.bus.ram_mut();
        let base = physical(address) as usize;
        for (offset, byte) in bytes.iter().enumerate() {
            ram[(base + offset) & (ram.len() - 1)] = *byte;
        }
    }

    /// Copies `words` into RAM at `address`, little-endian.
    pub fn write_words(&mut self, address: u32, words: &[u32]) {
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        self.write_ram(address, &bytes);
    }

    /// Runs one instruction.
    ///
    /// # Errors
    ///
    /// The [`Halt`] that stopped the CPU.
    pub fn step(&mut self) -> Result<(), Halt> {
        self.cpu.step(&mut self.bus)?;
        self.bus.cycles += 2;
        Ok(())
    }

    /// Runs until the next instruction is at `pc`, or `budget` instructions have run.
    ///
    /// # Errors
    ///
    /// The [`Halt`] that stopped the CPU.
    pub fn run_until_pc(&mut self, pc: u32, budget: u64) -> Result<Stop, Halt> {
        for _ in 0..budget {
            if self.cpu.pc() == pc {
                return Ok(Stop::Pc);
            }
            self.step()?;
        }
        Ok(if self.cpu.pc() == pc { Stop::Pc } else { Stop::Budget })
    }
}
