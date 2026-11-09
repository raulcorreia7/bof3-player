// SPDX-License-Identifier: MIT
//! PlayStation audio for *Breath of Fire III*: VAB soundbanks, SEP sequences, CD-ROM XA streams and CD-DA, rendered
//! through an SPU-accurate voice engine.
//! ```no_run
//! use bof3player::{Library, MemorySource};
//!
//! # fn main() -> bof3player::Result<()> {
//! let emi_bytes: &[u8] = &[]; // BIN/BGM/BGM000.EMI, loaded by the engine
//! let mut source = MemorySource::new();
//! source.add("BIN/BGM/BGM000.EMI", emi_bytes);
//! let library = Library::open(source)?;
//! let item = library.catalog().find("music:BGM000#0").expect("item exists");
//! let mut player = library.player(item)?;
//! let mut pcm = vec![0.0f32; 1024 * player.channels() as usize];
//! let frames = player.render_f32(&mut pcm)?;
//! # let _ = frames;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod catalog;
#[cfg(feature = "cli")]
pub mod cli;
mod clock;
#[cfg(feature = "audio")]
pub mod deck;
pub mod dedup;
#[cfg(feature = "std")]
pub mod discover;
pub mod format;
#[cfg(feature = "gui")]
pub mod gui;
pub mod library;
#[cfg(feature = "names")]
pub mod names;
#[cfg(any(feature = "cli", feature = "gui"))]
pub mod options;
pub mod player;
#[cfg(feature = "std")]
pub mod render;
#[cfg(feature = "std")]
pub mod save;
#[cfg(feature = "audio")]
pub mod spectrum;
pub mod theme;

mod driver;
mod xa_cues;

pub use catalog::{Catalog, Item, Kind, Naming, NoNaming};
pub use library::{Library, PlayerOptions};
pub use player::Player;
pub use psx_audio::{Blob, CallbackSource, Error, MemorySource, Result, Source, error, export, source, spu};
#[cfg(feature = "std")]
pub use psx_audio::{DiscSource, FolderSource, ReaderBlob};

/// Version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
