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

fn zero_string() -> psx_string {
    psx_string { data: ptr::null_mut(), len: 0 }
}

fn zero_info() -> bof3player_item_info {
    bof3player_item_info { kind: bof3player_kind::Music, has_duration: false, playable: false, duration: 0.0, id: zero_string(), name: zero_string(), title: zero_string() }
}

#[test]
fn abi_version_is_two() {
    assert_eq!(bof3player_abi_version(), BOF3PLAYER_ABI_VERSION);
}

#[test]
fn library_error_paths() {
    let mut library: *mut bof3player_library = ptr::null_mut();
    let status = unsafe { bof3player_library_open_disc_path(ptr::null(), 0, &mut library) };
    assert_eq!(status, psx_status::InvalidArgument);
    assert!(library.is_null());
    assert!(!psx_last_error_message().is_null());

    let status = unsafe { bof3player_library_open_disc_path(ptr::null(), 0, ptr::null_mut()) };
    assert_eq!(status, psx_status::InvalidArgument);

    let missing = Path::new("target/tmp/capi-does-not-exist.cue");
    let (path, len) = path_arg(missing);
    let status = unsafe { bof3player_library_open_disc_path(path, len, &mut library) };
    assert_eq!(status, psx_status::Io);
    assert!(library.is_null());
    assert!(!psx_last_error_message().is_null());

    let invalid = [0xffu8, 0xfe];
    let status = unsafe { bof3player_library_open_folder_path(invalid.as_ptr().cast(), invalid.len(), &mut library) };
    assert_eq!(status, psx_status::InvalidArgument);

    let status = unsafe { bof3player_library_item_count(library, ptr::null_mut()) };
    assert_eq!(status, psx_status::InvalidArgument);

    let status = unsafe { bof3player_library_player_at(library, 0, ptr::null_mut()) };
    assert_eq!(status, psx_status::InvalidArgument);

    unsafe {
        bof3player_item_info_free(ptr::null_mut());
        bof3player_player_free(ptr::null_mut());
        bof3player_library_free(ptr::null_mut());
        psx_string_free(ptr::null_mut());
    }
}

#[test]
fn open_folder_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let (path, len) = path_arg(dir.path());
    let mut library: *mut bof3player_library = ptr::null_mut();
    let status = unsafe { bof3player_library_open_folder_path(path, len, &mut library) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });
    assert!(!library.is_null());
    assert!(psx_last_error_message().is_null(), "a success clears the last error");

    let mut count = 99usize;
    let status = unsafe { bof3player_library_item_count(library, &mut count) };
    assert_eq!(status, psx_status::Ok);
    assert_eq!(count, 0);

    let mut info = zero_info();
    let status = unsafe { bof3player_library_item_at(library, 0, &mut info) };
    assert_eq!(status, psx_status::InvalidIndex);
    assert_eq!(info.id.len, 0);
    unsafe { bof3player_item_info_free(&mut info) };

    let mut player: *mut bof3player_player = ptr::null_mut();
    let status = unsafe { bof3player_library_player_at(library, 0, &mut player) };
    assert_eq!(status, psx_status::InvalidIndex);
    assert!(player.is_null());

    let mut format = bof3player_format { sample_rate: 1, channels: 1, duration: 1.0 };
    let status = unsafe { bof3player_player_format(player, &mut format) };
    assert_eq!(status, psx_status::InvalidArgument);
    assert_eq!(format.sample_rate, 0);

    let mut written = 9usize;
    let status = unsafe { bof3player_player_render_i16(player, ptr::null_mut(), 0, &mut written) };
    assert_eq!(status, psx_status::InvalidArgument);
    assert_eq!(written, 0);

    let status = unsafe { bof3player_player_set_loop(player, bof3player_loop::Off, 0) };
    assert_eq!(status, psx_status::InvalidArgument);

    unsafe { bof3player_library_free(library) };
}

/// Read the bytes of an owned string.
unsafe fn string_of(string: &psx_string) -> String {
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
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });

    let mut count = 0usize;
    assert_eq!(unsafe { disc::bof3player_disc_entry_count(disc, &mut count) }, psx_status::Ok);
    let mut found = false;
    for index in 0..count {
        let mut name_string = zero_string();
        assert_eq!(unsafe { disc::bof3player_disc_entry_path(disc, index, &mut name_string) }, psx_status::Ok);
        let name = unsafe { string_of(&name_string) };
        unsafe { psx_string_free(&mut name_string) };
        if name != "A.TXT" {
            continue;
        }
        let mut entry = bof3player_disc_entry { lba: 0, size: 0, is_directory: false, is_xa: false, extended_attributes: 0 };
        assert_eq!(unsafe { disc::bof3player_disc_entry_at(disc, index, &mut entry) }, psx_status::Ok);
        assert_eq!(entry.size, 5);
        let mut bytes = psx_buffer { data: ptr::null_mut(), len: 0 };
        assert_eq!(unsafe { disc::bof3player_disc_read_entry(disc, index, &mut bytes) }, psx_status::Ok);
        let data = unsafe { std::slice::from_raw_parts(bytes.data, bytes.len) };
        assert_eq!(data, b"hello");
        unsafe { psx_buffer_free(&mut bytes) };
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
    let mut archive: *mut psx_emi = ptr::null_mut();
    let status = unsafe { psx_emi_parse(bytes.as_ptr(), bytes.len(), &mut archive) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });

    let mut count = 0usize;
    assert_eq!(unsafe { psx_emi_entry_count(archive, &mut count) }, psx_status::Ok);
    assert_eq!(count, 1);

    let mut entry = psx_emi_entry { size: 0, ram_ptr: 0, first4: 0, file_type: 0, reserved: 0, offset: 0 };
    assert_eq!(unsafe { psx_emi_entry_at(archive, 0, &mut entry) }, psx_status::Ok);
    assert_eq!((entry.size, entry.ram_ptr, entry.first4, entry.file_type, entry.offset), (4, 0x1234, 0x5678, 3, 0x800));

    let mut payload = psx_buffer { data: ptr::null_mut(), len: 0 };
    assert_eq!(unsafe { psx_emi_read_entry(archive, 0, &mut payload) }, psx_status::Ok);
    assert_eq!(unsafe { std::slice::from_raw_parts(payload.data, payload.len) }, b"abcd");
    unsafe { psx_buffer_free(&mut payload) };

    let mut missing = psx_buffer { data: ptr::null_mut(), len: 0 };
    assert_eq!(unsafe { psx_emi_read_entry(archive, 1, &mut missing) }, psx_status::InvalidIndex);
    assert!(missing.data.is_null() && missing.len == 0);

    unsafe { psx_emi_free(archive) };

    let archive_dir = tempfile::tempdir().unwrap();
    let path = archive_dir.path().join("audio.emi");
    std::fs::write(&path, &bytes).unwrap();
    let (path, len) = path_arg(&path);
    let mut archive: *mut psx_emi = ptr::null_mut();
    assert_eq!(unsafe { psx_emi_open_path(path, len, &mut archive) }, psx_status::Ok);
    assert_eq!(unsafe { psx_emi_entry_count(archive, &mut count) }, psx_status::Ok);
    assert_eq!(count, 1);
    unsafe { psx_emi_free(archive) };
}

#[test]
fn vab_parses_and_decodes() {
    let (header, body) = psx_formats::vab::encode_bank(0, &[], &[vec![0u8; 16]]).unwrap();
    let mut file = header;
    file.extend_from_slice(&body);

    let mut bank: *mut bof3player_vab = ptr::null_mut();
    let status = unsafe { vab::bof3player_vab_parse(file.as_ptr(), file.len(), &mut bank) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });

    let mut count = 0usize;
    assert_eq!(unsafe { vab::bof3player_vab_vag_count(bank, &mut count) }, psx_status::Ok);
    assert_eq!(count, 2, "the null sample plus one VAG");

    let mut info = bof3player_vag_info { number: 0, offset: 0, size: 0, frames: 0, loop_start_frame: 0, loop_end_frame: 0, has_end: false, has_loop: false };
    assert_eq!(unsafe { vab::bof3player_vab_vag_at(bank, 1, &mut info) }, psx_status::Ok);
    assert_eq!((info.number, info.size, info.frames, info.has_end), (1, 16, 28, false));

    let mut pcm = bof3player_pcm { samples: ptr::null_mut(), frames: 0, channels: 0, sample_rate: 0 };
    assert_eq!(unsafe { vab::bof3player_vab_vag_decode(bank, 0, &mut pcm) }, psx_status::Ok);
    assert_eq!((pcm.frames, pcm.channels), (0, 1));
    assert!(pcm.samples.is_null());
    assert_eq!(unsafe { vab::bof3player_vab_vag_decode(bank, 1, &mut pcm) }, psx_status::Ok);
    assert_eq!((pcm.frames, pcm.channels, pcm.sample_rate), (28, 1, 44_100));
    assert!(!pcm.samples.is_null());
    unsafe { bof3player_pcm_free(&mut pcm) };
    assert!(pcm.samples.is_null() && pcm.frames == 0);

    assert_eq!(unsafe { vab::bof3player_vab_vag_at(bank, 2, &mut info) }, psx_status::InvalidIndex);
    unsafe { vab::bof3player_vab_free(bank) };
}

#[test]
fn new_error_paths() {
    let mut disc: *mut bof3player_disc = ptr::null_mut();
    assert_eq!(unsafe { disc::bof3player_disc_open_path(ptr::null(), 0, &mut disc) }, psx_status::InvalidArgument);

    let mut archive: *mut psx_emi = ptr::null_mut();
    assert_eq!(unsafe { psx_emi_parse(ptr::null(), 0, &mut archive) }, psx_status::Format);

    let mut bank: *mut bof3player_vab = ptr::null_mut();
    assert_eq!(unsafe { vab::bof3player_vab_parse(ptr::null(), 0, &mut bank) }, psx_status::Format);

    let mut buffer = psx_buffer { data: ptr::null_mut(), len: 0 };
    let mut pcm = bof3player_pcm { samples: ptr::null_mut(), frames: 0, channels: 0, sample_rate: 0 };
    unsafe {
        psx_buffer_free(&mut buffer);
        bof3player_pcm_free(&mut pcm);
        psx_buffer_free(ptr::null_mut());
        bof3player_pcm_free(ptr::null_mut());
        disc::bof3player_disc_free(ptr::null_mut());
        psx_emi_free(ptr::null_mut());
        vab::bof3player_vab_free(ptr::null_mut());
    }
}

fn zero_image() -> bof3player_image {
    bof3player_image { width: 0, height: 0, rgba: psx_buffer { data: ptr::null_mut(), len: 0 } }
}

#[test]
fn vram_decodes_to_rgba() {
    let rectangle = psx_formats::gfx::vram::Rectangle { x: 0, y: 0, words: [0u16; 32 * 32] };
    let payload = psx_formats::gfx::vram::encode(&[rectangle]);
    let mut image = zero_image();
    let status = unsafe { gfx::bof3player_vram_decode(0x0000_0100, payload.as_ptr(), payload.len(), &mut image) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });
    assert_eq!((image.width, image.height), (32, 32));
    assert_eq!(image.rgba.len, 32 * 32 * 4);
    unsafe { bof3player_image_free(&mut image) };
    assert_eq!((image.width, image.height, image.rgba.len), (0, 0, 0));
}

#[test]
fn wav_encodes_pcm() {
    let samples = [0i16, 1, 2, 3];
    let pcm = bof3player_pcm { samples: samples.as_ptr() as *mut i16, frames: 2, channels: 2, sample_rate: 44_100 };
    let mut bytes = psx_buffer { data: ptr::null_mut(), len: 0 };
    let status = unsafe { audio::bof3player_wav_encode(&pcm, &mut bytes) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });
    assert!(bytes.len >= 44);
    assert_eq!(unsafe { std::slice::from_raw_parts(bytes.data, 4) }, b"RIFF");
    unsafe { psx_buffer_free(&mut bytes) };
}

fn synthetic_exe() -> Vec<u8> {
    let mut exe = vec![0u8; 0x800];
    exe[..8].copy_from_slice(b"PS-X EXE");
    exe[0x10..0x14].copy_from_slice(&0x8001_0000u32.to_le_bytes());
    exe[0x18..0x1c].copy_from_slice(&0x8001_0000u32.to_le_bytes());
    exe[0x1c..0x20].copy_from_slice(&4u32.to_le_bytes());
    exe.extend_from_slice(&0x1000_FFFFu32.to_le_bytes());
    exe
}

#[test]
fn machine_boots_runs_and_displays() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("SYSTEM.CNF"), b"BOOT = cdrom:\\MAIN.EXE;1\n").unwrap();
    std::fs::write(input.join("MAIN.EXE"), synthetic_exe()).unwrap();
    let image_path = dir.path().join("disc.iso");
    psx_cd::rebuild::iso(&input, &image_path).unwrap();

    let (path, len) = path_arg(&image_path);
    let mut machine: *mut bof3player_machine = ptr::null_mut();
    let status = unsafe { machine::bof3player_machine_boot_disc_path(path, len, &mut machine) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });

    let mut stop = bof3player_stop::Halt;
    let status = unsafe { machine::bof3player_machine_run_frames(machine, 1, 1000, &mut stop) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });
    assert_eq!(stop, bof3player_stop::Budget);

    let mut image = zero_image();
    let status = unsafe { machine::bof3player_machine_display(machine, &mut image) };
    assert_eq!(status, psx_status::Ok, "{}", unsafe { c_string(psx_last_error_message()) });
    assert!(image.width > 0 && image.height > 0);
    assert_eq!(image.rgba.len, image.width as usize * image.height as usize * 4);

    unsafe {
        bof3player_image_free(&mut image);
        machine::bof3player_machine_free(machine);
    }
}

#[test]
fn image_audio_and_machine_error_paths() {
    let mut image = zero_image();
    assert_eq!(unsafe { gfx::bof3player_vram_decode(0, ptr::null(), 0, &mut image) }, psx_status::Format);
    assert_eq!((image.width, image.height), (0, 0));

    let mut bytes = psx_buffer { data: ptr::null_mut(), len: 0 };
    assert_eq!(unsafe { audio::bof3player_wav_encode(ptr::null(), &mut bytes) }, psx_status::InvalidArgument);

    let mut machine: *mut bof3player_machine = ptr::null_mut();
    assert_eq!(unsafe { machine::bof3player_machine_boot_disc_path(ptr::null(), 0, &mut machine) }, psx_status::InvalidArgument);
    let mut stop = bof3player_stop::Pc;
    assert_eq!(unsafe { machine::bof3player_machine_run_frames(ptr::null_mut(), 1, 1, &mut stop) }, psx_status::InvalidArgument);
    assert_eq!(unsafe { machine::bof3player_machine_display(ptr::null(), &mut image) }, psx_status::InvalidArgument);

    unsafe {
        bof3player_image_free(ptr::null_mut());
        machine::bof3player_machine_free(ptr::null_mut());
    }
}
