// SPDX-License-Identifier: MIT
//! Boots the retail disc that `BOF3_DISC_DIR` names (a disc image, or a folder holding one) and
//! checks the first texture the game uploads against the EMI entry it came from. Skips when the
//! variable is unset.

use std::path::PathBuf;

use psx_formats::gfx::vram::{self, Descriptor};
use psx_machine::Machine;
use psx_machine::gpu::{Command, VramRect};

/// Instructions the boot gets to reach its first texture upload: about 2000 frames.
const BUDGET: u64 = 600_000_000;

fn disc_image() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("BOF3_DISC_DIR")?);
    if path.is_dir() { psx_cd::discover_image(&path).ok() } else { Some(path) }
}

/// The first 32×32 upload, the size of one type-3 EMI chunk, in the GPU log from `from` on.
fn chunk_upload(machine: &Machine, from: usize) -> Option<(VramRect, Vec<u16>)> {
    machine.gpu_log()[from..].iter().find_map(|logged| match &logged.command {
        Command::Upload { rect, pixels } if rect.width == 32 && rect.height == 32 => Some((*rect, pixels.clone())),
        _ => None,
    })
}

fn from_bcd(value: u8) -> u32 {
    u32::from(value >> 4) * 10 + u32::from(value & 0xF)
}

#[test]
fn first_texture_upload_matches_its_emi_entry() {
    let Some(image) = disc_image() else {
        eprintln!("BOF3_DISC_DIR is not set; skipping");
        return;
    };
    let mut machine = Machine::boot_image(&image).expect("the disc boots");
    let mut seen = 0;
    let stop = machine
        .run_until(
            |machine| {
                if chunk_upload(machine, seen).is_some() {
                    return true;
                }
                seen = machine.gpu_log().len();
                false
            },
            BUDGET,
        )
        .expect("the boot runs without a halt");
    assert_eq!(stop, psx_machine::Stop::Condition, "no 32x32 upload by frame {}", machine.frame());
    let (rect, pixels) = chunk_upload(&machine, seen).expect("the condition saw the upload");
    let vram: Vec<u16> = (0..32)
        .flat_map(|row| {
            let start = (usize::from(rect.y) + row) * 1024 + usize::from(rect.x);
            machine.vram()[start..start + 32].to_vec()
        })
        .collect();
    assert_eq!(vram, pixels, "VRAM holds the uploaded rectangle");

    // The EMI files under the sectors the game asked for.
    let mut disc = psx_cd::Image::open(&image).expect("the image opens");
    let entries = disc.entries().expect("the image has a file system");
    let mut sectors: Vec<u32> = machine
        .bus()
        .cdrom
        .commands
        .iter()
        .filter(|record| record.command == 0x02 && record.parameters.len() == 3)
        .map(|record| {
            let [minute, second, frame] = [0, 1, 2].map(|index| from_bcd(record.parameters[index]));
            (minute * 60 + second) * 75 + frame - 150
        })
        .collect();
    sectors.dedup();
    let files: Vec<_> = entries
        .iter()
        .filter(|entry| {
            !entry.is_directory
                && entry.path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("emi"))
                && sectors.iter().any(|&sector| (entry.lba..entry.lba + entry.size.div_ceil(2048)).contains(&sector))
        })
        .cloned()
        .collect();
    assert!(!files.is_empty(), "the game read no EMI file before its first upload");

    let mut found = None;
    for file in &files {
        let bytes = disc.read_file(file).expect("the EMI file reads");
        let archive = emi::Archive::parse(&bytes).expect("the EMI table parses");
        for (index, entry) in archive.entries().iter().enumerate() {
            if entry.file_type != vram::UPLOAD_TYPE {
                continue;
            }
            let payload = archive.payload(&bytes, index).expect("the payload is inside the file");
            let descriptor = Descriptor::from_load_argument(entry.ram_ptr);
            let Ok(rectangles) = vram::decode(payload, descriptor) else { continue };
            if rectangles.iter().any(|chunk| {
                (chunk.x, chunk.y) == (usize::from(rect.x), usize::from(rect.y)) && chunk.words[..] == pixels[..]
            }) {
                found = Some((file.path.clone(), index));
            }
        }
    }
    let (path, index) = found.expect("a type-3 EMI entry holds the uploaded rectangle at its position");
    eprintln!(
        "frame {}: 32x32 upload at ({}, {}) equals {} entry {index}",
        machine.frame(),
        rect.x,
        rect.y,
        path.display()
    );
}
