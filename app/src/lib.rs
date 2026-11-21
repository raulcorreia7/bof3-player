// SPDX-License-Identifier: MIT
//! The player's command line, terminal browser and window over the `bof3player` library.

pub mod cli;
pub mod deck;
pub mod discover;
#[cfg(feature = "gui")]
pub mod gui;
pub mod options;
pub mod spectrum;
pub mod theme;
