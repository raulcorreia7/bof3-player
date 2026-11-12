// SPDX-License-Identifier: MIT
//! Audio output: PCM as a RIFF/WAVE file.

use std::ptr;
use std::slice;

use bof3player::export;

use crate::error::{Fail, fail, guard, map_audio};
use crate::{bof3player_buffer, bof3player_pcm, bof3player_status, owned_buffer};

/// Write interleaved 16-bit PCM as a RIFF/WAVE file.
///
/// The input is borrowed; the output is owned and freed with
/// `bof3player_buffer_free`.
///
/// # Safety
///
/// `pcm` is NULL or a live record whose `samples` points to
/// `frames * channels` readable samples (or is NULL with `frames` 0), and
/// `out_bytes` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_wav_encode(pcm: *const bof3player_pcm, out_bytes: *mut bof3player_buffer) -> bof3player_status {
    if out_bytes.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_bytes is NULL");
    }
    unsafe { *out_bytes = bof3player_buffer { data: ptr::null_mut(), len: 0 } };
    if pcm.is_null() {
        return fail(bof3player_status::InvalidArgument, "pcm is NULL");
    }
    guard(|| {
        let pcm = unsafe { &*pcm };
        let len = pcm.frames.checked_mul(pcm.channels as usize).ok_or_else(|| Fail::new(bof3player_status::InvalidArgument, "the sample count overflows"))?;
        if len > 0 && pcm.samples.is_null() {
            return Err(Fail::new(bof3player_status::InvalidArgument, "pcm.samples is NULL"));
        }
        let samples = if len == 0 { &[] } else { unsafe { slice::from_raw_parts(pcm.samples, len) } };
        let bytes = export::wav_bytes(samples, pcm.sample_rate, pcm.channels).map_err(map_audio)?;
        unsafe { *out_bytes = owned_buffer(bytes) };
        Ok(())
    })
}
