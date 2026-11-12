// SPDX-License-Identifier: MIT
//! VAB banks: parse one-file banks, list their VAGs and decode a sample.

use std::ptr;

use psx_formats::adpcm;
use psx_formats::vab::{self, Bank};

use crate::args::bytes_arg;
use crate::error::{Fail, fail, guard, map_formats};
use crate::{bof3player_pcm, bof3player_status, bof3player_vab, bof3player_vag_info, owned_pcm};

/// A VAG record with zeroed fields.
fn empty_vag() -> bof3player_vag_info {
    bof3player_vag_info { number: 0, offset: 0, size: 0, frames: 0, loop_start_frame: 0, loop_end_frame: 0, has_end: 0, has_loop: 0 }
}

/// Parse a one-file VAB bank (header followed by body).
///
/// The body is copied into the handle for decoding.
///
/// # Safety
///
/// `vab` points to `vab_len` readable bytes (or is NULL with length 0), and
/// `out_bank` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_vab_parse(vab: *const u8, vab_len: usize, out_bank: *mut *mut bof3player_vab) -> bof3player_status {
    if out_bank.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_bank is NULL");
    }
    unsafe { *out_bank = ptr::null_mut() };
    guard(|| {
        let bytes = unsafe { bytes_arg(vab, vab_len)? };
        let (header, body) = vab::file::split(bytes).map_err(map_formats)?;
        let bank = Bank::parse(header, body).map_err(map_formats)?;
        let handle = Box::new(bof3player_vab { bank, body: body.to_vec() });
        unsafe { *out_bank = Box::into_raw(handle) };
        Ok(())
    })
}

/// Free a bank handle.
///
/// # Safety
///
/// `bank` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_vab_free(bank: *mut bof3player_vab) {
    if bank.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(bank) });
}

/// The number of VAG entries, including the null sample at index 0.
///
/// # Safety
///
/// `bank` is NULL or a live handle, and `out_count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_vab_vag_count(bank: *const bof3player_vab, out_count: *mut usize) -> bof3player_status {
    if out_count.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_count is NULL");
    }
    unsafe { *out_count = 0 };
    if bank.is_null() {
        return fail(bof3player_status::InvalidArgument, "bank is NULL");
    }
    guard(|| {
        let bank = unsafe { &*bank };
        unsafe { *out_count = bank.bank.vags.len() };
        Ok(())
    })
}

/// One VAG's range and loop points.
///
/// `index` is the 0-based position; `number` in the record is the 1-based VAG
/// number the tones reference.
///
/// # Safety
///
/// `bank` is NULL or a live handle, and `out_info` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_vab_vag_at(bank: *const bof3player_vab, index: usize, out_info: *mut bof3player_vag_info) -> bof3player_status {
    if out_info.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_info is NULL");
    }
    unsafe { *out_info = empty_vag() };
    if bank.is_null() {
        return fail(bof3player_status::InvalidArgument, "bank is NULL");
    }
    guard(|| {
        let bank = unsafe { &*bank };
        let vag = bank.bank.vags.get(index).ok_or_else(|| Fail::new(bof3player_status::InvalidIndex, format!("VAG index {index} is out of range (the bank has {} entries)", bank.bank.vags.len())))?;
        unsafe {
            *out_info = bof3player_vag_info {
                number: vag.index,
                offset: vag.offset,
                size: vag.size,
                frames: vag.frames,
                loop_start_frame: vag.loop_start_frame,
                loop_end_frame: vag.loop_end_frame,
                has_end: u8::from(vag.has_end),
                has_loop: u8::from(vag.has_loop),
            };
        }
        Ok(())
    })
}

/// Decode one VAG to mono 44,100 Hz PCM.
///
/// The null sample (index 0) decodes to 0 frames without an error. Free the
/// samples with `bof3player_pcm_free`.
///
/// # Safety
///
/// `bank` is NULL or a live handle, and `out_pcm` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_vab_vag_decode(bank: *const bof3player_vab, index: usize, out_pcm: *mut bof3player_pcm) -> bof3player_status {
    if out_pcm.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_pcm is NULL");
    }
    unsafe { *out_pcm = bof3player_pcm { samples: ptr::null_mut(), frames: 0, channels: 0, sample_rate: 0 } };
    if bank.is_null() {
        return fail(bof3player_status::InvalidArgument, "bank is NULL");
    }
    guard(|| {
        let bank = unsafe { &*bank };
        let vag = bank.bank.vags.get(index).ok_or_else(|| Fail::new(bof3player_status::InvalidIndex, format!("VAG index {index} is out of range (the bank has {} entries)", bank.bank.vags.len())))?;
        if vag.is_empty() {
            unsafe { *out_pcm = owned_pcm(Vec::new(), 1, psx_formats::VAB_SAMPLE_RATE) };
            return Ok(());
        }
        let start = vag.offset as usize;
        let end = start.checked_add(vag.size as usize).ok_or_else(|| Fail::new(bof3player_status::Format, "the VAG range overflows"))?;
        let region = bank.body.get(start..end).ok_or_else(|| Fail::new(bof3player_status::Format, "the VAG range is outside the bank body"))?;
        let samples = adpcm::decode_stream(region);
        unsafe { *out_pcm = owned_pcm(samples, 1, psx_formats::VAB_SAMPLE_RATE) };
        Ok(())
    })
}
