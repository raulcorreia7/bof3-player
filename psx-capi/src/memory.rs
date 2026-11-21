// SPDX-License-Identifier: MIT
//! The owned byte buffer and string, and their free functions.

use core::ffi::c_char;
use std::ptr;

/// An owned byte buffer. Free it with `psx_buffer_free`.
///
/// `data` is NULL when `len` is 0.
#[repr(C)]
pub struct psx_buffer {
    /// The bytes; NULL when `len` is 0.
    pub data: *mut u8,
    /// The length in bytes.
    pub len: usize,
}

/// An owned UTF-8 string without a terminator. Free it with `psx_string_free`.
///
/// `data` is NULL when `len` is 0.
#[repr(C)]
pub struct psx_string {
    /// The bytes; NULL when `len` is 0.
    pub data: *mut c_char,
    /// The length in bytes.
    pub len: usize,
}

/// Move `bytes` into an owned buffer.
pub fn owned_buffer(bytes: Vec<u8>) -> psx_buffer {
    if bytes.is_empty() {
        return psx_buffer { data: ptr::null_mut(), len: 0 };
    }
    let bytes = bytes.into_boxed_slice();
    let len = bytes.len();
    psx_buffer { data: Box::into_raw(bytes).cast::<u8>(), len }
}

/// Move `text` into an owned string.
pub fn owned_string(text: impl Into<String>) -> psx_string {
    let buffer = owned_buffer(text.into().into_bytes());
    psx_string { data: buffer.data.cast::<c_char>(), len: buffer.len }
}

/// Free a buffer and zero it.
pub fn free_buffer(buffer: &mut psx_buffer) {
    if !buffer.data.is_null() {
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
    }
    *buffer = psx_buffer { data: ptr::null_mut(), len: 0 };
}

/// Free a string and zero it.
pub fn free_string(string: &mut psx_string) {
    let mut buffer = psx_buffer { data: string.data.cast::<u8>(), len: string.len };
    free_buffer(&mut buffer);
    *string = psx_string { data: ptr::null_mut(), len: 0 };
}

/// Free an owned buffer and zero it.
///
/// # Safety
///
/// `buffer` is NULL or points to a buffer returned by this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_buffer_free(buffer: *mut psx_buffer) {
    if let Some(buffer) = unsafe { buffer.as_mut() } {
        free_buffer(buffer);
    }
}

/// Free an owned string and zero it.
///
/// # Safety
///
/// `string` is NULL or points to a string returned by this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_string_free(string: *mut psx_string) {
    if let Some(string) = unsafe { string.as_mut() } {
        free_string(string);
    }
}
