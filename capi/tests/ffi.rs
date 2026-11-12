// SPDX-License-Identifier: MIT
//! Calls the exported functions through the C ABI with synthetic input.

use std::ffi::c_char;
use std::path::Path;
use std::ptr;

use bof3player_capi::library::*;
use bof3player_capi::*;

fn path_arg(path: &Path) -> (*const c_char, usize) {
    let text = path.to_str().expect("the test path is UTF-8");
    (text.as_ptr().cast(), text.len())
}

fn zero_string() -> bof3player_string {
    bof3player_string { data: ptr::null_mut(), len: 0 }
}

fn zero_info() -> bof3player_item_info {
    bof3player_item_info { kind: bof3player_kind::Music, has_duration: 0, playable: 0, reserved: [0; 2], duration: 0.0, id: zero_string(), name: zero_string(), title: zero_string() }
}

#[test]
fn abi_version_is_one() {
    assert_eq!(bof3player_abi_version(), 1);
}

#[test]
fn library_error_paths() {
    let mut library: *mut bof3player_library = ptr::null_mut();
    let status = unsafe { bof3player_library_open_disc_path(ptr::null(), 0, &mut library) };
    assert_eq!(status, bof3player_status::InvalidArgument);
    assert!(library.is_null());
    assert!(!bof3player_last_error_message().is_null());

    let status = unsafe { bof3player_library_open_disc_path(ptr::null(), 0, ptr::null_mut()) };
    assert_eq!(status, bof3player_status::InvalidArgument);

    let missing = Path::new("target/tmp/capi-does-not-exist.cue");
    let (path, len) = path_arg(missing);
    let status = unsafe { bof3player_library_open_disc_path(path, len, &mut library) };
    assert_eq!(status, bof3player_status::Io);
    assert!(library.is_null());
    assert!(!bof3player_last_error_message().is_null());

    let invalid = [0xffu8, 0xfe];
    let status = unsafe { bof3player_library_open_folder_path(invalid.as_ptr().cast(), invalid.len(), &mut library) };
    assert_eq!(status, bof3player_status::InvalidArgument);

    let status = unsafe { bof3player_library_item_count(library, ptr::null_mut()) };
    assert_eq!(status, bof3player_status::InvalidArgument);

    let status = unsafe { bof3player_library_player_at(library, 0, ptr::null_mut()) };
    assert_eq!(status, bof3player_status::InvalidArgument);

    unsafe {
        bof3player_item_info_free(ptr::null_mut());
        bof3player_player_free(ptr::null_mut());
        bof3player_library_free(ptr::null_mut());
        bof3player_string_free(ptr::null_mut());
    }
}

#[test]
fn open_folder_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let (path, len) = path_arg(dir.path());
    let mut library: *mut bof3player_library = ptr::null_mut();
    let status = unsafe { bof3player_library_open_folder_path(path, len, &mut library) };
    assert_eq!(status, bof3player_status::Ok, "{}", unsafe { c_string(bof3player_last_error_message()) });
    assert!(!library.is_null());
    assert!(bof3player_last_error_message().is_null(), "a success clears the last error");

    let mut count = 99usize;
    let status = unsafe { bof3player_library_item_count(library, &mut count) };
    assert_eq!(status, bof3player_status::Ok);
    assert_eq!(count, 0);

    let mut info = zero_info();
    let status = unsafe { bof3player_library_item_at(library, 0, &mut info) };
    assert_eq!(status, bof3player_status::InvalidIndex);
    assert_eq!(info.id.len, 0);
    unsafe { bof3player_item_info_free(&mut info) };

    let mut player: *mut bof3player_player = ptr::null_mut();
    let status = unsafe { bof3player_library_player_at(library, 0, &mut player) };
    assert_eq!(status, bof3player_status::InvalidIndex);
    assert!(player.is_null());

    let mut format = bof3player_format { sample_rate: 1, channels: 1, duration: 1.0 };
    let status = unsafe { bof3player_player_format(player, &mut format) };
    assert_eq!(status, bof3player_status::InvalidArgument);
    assert_eq!(format.sample_rate, 0);

    let mut written = 9usize;
    let status = unsafe { bof3player_player_render_i16(player, ptr::null_mut(), 0, &mut written) };
    assert_eq!(status, bof3player_status::InvalidArgument);
    assert_eq!(written, 0);

    let status = unsafe { bof3player_player_set_loop(player, bof3player_loop::Off, 0) };
    assert_eq!(status, bof3player_status::InvalidArgument);

    unsafe { bof3player_library_free(library) };
}

/// Read a NUL-terminated pointer for a failure message.
unsafe fn c_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::from("(no error)");
    }
    unsafe { std::ffi::CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
}
