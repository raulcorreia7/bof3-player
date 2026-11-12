// SPDX-License-Identifier: MIT
//! EMI archive access: open or parse a container, list its table, read one
//! entry.

use core::ffi::c_char;
use std::ptr;

use emi::{Archive, Entry};

use crate::args::{bytes_arg, path_arg};
use crate::error::{Fail, fail, guard, map_emi};
use crate::{bof3player_buffer, bof3player_emi, bof3player_emi_entry, bof3player_status, owned_buffer};

/// The entry at `index`, or the failure that names the range.
fn entry_at_index(archive: &Archive, index: usize) -> Result<&Entry, Fail> {
    archive.entries().get(index).ok_or_else(|| Fail::new(bof3player_status::InvalidIndex, format!("entry index {index} is out of range (the archive has {} entries)", archive.entries().len())))
}

/// An entry record with zeroed fields.
fn empty_entry() -> bof3player_emi_entry {
    bof3player_emi_entry { size: 0, ram_ptr: 0, first4: 0, file_type: 0, reserved: 0, offset: 0 }
}

/// Read an EMI archive from a file and parse its table.
///
/// The whole container is held by the handle, so `bof3player_emi_read_entry`
/// returns copies without the caller keeping the file bytes.
///
/// # Safety
///
/// `path` points to `path_len` readable bytes (or is NULL with length 0), and
/// `out_archive` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_emi_open_path(path: *const c_char, path_len: usize, out_archive: *mut *mut bof3player_emi) -> bof3player_status {
    if out_archive.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_archive is NULL");
    }
    unsafe { *out_archive = ptr::null_mut() };
    guard(|| {
        let path = unsafe { path_arg(path, path_len)? };
        let bytes = std::fs::read(&path).map_err(|error| Fail::new(bof3player_status::Io, format!("cannot read {}: {error}", path.display())))?;
        let archive = Archive::parse(&bytes).map_err(map_emi)?;
        let handle = Box::new(bof3player_emi { archive, bytes });
        unsafe { *out_archive = Box::into_raw(handle) };
        Ok(())
    })
}

/// Parse an EMI archive from bytes. The bytes are copied into the handle, so
/// the caller may free them after the call.
///
/// # Safety
///
/// `bytes` points to `bytes_len` readable bytes (or is NULL with length 0), and
/// `out_archive` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_emi_parse(bytes: *const u8, bytes_len: usize, out_archive: *mut *mut bof3player_emi) -> bof3player_status {
    if out_archive.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_archive is NULL");
    }
    unsafe { *out_archive = ptr::null_mut() };
    guard(|| {
        let bytes = unsafe { bytes_arg(bytes, bytes_len)? };
        let archive = Archive::parse(bytes).map_err(map_emi)?;
        let handle = Box::new(bof3player_emi { archive, bytes: bytes.to_vec() });
        unsafe { *out_archive = Box::into_raw(handle) };
        Ok(())
    })
}

/// Free an archive handle.
///
/// # Safety
///
/// `archive` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_emi_free(archive: *mut bof3player_emi) {
    if archive.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(archive) });
}

/// The number of table entries.
///
/// # Safety
///
/// `archive` is NULL or a live handle, and `out_count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_emi_entry_count(archive: *const bof3player_emi, out_count: *mut usize) -> bof3player_status {
    if out_count.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_count is NULL");
    }
    unsafe { *out_count = 0 };
    if archive.is_null() {
        return fail(bof3player_status::InvalidArgument, "archive is NULL");
    }
    guard(|| {
        let archive = unsafe { &*archive };
        unsafe { *out_count = archive.archive.entries().len() };
        Ok(())
    })
}

/// One table entry's type, size and position.
///
/// # Safety
///
/// `archive` is NULL or a live handle, and `out_entry` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_emi_entry_at(archive: *const bof3player_emi, index: usize, out_entry: *mut bof3player_emi_entry) -> bof3player_status {
    if out_entry.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_entry is NULL");
    }
    unsafe { *out_entry = empty_entry() };
    if archive.is_null() {
        return fail(bof3player_status::InvalidArgument, "archive is NULL");
    }
    guard(|| {
        let archive = unsafe { &*archive };
        let entry = entry_at_index(&archive.archive, index)?;
        unsafe {
            *out_entry = bof3player_emi_entry { size: entry.size, ram_ptr: entry.ram_ptr, first4: entry.first4, file_type: entry.file_type, reserved: 0, offset: entry.offset };
        }
        Ok(())
    })
}

/// Read one entry's payload as a copy.
///
/// # Safety
///
/// `archive` is NULL or a live handle, and `out_bytes` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_emi_read_entry(archive: *const bof3player_emi, index: usize, out_bytes: *mut bof3player_buffer) -> bof3player_status {
    if out_bytes.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_bytes is NULL");
    }
    unsafe { *out_bytes = bof3player_buffer { data: ptr::null_mut(), len: 0 } };
    if archive.is_null() {
        return fail(bof3player_status::InvalidArgument, "archive is NULL");
    }
    guard(|| {
        let archive = unsafe { &*archive };
        let payload = archive.archive.payload(&archive.bytes, index).map_err(map_emi)?;
        unsafe { *out_bytes = owned_buffer(payload.to_vec()) };
        Ok(())
    })
}
