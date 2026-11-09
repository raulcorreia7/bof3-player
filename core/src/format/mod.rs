// SPDX-License-Identifier: MIT
//! Parsers for the on-disc formats.

pub mod emi;

pub use psx_formats::vab;

pub(crate) use psx_formats::{VAB_SAMPLE_RATE, sep, wav, xa};
