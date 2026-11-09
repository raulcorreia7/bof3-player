// SPDX-License-Identifier: MIT
//! The geometry transformation engine (coprocessor 2): its 32 data and 32 control registers.
//! The commands are not implemented yet; one halts the machine with [`Halt::Gte`].

use crate::Halt;

/// GTE register file.
#[derive(Clone, Debug, Default)]
pub struct Gte {
    data: [u32; 32],
    control: [u32; 32],
}

impl Gte {
    /// Data register `index`, as `MFC2` and `SWC2` read it.
    #[must_use]
    pub fn read_data(&self, index: usize) -> u32 {
        self.data[index]
    }

    /// Writes data register `index`, as `MTC2` and `LWC2` do.
    pub fn write_data(&mut self, index: usize, value: u32) {
        self.data[index] = value;
    }

    /// Control register `index`, as `CFC2` reads it.
    #[must_use]
    pub fn read_control(&self, index: usize) -> u32 {
        self.control[index]
    }

    /// Writes control register `index`, as `CTC2` does.
    pub fn write_control(&mut self, index: usize, value: u32) {
        self.control[index] = value;
    }

    /// Runs the command in the low 25 bits of a `COP2` instruction.
    ///
    /// # Errors
    ///
    /// Always [`Halt::Gte`]: the commands come with the GTE milestone.
    pub fn command(&mut self, command: u32) -> Result<(), Halt> {
        Err(Halt::Gte { command })
    }
}
