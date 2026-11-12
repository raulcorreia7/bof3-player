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

/// Read the bytes of an owned string.
unsafe fn string_of(string: &bof3player_string) -> String {
    if string.data.is_null() {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(string.data.cast::<u8>(), string.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Read a NUL-terminated pointer for a failure message.
unsafe fn c_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::from("(no error)");
    }
    unsafe { std::ffi::CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
}

#[test]
fn disc_lists_and_reads() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("A.TXT"), b"hello").unwrap();
    let image = dir.path().join("disc.iso");
    psx_cd::rebuild::iso(&input, &image).unwrap();

    let (path, len) = path_arg(&image);
    let mut disc: *mut bof3player_disc = ptr::null_mut();
    let status = unsafe { disc::bof3player_disc_open_path(path, len, &mut disc) };
    assert_eq!(status, bof3player_status::Ok, "{}", unsafe { c_string(bof3player_last_error_message()) });

    let mut count = 0usize;
    assert_eq!(unsafe { disc::bof3player_disc_entry_count(disc, &mut count) }, bof3player_status::Ok);
    let mut found = false;
    for index in 0..count {
        let mut name_string = zero_string();
        assert_eq!(unsafe { disc::bof3player_disc_entry_path(disc, index, &mut name_string) }, bof3player_status::Ok);
        let name = unsafe { string_of(&name_string) };
        unsafe { bof3player_string_free(&mut name_string) };
        if name != "A.TXT" {
            continue;
        }
        let mut entry = bof3player_disc_entry { lba: 0, size: 0, is_directory: 0, is_xa: 0, extended_attributes: 0, reserved: 0 };
        assert_eq!(unsafe { disc::bof3player_disc_entry_at(disc, index, &mut entry) }, bof3player_status::Ok);
        assert_eq!(entry.size, 5);
        let mut bytes = bof3player_buffer { data: ptr::null_mut(), len: 0 };
        assert_eq!(unsafe { disc::bof3player_disc_read_entry(disc, index, &mut bytes) }, bof3player_status::Ok);
        let data = unsafe { std::slice::from_raw_parts(bytes.data, bytes.len) };
        assert_eq!(data, b"hello");
        unsafe { bof3player_buffer_free(&mut bytes) };
        found = true;
    }
    assert!(found, "A.TXT is in the file table");
    unsafe { disc::bof3player_disc_free(disc) };
}

fn minimal_archive(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 0x800];
    bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
    bytes[4..8].copy_from_slice(&7u32.to_le_bytes());
    bytes[8..16].copy_from_slice(b"MATH_TBL");
    bytes[0x10..0x14].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes[0x14..0x18].copy_from_slice(&0x1234u32.to_le_bytes());
    bytes[0x18..0x1c].copy_from_slice(&0x5678u32.to_le_bytes());
    bytes[0x1c..0x1e].copy_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn emi_table_and_entry() {
    let bytes = minimal_archive(b"abcd");
    let mut archive: *mut bof3player_emi = ptr::null_mut();
    let status = unsafe { emi::bof3player_emi_parse(bytes.as_ptr(), bytes.len(), &mut archive) };
    assert_eq!(status, bof3player_status::Ok, "{}", unsafe { c_string(bof3player_last_error_message()) });

    let mut count = 0usize;
    assert_eq!(unsafe { emi::bof3player_emi_entry_count(archive, &mut count) }, bof3player_status::Ok);
    assert_eq!(count, 1);

    let mut entry = bof3player_emi_entry { size: 0, ram_ptr: 0, first4: 0, file_type: 0, reserved: 0, offset: 0 };
    assert_eq!(unsafe { emi::bof3player_emi_entry_at(archive, 0, &mut entry) }, bof3player_status::Ok);
    assert_eq!((entry.size, entry.ram_ptr, entry.first4, entry.file_type, entry.offset), (4, 0x1234, 0x5678, 3, 0x800));

    let mut payload = bof3player_buffer { data: ptr::null_mut(), len: 0 };
    assert_eq!(unsafe { emi::bof3player_emi_read_entry(archive, 0, &mut payload) }, bof3player_status::Ok);
    assert_eq!(unsafe { std::slice::from_raw_parts(payload.data, payload.len) }, b"abcd");
    unsafe { bof3player_buffer_free(&mut payload) };

    let mut missing = bof3player_buffer { data: ptr::null_mut(), len: 0 };
    assert_eq!(unsafe { emi::bof3player_emi_read_entry(archive, 1, &mut missing) }, bof3player_status::InvalidIndex);
    assert!(missing.data.is_null() && missing.len == 0);

    unsafe { emi::bof3player_emi_free(archive) };

    let archive_dir = tempfile::tempdir().unwrap();
    let path = archive_dir.path().join("audio.emi");
    std::fs::write(&path, &bytes).unwrap();
    let (path, len) = path_arg(&path);
    let mut archive: *mut bof3player_emi = ptr::null_mut();
    assert_eq!(unsafe { emi::bof3player_emi_open_path(path, len, &mut archive) }, bof3player_status::Ok);
    assert_eq!(unsafe { emi::bof3player_emi_entry_count(archive, &mut count) }, bof3player_status::Ok);
    assert_eq!(count, 1);
    unsafe { emi::bof3player_emi_free(archive) };
}

#[test]
fn vab_parses_and_decodes() {
    let (header, body) = psx_formats::vab::encode_bank(0, &[], &[vec![0u8; 16]]).unwrap();
    let mut file = header;
    file.extend_from_slice(&body);

    let mut bank: *mut bof3player_vab = ptr::null_mut();
    let status = unsafe { vab::bof3player_vab_parse(file.as_ptr(), file.len(), &mut bank) };
    assert_eq!(status, bof3player_status::Ok, "{}", unsafe { c_string(bof3player_last_error_message()) });

    let mut count = 0usize;
    assert_eq!(unsafe { vab::bof3player_vab_vag_count(bank, &mut count) }, bof3player_status::Ok);
    assert_eq!(count, 2, "the null sample plus one VAG");

    let mut info = bof3player_vag_info { number: 0, offset: 0, size: 0, frames: 0, loop_start_frame: 0, loop_end_frame: 0, has_end: 0, has_loop: 0 };
    assert_eq!(unsafe { vab::bof3player_vab_vag_at(bank, 1, &mut info) }, bof3player_status::Ok);
    assert_eq!((info.number, info.size, info.frames, info.has_end), (1, 16, 28, 0));

    let mut pcm = bof3player_pcm { samples: ptr::null_mut(), frames: 0, channels: 0, sample_rate: 0 };
    assert_eq!(unsafe { vab::bof3player_vab_vag_decode(bank, 0, &mut pcm) }, bof3player_status::Ok);
    assert_eq!((pcm.frames, pcm.channels), (0, 1));
    assert!(pcm.samples.is_null());
    assert_eq!(unsafe { vab::bof3player_vab_vag_decode(bank, 1, &mut pcm) }, bof3player_status::Ok);
    assert_eq!((pcm.frames, pcm.channels, pcm.sample_rate), (28, 1, 44_100));
    assert!(!pcm.samples.is_null());
    unsafe { bof3player_pcm_free(&mut pcm) };
    assert!(pcm.samples.is_null() && pcm.frames == 0);

    assert_eq!(unsafe { vab::bof3player_vab_vag_at(bank, 2, &mut info) }, bof3player_status::InvalidIndex);
    unsafe { vab::bof3player_vab_free(bank) };
}

#[test]
fn new_error_paths() {
    let mut disc: *mut bof3player_disc = ptr::null_mut();
    assert_eq!(unsafe { disc::bof3player_disc_open_path(ptr::null(), 0, &mut disc) }, bof3player_status::InvalidArgument);

    let mut archive: *mut bof3player_emi = ptr::null_mut();
    assert_eq!(unsafe { emi::bof3player_emi_parse(ptr::null(), 0, &mut archive) }, bof3player_status::Format);

    let mut bank: *mut bof3player_vab = ptr::null_mut();
    assert_eq!(unsafe { vab::bof3player_vab_parse(ptr::null(), 0, &mut bank) }, bof3player_status::Format);

    let mut buffer = bof3player_buffer { data: ptr::null_mut(), len: 0 };
    let mut pcm = bof3player_pcm { samples: ptr::null_mut(), frames: 0, channels: 0, sample_rate: 0 };
    unsafe {
        bof3player_buffer_free(&mut buffer);
        bof3player_pcm_free(&mut pcm);
        bof3player_buffer_free(ptr::null_mut());
        bof3player_pcm_free(ptr::null_mut());
        disc::bof3player_disc_free(ptr::null_mut());
        emi::bof3player_emi_free(ptr::null_mut());
        vab::bof3player_vab_free(ptr::null_mut());
    }
}
