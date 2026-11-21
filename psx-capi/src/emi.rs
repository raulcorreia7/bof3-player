// SPDX-License-Identifier: MIT
//! The EMI archive handle: open or parse a container, read its table and entries.

use core::ffi::c_char;

use emi::Archive;

use crate::args::{handle_ref, input, out_ref, path_arg};
use crate::error::{Fail, guard, map_emi, psx_status};
use crate::memory::{owned_buffer, psx_buffer};

/// One entry of an EMI archive's table.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct psx_emi_entry {
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

impl From<&emi::Entry> for psx_emi_entry {
    fn from(entry: &emi::Entry) -> Self {
        Self { size: entry.size, ram_ptr: entry.ram_ptr, first4: entry.first4, file_type: entry.file_type, reserved: 0, offset: entry.offset }
    }
}

impl From<&psx_emi_entry> for emi::Entry {
    fn from(entry: &psx_emi_entry) -> Self {
        Self { size: entry.size, offset: entry.offset, ram_ptr: entry.ram_ptr, first4: entry.first4, file_type: entry.file_type }
    }
}

/// A parsed EMI archive and the container bytes its payloads live in.
///
/// Opaque. Free it with `psx_emi_free`.
pub struct psx_emi {
    /// The parsed table.
    pub archive: Archive,
    /// The container bytes the payloads live in.
    pub bytes: Vec<u8>,
}

fn open(archive: Archive, bytes: Vec<u8>, out_archive: *mut *mut psx_emi) -> Result<(), Fail> {
    unsafe { *out_ref(out_archive, "out_archive")? = Box::into_raw(Box::new(psx_emi { archive, bytes })) };
    Ok(())
}

/// Read an EMI archive from a file and parse its table. The handle holds the whole container.
///
/// # Safety
///
/// `path` points to `path_len` readable bytes and `out_archive` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_open_path(path: *const c_char, path_len: usize, out_archive: *mut *mut psx_emi) -> psx_status {
    guard(|| {
        unsafe { *out_ref(out_archive, "out_archive")? = std::ptr::null_mut() };
        let path = unsafe { path_arg(path, path_len)? };
        let archive = Archive::open(&path).map_err(map_emi)?;
        open(archive, std::fs::read(&path)?, out_archive)
    })
}

/// Parse an EMI archive from bytes. The bytes are copied into the handle, so the caller may free
/// them after the call.
///
/// # Safety
///
/// `bytes` points to `bytes_len` readable bytes (or is NULL with length 0) and `out_archive` is
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_parse(bytes: *const u8, bytes_len: usize, out_archive: *mut *mut psx_emi) -> psx_status {
    guard(|| {
        unsafe { *out_ref(out_archive, "out_archive")? = std::ptr::null_mut() };
        let bytes = unsafe { input(bytes, bytes_len, "bytes")? };
        open(Archive::parse(bytes).map_err(map_emi)?, bytes.to_vec(), out_archive)
    })
}

/// Free an archive handle.
///
/// # Safety
///
/// `archive` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_free(archive: *mut psx_emi) {
    if !archive.is_null() {
        drop(unsafe { Box::from_raw(archive) });
    }
}

/// The table's version word.
///
/// # Safety
///
/// `archive` is a live handle and `out_version` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_version(archive: *const psx_emi, out_version: *mut u32) -> psx_status {
    guard(|| {
        unsafe { *out_ref(out_version, "out_version")? = handle_ref(archive, "archive")?.archive.version() };
        Ok(())
    })
}

/// The number of table entries.
///
/// # Safety
///
/// `archive` is a live handle and `out_count` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_entry_count(archive: *const psx_emi, out_count: *mut usize) -> psx_status {
    guard(|| {
        unsafe { *out_ref(out_count, "out_count")? = handle_ref(archive, "archive")?.archive.entries().len() };
        Ok(())
    })
}

/// One table entry's type, size and position.
///
/// # Safety
///
/// `archive` is a live handle and `out_entry` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_entry_at(archive: *const psx_emi, index: usize, out_entry: *mut psx_emi_entry) -> psx_status {
    guard(|| {
        let out_entry = unsafe { out_ref(out_entry, "out_entry")? };
        *out_entry = psx_emi_entry::default();
        let entries = unsafe { handle_ref(archive, "archive")? }.archive.entries();
        let entry = entries.get(index).ok_or_else(|| Fail::new(psx_status::InvalidIndex, format!("entry index {index} is out of range (the archive has {} entries)", entries.len())))?;
        *out_entry = entry.into();
        Ok(())
    })
}

/// Read one entry's payload as a copy. Free it with `psx_buffer_free`.
///
/// # Safety
///
/// `archive` is a live handle and `out_bytes` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psx_emi_read_entry(archive: *const psx_emi, index: usize, out_bytes: *mut psx_buffer) -> psx_status {
    guard(|| {
        let out_bytes = unsafe { out_ref(out_bytes, "out_bytes")? };
        *out_bytes = owned_buffer(Vec::new());
        let archive = unsafe { handle_ref(archive, "archive")? };
        *out_bytes = owned_buffer(archive.archive.payload(&archive.bytes, index).map_err(map_emi)?.to_vec());
        Ok(())
    })
}
