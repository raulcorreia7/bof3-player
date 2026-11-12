// SPDX-License-Identifier: MIT
//! The machine: boot a disc image, run frames, read the displayed frame.

use core::ffi::c_char;
use std::ptr;
use std::sync::Mutex;

use psx_machine::{Machine, Stop};

use crate::args::path_arg;
use crate::error::{Fail, fail, guard, map_machine, poisoned};
use crate::{bof3player_buffer, bof3player_image, bof3player_machine, bof3player_status, bof3player_stop, owned_buffer};

/// Boot a disc image: read `SYSTEM.CNF`, load its executable and install the
/// stand-in kernel.
///
/// # Safety
///
/// `path` points to `path_len` readable bytes (or is NULL with length 0), and
/// `out_machine` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_machine_boot_disc_path(path: *const c_char, path_len: usize, out_machine: *mut *mut bof3player_machine) -> bof3player_status {
    if out_machine.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_machine is NULL");
    }
    unsafe { *out_machine = ptr::null_mut() };
    guard(|| {
        let path = unsafe { path_arg(path, path_len)? };
        let machine = Machine::boot_image(&path).map_err(map_machine)?;
        let handle = Box::new(bof3player_machine { machine: Mutex::new(machine) });
        unsafe { *out_machine = Box::into_raw(handle) };
        Ok(())
    })
}

/// Free a machine handle.
///
/// # Safety
///
/// `machine` is NULL or a handle from this library that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_machine_free(machine: *mut bof3player_machine) {
    if machine.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(machine) });
}

/// Run `frames` vertical blanks, or until `budget` instructions have run.
///
/// A halt returns `BOF3PLAYER_STATUS_HALT`, writes `BOF3PLAYER_STOP_HALT` and
/// puts the halt text in the last error.
///
/// # Safety
///
/// `machine` is NULL or a live handle, and `out_stop` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_machine_run_frames(machine: *mut bof3player_machine, frames: u64, budget: u64, out_stop: *mut bof3player_stop) -> bof3player_status {
    if out_stop.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_stop is NULL");
    }
    unsafe { *out_stop = bof3player_stop::Pc };
    if machine.is_null() {
        return fail(bof3player_status::InvalidArgument, "machine is NULL");
    }
    guard(|| {
        let machine = unsafe { &*machine };
        let mut machine = machine.machine.lock().map_err(|_| poisoned())?;
        match machine.run_frames(frames, budget) {
            Ok(stop) => {
                unsafe { *out_stop = stop_of(stop) };
                Ok(())
            }
            Err(halt) => {
                unsafe { *out_stop = bof3player_stop::Halt };
                Err(Fail::new(bof3player_status::Halt, halt.to_string()))
            }
        }
    })
}

/// Copy the displayed frame as RGBA.
///
/// Free the pixels with `bof3player_image_free`.
///
/// # Safety
///
/// `machine` is NULL or a live handle, and `out_image` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_machine_display(machine: *const bof3player_machine, out_image: *mut bof3player_image) -> bof3player_status {
    if out_image.is_null() {
        return fail(bof3player_status::InvalidArgument, "out_image is NULL");
    }
    unsafe { *out_image = bof3player_image { width: 0, height: 0, rgba: bof3player_buffer { data: ptr::null_mut(), len: 0 } } };
    if machine.is_null() {
        return fail(bof3player_status::InvalidArgument, "machine is NULL");
    }
    guard(|| {
        let machine = unsafe { &*machine };
        let machine = machine.machine.lock().map_err(|_| poisoned())?;
        let frame = machine.display();
        let width = u32::try_from(frame.width).map_err(|_| Fail::new(bof3player_status::Format, "the frame width overflows u32"))?;
        let height = u32::try_from(frame.height).map_err(|_| Fail::new(bof3player_status::Format, "the frame height overflows u32"))?;
        unsafe { *out_image = bof3player_image { width, height, rgba: owned_buffer(frame.rgba) } };
        Ok(())
    })
}

fn stop_of(stop: Stop) -> bof3player_stop {
    match stop {
        Stop::Pc => bof3player_stop::Pc,
        Stop::Frames => bof3player_stop::Frames,
        Stop::Condition => bof3player_stop::Condition,
        Stop::Budget => bof3player_stop::Budget,
    }
}
