// SPDX-License-Identifier: MIT
//! The C interface to the `bof3player` library crates.
//!
//! The committed header is `capi/include/bof3player_capi.h`; `tests/header.rs`
//! generates it with cbindgen and fails when the two differ. Every fallible
//! function returns `psx_status`, and the error of the last failing call
//! is available through `bof3player_last_error_message`.
//!
//! The C library builds the workspace crates with `std` only: the tools, the
//! GUI, the audio device and Ogg are not part of it.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
// The C ABI names are the header's names; Rust's camel-case lint does not apply.
#![allow(non_camel_case_types)]

use std::ptr;
use std::sync::{Arc, Mutex};

use bof3player::{Library, Player};
use psx_cd::Entry as DiscEntry;
use psx_formats::vab::Bank as VabBank;

pub mod audio;
pub mod disc;
mod error;
pub mod library;
pub mod machine;
pub mod vab;

pub use psx_capi::*;

/// The C ABI version; the header defines it too. Version 2 shares the `psx_` layer.
pub const BOF3PLAYER_ABI_VERSION: u32 = 3;

/// The C ABI version of the loaded library.
#[unsafe(no_mangle)]
pub extern "C" fn bof3player_abi_version() -> u32 {
    BOF3PLAYER_ABI_VERSION
}

/// The kind of media a catalog item is.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum bof3player_kind {
    /// A sequence played through its bank.
    Music = 0,
    /// A VAB sound bank or one of its cue records.
    Bank = 1,
    /// One raw sample of a bank.
    Sample = 2,
    /// Streaming XA audio.
    Xa = 3,
    /// A CD-DA track.
    Cdda = 4,
}

impl From<bof3player::Kind> for bof3player_kind {
    fn from(kind: bof3player::Kind) -> Self {
        match kind {
            bof3player::Kind::Music => Self::Music,
            bof3player::Kind::Bank => Self::Bank,
            bof3player::Kind::Sample => Self::Sample,
            bof3player::Kind::Xa => Self::Xa,
            bof3player::Kind::Cdda => Self::Cdda,
        }
    }
}

/// Which part of a track a player repeats.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum bof3player_loop {
    /// Play straight through once.
    Off = 0,
    /// Play the loop window this many extra times, then run on to the end.
    Times = 1,
    /// Never end.
    Forever = 2,
}

/// One catalog item's metadata.
///
/// The strings are owned; free the whole record with
/// `bof3player_item_info_free`.
#[repr(C)]
pub struct bof3player_item_info {
    /// The media kind.
    pub kind: bof3player_kind,
    /// True when `duration` is meaningful.
    pub has_duration: bool,
    /// True when the item carries audio.
    pub playable: bool,
    /// Seconds, meaningful when `has_duration` is true.
    pub duration: f64,
    /// The catalog id, for example `music:BGM000#0`.
    pub id: psx_string,
    /// The archive name.
    pub name: psx_string,
    /// The title from the names file, empty when none is known.
    pub title: psx_string,
}

/// Output shape of a player.
#[repr(C)]
pub struct bof3player_format {
    /// Samples per second.
    pub sample_rate: u32,
    /// Interleaved channels.
    pub channels: u32,
    /// Seconds.
    pub duration: f64,
}

/// Interleaved 16-bit PCM. Free the samples with `bof3player_pcm_free`.
///
/// `samples` holds `frames * channels` values and is NULL when `frames` is 0.
#[repr(C)]
pub struct bof3player_pcm {
    /// The interleaved samples; NULL when `frames` is 0.
    pub samples: *mut i16,
    /// The number of frames.
    pub frames: usize,
    /// The number of channels.
    pub channels: u32,
    /// The sample rate in Hz.
    pub sample_rate: u32,
}

/// One entry of a disc image's file table.
#[repr(C)]
pub struct bof3player_disc_entry {
    /// The logical block address.
    pub lba: u32,
    /// The size in bytes.
    pub size: u32,
    /// True for a directory.
    pub is_directory: bool,
    /// True when the entry is an XA stream.
    pub is_xa: bool,
    /// The extended attribute bytes.
    pub extended_attributes: u8,
}

/// One VAG sample of a VAB bank.
#[repr(C)]
pub struct bof3player_vag_info {
    /// The 1-based VAG number, as tones reference it.
    pub number: u32,
    /// The byte offset in the bank body.
    pub offset: u32,
    /// The byte size.
    pub size: u32,
    /// The samples a one-pass decode produces.
    pub frames: u32,
    /// The first looped sample, when `has_loop` is true.
    pub loop_start_frame: u32,
    /// One past the last looped sample, when `has_loop` is true.
    pub loop_end_frame: u32,
    /// True when an end flag was found.
    pub has_end: bool,
    /// True when the sample loops.
    pub has_loop: bool,
}

/// An RGBA image. Free the pixels with `bof3player_image_free`.
#[repr(C)]
pub struct bof3player_image {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// The pixels, four bytes per pixel, row-major.
    pub rgba: psx_buffer,
}

/// Why a machine run stopped.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum bof3player_stop {
    /// The program counter reached the requested address.
    Pc = 0,
    /// The requested number of frames ran.
    Frames = 1,
    /// The run condition held.
    Condition = 2,
    /// The instruction budget ran out first.
    Budget = 3,
    /// The machine halted; the last error holds the halt text.
    Halt = 4,
}

/// An opened catalog over a disc image or an extracted folder.
///
/// Opaque. Free it with `bof3player_library_free`.
pub struct bof3player_library {
    pub(crate) inner: Arc<Library<'static>>,
}

/// A player over one catalog item.
///
/// Opaque. It keeps its library alive, so either handle may be freed first.
/// Free it with `bof3player_player_free`.
pub struct bof3player_player {
    pub(crate) player: Mutex<Player<'static>>,
    /// Keeps the allocation the player borrows alive.
    pub(crate) _library: Arc<Library<'static>>,
}

/// An opened disc image with its cached file table.
///
/// Opaque. Free it with `bof3player_disc_free`.
pub struct bof3player_disc {
    pub(crate) image: Mutex<psx_cd::Image>,
    pub(crate) entries: Vec<DiscEntry>,
}

/// A parsed VAB bank and the body its samples live in.
///
/// Opaque. Free it with `bof3player_vab_free`.
pub struct bof3player_vab {
    pub(crate) bank: VabBank,
    pub(crate) body: Vec<u8>,
}

/// A booted PlayStation machine.
///
/// Opaque. Free it with `bof3player_machine_free`.
pub struct bof3player_machine {
    pub(crate) machine: Mutex<psx_machine::Machine>,
}

/// Free PCM samples.
///
/// # Safety
///
/// `pcm` is NULL or points to PCM returned by this library that has not been
/// freed. The function zeroes the record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_pcm_free(pcm: *mut bof3player_pcm) {
    if pcm.is_null() {
        return;
    }
    unsafe { free_pcm(&mut *pcm) };
}

/// Free an image's pixels.
///
/// # Safety
///
/// `image` is NULL or points to an image returned by this library that has not
/// been freed. The function zeroes the record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_image_free(image: *mut bof3player_image) {
    if image.is_null() {
        return;
    }
    let image = unsafe { &mut *image };
    free_buffer(&mut image.rgba);
    image.width = 0;
    image.height = 0;
}

/// Copy interleaved samples into an owned PCM record.
pub(crate) fn owned_pcm(samples: Vec<i16>, channels: u32, sample_rate: u32) -> bof3player_pcm {
    let frames = if channels == 0 { 0 } else { samples.len() / channels as usize };
    if samples.is_empty() {
        return bof3player_pcm { samples: ptr::null_mut(), frames, channels, sample_rate };
    }
    bof3player_pcm { samples: Box::into_raw(samples.into_boxed_slice()).cast::<i16>(), frames, channels, sample_rate }
}

/// Free PCM samples and zero the record.
pub(crate) fn free_pcm(pcm: &mut bof3player_pcm) {
    if !pcm.samples.is_null() {
        let len = pcm.frames.saturating_mul(pcm.channels as usize);
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(pcm.samples, len)) });
    }
    *pcm = bof3player_pcm { samples: ptr::null_mut(), frames: 0, channels: 0, sample_rate: 0 };
}
#[cfg(test)]
mod smoke;
