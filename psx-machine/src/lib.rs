// SPDX-License-Identifier: MIT
#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bus;
pub mod cpu;
pub mod dma;
pub mod gpu;
pub mod gte;
pub mod irq;
mod machine;

pub use bus::Bus;
pub use cpu::Cpu;
pub use machine::{Machine, Stop};

use std::fmt;

/// Why the machine stopped before the requested point.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Halt {
    /// The code ran a GTE command, which the machine does not implement yet.
    Gte {
        /// The command field of the `COP2` instruction.
        command: u32,
    },
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gte { command } => write!(f, "GTE command {command:#09x} is not implemented"),
        }
    }
}

impl std::error::Error for Halt {}
