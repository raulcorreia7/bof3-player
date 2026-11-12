// SPDX-License-Identifier: MIT
//! Library and player: open a catalog, list its items, render PCM.

use core::ffi::c_char;
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::sync::{Arc, Mutex};

use bof3player::player::Loop;
use bof3player::{DiscSource, FolderSource, Library, Player, Source};

use crate::error::{Fail, fail, guard, map_audio, poisoned};
use crate::{bof3player_format, bof3player_item_info, bof3player_kind, bof3player_library, bof3player_loop, bof3player_player, bof3player_status, bof3player_string, free_string, owned_string};

/// Read a UTF-8 path argument. A NUL byte is rejected because it cannot name a
/// file.
///
/// # Safety
///
/// `path` points to `len` readable bytes, or is NULL with `len` 0.
unsafe fn path_arg(path: *const c_char, len: usize) -> Result<PathBuf, Fail> {
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

/// Build a catalog from a source, with the bundled names.
fn open_library(source: impl Source + 'static) -> Result<Library<'static>, Fail> {
    Library::open_with(source, bof3player::names::game_names()).map_err(map_audio)
}

/// An item info with no strings; also the value written on failure.
fn empty_item_info() -> bof3player_item_info {
    bof3player_item_info {
        kind: bof3player_kind::Music,
        has_duration: 0,
        playable: 0,
        reserved: [0; 2],
        duration: 0.0,
        id: bof3player_string { data: ptr::null_mut(), len: 0 },
        name: bof3player_string { data: ptr::null_mut(), len: 0 },
        title: bof3player_string { data: ptr::null_mut(), len: 0 },
    }
}

/// Open a disc image (`.cue`, `.bin` or `.iso`) and build its catalog.
///
/// `path` is UTF-8 and need not be NUL-terminated. On success `out_library`
/// owns the library; free it with `bof3player_library_free`.
///
/// # Safety
///
/// `path` points to `path_len` readable bytes (or is NULL with length 0), and
/// `out_library` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_library_open_disc_path(path: *const c_char, path_len: usize, out_library: *mut *mut bof3player_library) -> bof3player_status {
    if out_library.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_library is NULL");
    }
    unsafe { *out_library = ptr::null_mut() };
    guard(|| {
        let path = unsafe { path_arg(path, path_len)? };
        let source = DiscSource::open(&path).map_err(map_audio)?;
        let library = open_library(source)?;
        let handle = Box::new(bof3player_library { inner: Arc::new(library) });
        unsafe { *out_library = Box::into_raw(handle) };
        Ok(())
    })
}

/// Open a folder extracted from the disc and build its catalog.
///
/// `path` is UTF-8 and need not be NUL-terminated. On success `out_library`
/// owns the library; free it with `bof3player_library_free`.
///
/// # Safety
///
/// `path` points to `path_len` readable bytes (or is NULL with length 0), and
/// `out_library` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_library_open_folder_path(path: *const c_char, path_len: usize, out_library: *mut *mut bof3player_library) -> bof3player_status {
    if out_library.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_library is NULL");
    }
    unsafe { *out_library = ptr::null_mut() };
    guard(|| {
        let path = unsafe { path_arg(path, path_len)? };
        let source = FolderSource::open(&path).map_err(map_audio)?;
        let library = open_library(source)?;
        let handle = Box::new(bof3player_library { inner: Arc::new(library) });
        unsafe { *out_library = Box::into_raw(handle) };
        Ok(())
    })
}

/// Free a library handle. Players created from it keep it alive.
///
/// # Safety
///
/// `library` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_library_free(library: *mut bof3player_library) {
    if library.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(library) });
}

/// The number of catalog items.
///
/// # Safety
///
/// `library` is NULL or a live handle, and `out_count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_library_item_count(library: *const bof3player_library, out_count: *mut usize) -> bof3player_status {
    if out_count.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_count is NULL");
    }
    unsafe { *out_count = 0 };
    if library.is_null() {
        return fail(bof3player_status::InvalidArgument, "library is NULL");
    }
    guard(|| {
        let library = unsafe { &*library };
        unsafe { *out_count = library.inner.catalog().len() };
        Ok(())
    })
}

/// The metadata of one catalog item.
///
/// On success `out_info` owns three strings; free the record with
/// `bof3player_item_info_free`. On failure it holds no strings.
///
/// # Safety
///
/// `library` is NULL or a live handle, and `out_info` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_library_item_at(library: *const bof3player_library, index: usize, out_info: *mut bof3player_item_info) -> bof3player_status {
    if out_info.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_info is NULL");
    }
    unsafe { *out_info = empty_item_info() };
    if library.is_null() {
        return fail(bof3player_status::InvalidArgument, "library is NULL");
    }
    guard(|| {
        let library = unsafe { &*library };
        let item = library.inner.catalog().get(index).ok_or_else(|| Fail::new(bof3player_status::InvalidIndex, format!("item index {index} is out of range")))?;
        let info = bof3player_item_info {
            kind: item.kind.into(),
            has_duration: u8::from(item.duration.is_some()),
            playable: u8::from(item.playable),
            reserved: [0; 2],
            duration: item.duration.unwrap_or(0.0),
            id: owned_string(&item.id),
            name: owned_string(&item.name),
            title: owned_string(&item.title),
        };
        unsafe { *out_info = info };
        Ok(())
    })
}

/// Free the strings of an item info and zero the record.
///
/// # Safety
///
/// `info` is NULL or a record filled by `bof3player_library_item_at` that has
/// not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_item_info_free(info: *mut bof3player_item_info) {
    if info.is_null() {
        return;
    }
    let info = unsafe { &mut *info };
    free_string(&mut info.id);
    free_string(&mut info.name);
    free_string(&mut info.title);
    *info = empty_item_info();
}

/// Create a player positioned at the start of one catalog item.
///
/// The item must be `playable` and must be music, a sample, XA or CD-DA. The
/// player keeps its library alive, so either handle may be freed first.
///
/// # Safety
///
/// `library` is NULL or a live handle, and `out_player` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_library_player_at(library: *const bof3player_library, index: usize, out_player: *mut *mut bof3player_player) -> bof3player_status {
    if out_player.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_player is NULL");
    }
    unsafe { *out_player = ptr::null_mut() };
    if library.is_null() {
        return fail(bof3player_status::InvalidArgument, "library is NULL");
    }
    guard(|| {
        let library = unsafe { &*library };
        let item = library.inner.catalog().get(index).ok_or_else(|| Fail::new(bof3player_status::InvalidIndex, format!("item index {index} is out of range")))?;
        let player = library.inner.player(item).map_err(map_audio)?;
        // SAFETY: the player handle stores a clone of this `Arc`, so the
        // `Library` allocation the player borrows lives as long as the player.
        // The music and sample engines parse their input into owned data, and
        // the XA and CD-DA engines own their blob.
        let player: Player<'static> = unsafe { std::mem::transmute(player) };
        let handle = Box::new(bof3player_player { player: Mutex::new(player), _library: Arc::clone(&library.inner) });
        unsafe { *out_player = Box::into_raw(handle) };
        Ok(())
    })
}

/// Free a player handle.
///
/// # Safety
///
/// `player` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_player_free(player: *mut bof3player_player) {
    if player.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(player) });
}

/// The player's sample rate, channels and duration.
///
/// # Safety
///
/// `player` is NULL or a live handle, and `out_format` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_player_format(player: *const bof3player_player, out_format: *mut bof3player_format) -> bof3player_status {
    if out_format.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_format is NULL");
    }
    unsafe { *out_format = bof3player_format { sample_rate: 0, channels: 0, duration: 0.0 } };
    if player.is_null() {
        return fail(bof3player_status::InvalidArgument, "player is NULL");
    }
    guard(|| {
        let player = unsafe { &*player };
        let player = player.player.lock().map_err(|_| poisoned())?;
        unsafe {
            *out_format = bof3player_format { sample_rate: player.sample_rate(), channels: player.channels(), duration: player.duration() };
        }
        Ok(())
    })
}

/// Render up to `frames` interleaved stereo frames into `out_samples`.
///
/// `out_samples` holds `frames * channels` samples. `out_frames` receives the
/// frames written; 0 means the item ended. A NULL `out_samples` is valid only
/// with `frames` 0.
///
/// # Safety
///
/// `player` is NULL or a live handle, `out_frames` is NULL or writable, and
/// `out_samples` points to `frames * channels` writable samples or is NULL with
/// `frames` 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_player_render_i16(player: *mut bof3player_player, out_samples: *mut i16, frames: usize, out_frames: *mut usize) -> bof3player_status {
    if out_frames.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_frames is NULL");
    }
    unsafe { *out_frames = 0 };
    if player.is_null() {
        return fail(bof3player_status::InvalidArgument, "player is NULL");
    }
    guard(|| {
        let player = unsafe { &*player };
        let mut player = player.player.lock().map_err(|_| poisoned())?;
        let channels = player.channels() as usize;
        let samples = frames.checked_mul(channels).ok_or_else(|| Fail::new(bof3player_status::InvalidArgument, "frame count overflows"))?;
        if samples > isize::MAX as usize / std::mem::size_of::<i16>() {
            return Err(Fail::new(bof3player_status::InvalidArgument, "buffer is larger than isize::MAX"));
        }
        if samples == 0 {
            return Ok(());
        }
        if out_samples.is_null() {
            return Err(Fail::new(bof3player_status::InvalidArgument, "out_samples is NULL"));
        }
        let out = unsafe { slice::from_raw_parts_mut(out_samples, samples) };
        let written = player.render_i16(out).map_err(map_audio)?;
        unsafe { *out_frames = written };
        Ok(())
    })
}

/// Set which part of the track the player repeats.
///
/// `times` is used only with `bof3player_loop::Times`.
///
/// # Safety
///
/// `player` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_player_set_loop(player: *mut bof3player_player, mode: bof3player_loop, times: u32) -> bof3player_status {
    if player.is_null() {
        return fail(bof3player_status::InvalidArgument, "player is NULL");
    }
    guard(|| {
        let player = unsafe { &*player };
        let mut player = player.player.lock().map_err(|_| poisoned())?;
        let mode = match mode {
            bof3player_loop::Off => Loop::Off,
            bof3player_loop::Times => Loop::Times(times),
            bof3player_loop::Forever => Loop::Forever,
        };
        player.set_loop(mode);
        Ok(())
    })
}
