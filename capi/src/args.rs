// SPDX-License-Identifier: MIT
//! Argument helpers shared by the exports.

use core::ffi::c_char;
use std::path::PathBuf;
use std::slice;

use crate::bof3player_status;
use crate::error::Fail;

/// Read a UTF-8 path argument. A NUL byte is rejected because it cannot name a
/// file.
///
/// # Safety
///
/// `path` points to `len` readable bytes, or is NULL with `len` 0.
pub(crate) unsafe fn path_arg(path: *const c_char, len: usize) -> Result<PathBuf, Fail> {
    if path.is_null() {
        return Err(Fail::new(bof3player_status::InvalidArgument, "path is NULL"));
    }
    if len == 0 {
        return Err(Fail::new(bof3player_status::InvalidArgument, "path is empty"));
    }
    let bytes = unsafe { slice::from_raw_parts(path.cast::<u8>(), len) };
    let text = std::str::from_utf8(bytes).map_err(|_| Fail::new(bof3player_status::InvalidArgument, "path is not UTF-8"))?;
    if text.contains('\0') {
        return Err(Fail::new(bof3player_status::InvalidArgument, "path contains a NUL byte"));
    }
    Ok(PathBuf::from(text))
}

/// Borrow a byte argument. A NULL pointer is valid only with length 0.
///
/// # Safety
///
/// `data` points to `len` readable bytes, or is NULL with `len` 0.
pub(crate) unsafe fn bytes_arg<'a>(data: *const u8, len: usize) -> Result<&'a [u8], Fail> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(Fail::new(bof3player_status::InvalidArgument, "bytes is NULL"));
    }
    Ok(unsafe { slice::from_raw_parts(data, len) })
}
