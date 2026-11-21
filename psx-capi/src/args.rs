// SPDX-License-Identifier: MIT
//! Pointer checks and slice conversions for the exported functions.

use core::ffi::c_char;
use std::path::PathBuf;

use crate::error::Fail;

/// The writable out-parameter `pointer`, or the failure that names it.
///
/// # Safety
///
/// `pointer` is NULL or writable for the returned lifetime.
pub unsafe fn out_ref<'a, T>(pointer: *mut T, what: &str) -> Result<&'a mut T, Fail> {
    unsafe { pointer.as_mut() }.ok_or_else(|| Fail::invalid(format!("{what} is NULL")))
}

/// The writable out-parameter, or `None` when the caller passed NULL to skip it.
///
/// # Safety
///
/// `pointer` is NULL or writable for the returned lifetime.
pub unsafe fn opt_out<'a, T>(pointer: *mut T) -> Option<&'a mut T> {
    unsafe { pointer.as_mut() }
}

/// The live handle `pointer`, or the failure that names it.
///
/// # Safety
///
/// `pointer` is NULL or a live handle for the returned lifetime.
pub unsafe fn handle_ref<'a, T>(pointer: *const T, what: &str) -> Result<&'a T, Fail> {
    unsafe { pointer.as_ref() }.ok_or_else(|| Fail::invalid(format!("{what} is NULL")))
}

/// Borrow `len` bytes. NULL is valid only with length 0.
///
/// # Safety
///
/// `data` points to `len` readable bytes, or is NULL with `len` 0.
pub unsafe fn input<'a>(data: *const u8, len: usize, what: &str) -> Result<&'a [u8], Fail> {
    unsafe { array(data, len, what) }
}

/// Borrow `count` items. NULL is valid only with count 0.
///
/// # Safety
///
/// `data` points to `count` readable items, or is NULL with `count` 0.
pub unsafe fn array<'a, T>(data: *const T, count: usize, what: &str) -> Result<&'a [T], Fail> {
    if count == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(Fail::invalid(format!("{what} is NULL with a nonzero length")));
    }
    Ok(unsafe { std::slice::from_raw_parts(data, count) })
}

/// Borrow `len` bytes as UTF-8 text.
///
/// # Safety
///
/// `data` points to `len` readable bytes, or is NULL with `len` 0.
pub unsafe fn utf8<'a>(data: *const c_char, len: usize, what: &str) -> Result<&'a str, Fail> {
    let bytes = unsafe { input(data.cast::<u8>(), len, what)? };
    std::str::from_utf8(bytes).map_err(|_| Fail::invalid(format!("{what} is not UTF-8")))
}

/// Like [`utf8`], but NULL with length 0 is `None`.
///
/// # Safety
///
/// `data` points to `len` readable bytes, or is NULL with `len` 0.
pub unsafe fn opt_utf8<'a>(data: *const c_char, len: usize, what: &str) -> Result<Option<&'a str>, Fail> {
    if data.is_null() && len == 0 { Ok(None) } else { unsafe { utf8(data, len, what) }.map(Some) }
}

/// A non-empty UTF-8 path without a NUL byte.
///
/// # Safety
///
/// `path` points to `len` readable bytes, or is NULL with `len` 0.
pub unsafe fn path_arg(path: *const c_char, len: usize) -> Result<PathBuf, Fail> {
    let text = unsafe { utf8(path, len, "path")? };
    if text.is_empty() || text.contains('\0') {
        return Err(Fail::invalid("path is empty or contains a NUL byte"));
    }
    Ok(PathBuf::from(text))
}
