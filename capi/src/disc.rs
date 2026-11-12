// SPDX-License-Identifier: MIT
//! Disc image access: open an image, list its file table, read one file.

use core::ffi::c_char;
use std::ptr;
use std::sync::Mutex;

use psx_cd::{Entry, Image};

use crate::args::path_arg;
use crate::error::{Fail, fail, guard, map_cd, poisoned};
use crate::{bof3player_buffer, bof3player_disc, bof3player_disc_entry, bof3player_status, bof3player_string, owned_buffer, owned_string};

/// The entry at `index`, or the failure that names the range.
fn entry_at_index(disc: &bof3player_disc, index: usize) -> Result<&Entry, Fail> {
    disc.entries.get(index).ok_or_else(|| Fail::new(bof3player_status::InvalidIndex, format!("entry index {index} is out of range (the image has {} entries)", disc.entries.len())))
}

/// An entry record with no path and zeroed numbers.
fn empty_entry() -> bof3player_disc_entry {
    bof3player_disc_entry { lba: 0, size: 0, is_directory: 0, is_xa: 0, extended_attributes: 0, reserved: 0 }
}

/// Open a disc image: a `.cue` sheet, a raw `.bin`, a cooked `.iso` or a folder
/// that holds one. The ISO 9660 file table is read once and cached.
///
/// # Safety
///
/// `path` points to `path_len` readable bytes (or is NULL with length 0), and
/// `out_disc` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_disc_open_path(path: *const c_char, path_len: usize, out_disc: *mut *mut bof3player_disc) -> bof3player_status {
    if out_disc.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_disc is NULL");
    }
    unsafe { *out_disc = ptr::null_mut() };
    guard(|| {
        let path = unsafe { path_arg(path, path_len)? };
        let mut image = Image::open(&path).map_err(map_cd)?;
        let entries = image.entries().map_err(map_cd)?;
        let handle = Box::new(bof3player_disc { image: Mutex::new(image), entries });
        unsafe { *out_disc = Box::into_raw(handle) };
        Ok(())
    })
}

/// Free a disc handle.
///
/// # Safety
///
/// `disc` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_disc_free(disc: *mut bof3player_disc) {
    if disc.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(disc) });
}

/// The number of entries in the cached file table.
///
/// # Safety
///
/// `disc` is NULL or a live handle, and `out_count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_disc_entry_count(disc: *const bof3player_disc, out_count: *mut usize) -> bof3player_status {
    if out_count.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_count is NULL");
    }
    unsafe { *out_count = 0 };
    if disc.is_null() {
        return fail(bof3player_status::InvalidArgument, "disc is NULL");
    }
    guard(|| {
        let disc = unsafe { &*disc };
        unsafe { *out_count = disc.entries.len() };
        Ok(())
    })
}

/// One entry's type, size and position.
///
/// # Safety
///
/// `disc` is NULL or a live handle, and `out_entry` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_disc_entry_at(disc: *const bof3player_disc, index: usize, out_entry: *mut bof3player_disc_entry) -> bof3player_status {
    if out_entry.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_entry is NULL");
    }
    unsafe { *out_entry = empty_entry() };
    if disc.is_null() {
        return fail(bof3player_status::InvalidArgument, "disc is NULL");
    }
    guard(|| {
        let disc = unsafe { &*disc };
        let entry = entry_at_index(disc, index)?;
        unsafe {
            *out_entry = bof3player_disc_entry {
                lba: entry.lba,
                size: entry.size,
                is_directory: u8::from(entry.is_directory),
                is_xa: u8::from(entry.is_xa),
                extended_attributes: entry.extended_attributes,
                reserved: 0,
            };
        }
        Ok(())
    })
}

/// One entry's path, as an owned UTF-8 string.
///
/// A path that is not UTF-8 is converted lossily. Free the string with
/// `bof3player_string_free`.
///
/// # Safety
///
/// `disc` is NULL or a live handle, and `out_path` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_disc_entry_path(disc: *const bof3player_disc, index: usize, out_path: *mut bof3player_string) -> bof3player_status {
    if out_path.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_path is NULL");
    }
    unsafe { *out_path = bof3player_string { data: ptr::null_mut(), len: 0 } };
    if disc.is_null() {
        return fail(bof3player_status::InvalidArgument, "disc is NULL");
    }
    guard(|| {
        let disc = unsafe { &*disc };
        let entry = entry_at_index(disc, index)?;
        let path = entry.path.to_string_lossy();
        unsafe { *out_path = owned_string(&path) };
        Ok(())
    })
}

/// Read one entry's bytes.
///
/// # Safety
///
/// `disc` is NULL or a live handle, and `out_bytes` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_disc_read_entry(disc: *mut bof3player_disc, index: usize, out_bytes: *mut bof3player_buffer) -> bof3player_status {
    if out_bytes.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_bytes is NULL");
    }
    unsafe { *out_bytes = bof3player_buffer { data: ptr::null_mut(), len: 0 } };
    if disc.is_null() {
        return fail(bof3player_status::InvalidArgument, "disc is NULL");
    }
    guard(|| {
        let disc = unsafe { &*disc };
        let entry = entry_at_index(disc, index)?;
        let mut image = disc.image.lock().map_err(|_| poisoned())?;
        let bytes = image.read_file(entry).map_err(map_cd)?;
        unsafe { *out_bytes = owned_buffer(bytes) };
        Ok(())
    })
}
