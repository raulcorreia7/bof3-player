// SPDX-License-Identifier: MIT
#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bios;
pub mod bus;
pub mod cdrom;
pub mod cpu;
pub mod disc;
pub mod dma;
pub mod exe;
pub mod gpu;
pub mod gte;
pub mod irq;
mod machine;
pub mod mdec;
pub mod spu;
pub mod timers;

pub use bios::{Event, Kernel};
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
    /// The code called a kernel function the machine has no stand-in for.
    Bios {
        /// `0xA0`, `0xB0` or `0xC0`.
        table: u8,
        /// The function number.
        function: u8,
        /// The return address of the call.
        caller: u32,
    },
    /// The code jumped into kernel RAM where the machine has no routine.
    KernelPc {
        /// The address.
        pc: u32,
        /// `$ra` at that moment.
        caller: u32,
    },
    /// The CPU took an exception the kernel does not handle, such as a break or an address error.
    Exception {
        /// `CAUSE.ExcCode`.
        code: u8,
        /// The exception PC.
        epc: u32,
        /// `BadVaddr`, for an address error.
        address: u32,
    },
    /// A handler the kernel called did not return.
    GuestCall {
        /// The handler.
        function: u32,
    },
    /// The boot executable returned to the kernel.
    Returned,
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gte { command } => write!(f, "GTE command {command:#09x} is not implemented"),
            Self::Bios { table, function, caller } => {
                write!(f, "kernel function {table:02X}:{function:02X}h has no stand-in (called from {caller:#010x})")
            }
            Self::KernelPc { pc, caller } => write!(f, "jump into kernel RAM at {pc:#010x} (ra {caller:#010x})"),
            Self::Exception { code, epc, address } => {
                write!(f, "unhandled CPU exception {code} at {epc:#010x} (address {address:#010x})")
            }
            Self::GuestCall { function } => write!(f, "handler {function:#010x} did not return"),
            Self::Returned => write!(f, "the boot executable returned"),
        }
    }
}

impl std::error::Error for Halt {}

/// Why a machine could not be set up.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The disc image could not be read.
    Disc(psx_cd::Error),
    /// The boot configuration or the executable is missing or malformed.
    Boot(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disc(error) => write!(f, "disc: {error}"),
            Self::Boot(message) => write!(f, "boot: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Disc(error) => Some(error),
            Self::Boot(_) => None,
        }
    }
}

impl From<psx_cd::Error> for Error {
    fn from(error: psx_cd::Error) -> Self {
        Self::Disc(error)
    }
}
