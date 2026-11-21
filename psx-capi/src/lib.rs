// SPDX-License-Identifier: MIT
//! The C ABI plumbing shared by `bof3player-capi` and `rebof3-capi`.
//!
//! One status enum, one thread-local error slot, one panic guard, the owned byte and string
//! records with their free functions, the argument checks and the EMI handle. Both C interfaces
//! re-export these symbols, so a C program sees the same `psx_` names whichever it links.
//!
//! Ownership: every owned result has one `psx_<noun>_free` that takes a pointer to it, accepts
//! NULL and zeroes the record. A borrowed pointer is documented with its lifetime.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
// The C ABI names are the header's names.
#![allow(non_camel_case_types)]

mod args;
mod emi;
mod error;
mod memory;

pub use args::*;
pub use emi::*;
pub use error::*;
pub use memory::*;

/// The header text this crate generates, for a consumer to check its copy against.
pub const HEADER: &str = include_str!("../include/psx_capi.h");
