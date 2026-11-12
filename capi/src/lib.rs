// SPDX-License-Identifier: MIT
//! The C interface to the `bof3player` library crates.
//!
//! The committed header is `capi/include/bof3player_capi.h`; `tests/header.rs`
//! generates it with cbindgen and fails when the two differ. Every fallible
//! function returns `bof3player_status`, and the error of the last failing call
//! is available through `bof3player_last_error_message`.
//!
//! The C library builds the workspace crates with `std` only: the tools, the
//! GUI, the audio device and Ogg are not part of it.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
// The C ABI names are the header's names; Rust's camel-case lint does not apply.
#![allow(non_camel_case_types)]

use core::ffi::c_char;
use std::ffi::CString;
use std::ptr;
use std::sync::{Arc, Mutex};

use ::emi::Archive as EmiArchive;
use bof3player::{Library, Player};
use psx_cd::Entry as DiscEntry;
use psx_formats::vab::Bank as VabBank;

mod args;
pub mod disc;
pub mod emi;
mod error;
pub mod library;
pub mod vab;

/// Status returned by every fallible C function.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum bof3player_status {
    /// The call succeeded.
    Ok = 0,
    /// A pointer, length or range the caller supplied is invalid.
    InvalidArgument = 1,
    /// An index is past the last item or entry.
    InvalidIndex = 2,
    /// The input is valid but uses a feature this library does not implement.
    Unsupported = 3,
    /// A file or stream could not be read or written.
    Io = 4,
    /// The bytes are malformed, truncated or not the format they claim.
    Format = 5,
    /// The call is not valid in the handle's current state.
    Usage = 6,
    /// A Rust panic was caught at the boundary.
    Panic = 7,
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

/// An owned UTF-8 string without a terminator.
///
/// Free it with `bof3player_string_free`. `data` is NULL when `len` is 0.
#[repr(C)]
pub struct bof3player_string {
    /// The bytes; NULL when `len` is 0.
    pub data: *mut c_char,
    /// The length in bytes.
    pub len: usize,
}

/// One catalog item's metadata.
///
/// The strings are owned; free the whole record with
/// `bof3player_item_info_free`.
#[repr(C)]
pub struct bof3player_item_info {
    /// The media kind.
    pub kind: bof3player_kind,
    /// One when `duration` is meaningful, zero otherwise.
    pub has_duration: u8,
    /// One when the item carries audio.
    pub playable: u8,
    /// Zero.
    pub reserved: [u8; 2],
    /// Seconds, meaningful when `has_duration` is one.
    pub duration: f64,
    /// The catalog id, for example `music:BGM000#0`.
    pub id: bof3player_string,
    /// The archive name.
    pub name: bof3player_string,
    /// The title from the names file, empty when none is known.
    pub title: bof3player_string,
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

/// An owned byte buffer. Free it with `bof3player_buffer_free`.
///
/// `data` is NULL when `len` is 0.
#[repr(C)]
pub struct bof3player_buffer {
    /// The bytes; NULL when `len` is 0.
    pub data: *mut u8,
    /// The length in bytes.
    pub len: usize,
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
    /// One for a directory.
    pub is_directory: u8,
    /// One when the entry is an XA stream.
    pub is_xa: u8,
    /// The extended attribute bytes.
    pub extended_attributes: u8,
    /// Zero.
    pub reserved: u8,
}

/// One entry of an EMI archive's table.
#[repr(C)]
pub struct bof3player_emi_entry {
    /// The payload size in bytes.
    pub size: u32,
    /// The `u32` at byte 4 of the table record.
    pub ram_ptr: u32,
    /// The `u32` at byte 8 of the table record.
    pub first4: u32,
    /// The entry type.
    pub file_type: u16,
    /// Zero.
    pub reserved: u16,
    /// The payload's byte offset in the container.
    pub offset: u64,
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
    /// The first looped sample, when `has_loop` is one.
    pub loop_start_frame: u32,
    /// One past the last looped sample, when `has_loop` is one.
    pub loop_end_frame: u32,
    /// One when an end flag was found.
    pub has_end: u8,
    /// One when the sample loops.
    pub has_loop: u8,
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

/// A parsed EMI archive and the container bytes its payloads live in.
///
/// Opaque. Free it with `bof3player_emi_free`.
pub struct bof3player_emi {
    pub(crate) archive: EmiArchive,
    pub(crate) bytes: Vec<u8>,
}

/// A parsed VAB bank and the body its samples live in.
///
/// Opaque. Free it with `bof3player_vab_free`.
pub struct bof3player_vab {
    pub(crate) bank: VabBank,
    pub(crate) body: Vec<u8>,
}

/// The C ABI version. Version 1 is the first.
#[unsafe(no_mangle)]
pub extern "C" fn bof3player_abi_version() -> u32 {
    1
}

/// The NUL-terminated message of the last failing call on this thread, or NULL
/// when that call succeeded.
///
/// The pointer is valid until the next `bof3player_` call on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn bof3player_last_error_message() -> *const c_char {
    error::last_error()
}

/// Free an owned string.
///
/// # Safety
///
/// `string` is NULL or points to a string returned by this library that has not
/// been freed. The function zeroes the string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_string_free(string: *mut bof3player_string) {
    if string.is_null() {
        return;
    }
    unsafe { free_string(&mut *string) };
}

/// Free an owned byte buffer.
///
/// # Safety
///
/// `buffer` is NULL or points to a buffer returned by this library that has not
/// been freed. The function zeroes the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_buffer_free(buffer: *mut bof3player_buffer) {
    if buffer.is_null() {
        return;
    }
    unsafe { free_buffer(&mut *buffer) };
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

/// Copy `text` into an owned string; empty text becomes a NULL pointer with
/// length 0.
pub(crate) fn owned_string(text: &str) -> bof3player_string {
    if text.is_empty() {
        return bof3player_string { data: ptr::null_mut(), len: 0 };
    }
    let text = text.replace('\0', " ");
    let text = CString::new(text).expect("NUL bytes are replaced");
    let len = text.as_bytes().len();
    bof3player_string { data: text.into_raw(), len }
}

/// Free a string and zero it.
pub(crate) fn free_string(string: &mut bof3player_string) {
    if !string.data.is_null() {
        drop(unsafe { CString::from_raw(string.data) });
    }
    string.data = ptr::null_mut();
    string.len = 0;
}

/// Copy bytes into an owned buffer; empty bytes become a NULL pointer with
/// length 0.
pub(crate) fn owned_buffer(bytes: Vec<u8>) -> bof3player_buffer {
    if bytes.is_empty() {
        return bof3player_buffer { data: ptr::null_mut(), len: 0 };
    }
    let bytes = bytes.into_boxed_slice();
    let len = bytes.len();
    bof3player_buffer { data: Box::into_raw(bytes).cast::<u8>(), len }
}

/// Free a buffer and zero it.
pub(crate) fn free_buffer(buffer: &mut bof3player_buffer) {
    if !buffer.data.is_null() {
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
    }
    buffer.data = ptr::null_mut();
    buffer.len = 0;
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
