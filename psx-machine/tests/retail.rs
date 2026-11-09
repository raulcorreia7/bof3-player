// SPDX-License-Identifier: MIT
//! Boots the retail disc that `BOF3_DISC_DIR` names (a disc image, or a folder holding one): checks the first texture
//! the game uploads against the EMI entry it came from, plays a new game with scripted buttons through the first
//! battle, and checks that the first area displays the same frame on two runs.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use psx_formats::gfx::vram::{self, Descriptor};
use psx_machine::Machine;
use psx_machine::gpu::{Command, Frame, VramRect};

/// Fewer colours than this and the frame is a blank or a fill, not a drawn field.
const DISTINCT_COLORS: usize = 64;

/// Instructions the boot gets to reach its first texture upload: about 2000 frames.
const BUDGET: u64 = 600_000_000;

/// The disc image `BOF3_DISC_DIR` names, or `None` when it is unset.
fn disc_image() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("BOF3_DISC_DIR").filter(|value| !value.is_empty())?);
    if !path.is_dir() {
        return Some(path);
    }
    match psx_cd::discover_image(&path) {
        Ok(image) => Some(image),
        Err(error) => panic!("BOF3_DISC_DIR {} holds no disc image: {error}", path.display()),
    }
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

/// The disc file under each `Setloc` the game sent, with the frame it was sent in.
fn files_read<'a>(machine: &Machine, entries: &'a [psx_cd::Entry]) -> Vec<(u64, &'a psx_cd::Entry)> {
    machine
        .bus()
        .cdrom
        .commands
        .iter()
        .filter(|record| record.command == 0x02 && record.parameters.len() == 3)
        .filter_map(|record| {
            let [minute, second, frame] = [0, 1, 2].map(|index| from_bcd(record.parameters[index]));
            let sector = ((minute * 60 + second) * 75 + frame).checked_sub(150)?;
            let entry = entries.iter().find(|entry| {
                !entry.is_directory && (entry.lba..entry.lba + entry.size.div_ceil(2048).max(1)).contains(&sector)
            })?;
            Some((record.frame, entry))
        })
        .collect()
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

    let mut disc = psx_cd::Image::open(&image).expect("the image opens");
    let entries = disc.entries().expect("the image has a file system");
    let mut files: Vec<psx_cd::Entry> = Vec::new();
    for (_, entry) in files_read(&machine, &entries) {
        let emi = entry.path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("emi"));
        if emi && !files.contains(entry) {
            files.push(entry.clone());
        }
    }
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

/// Start through the title screen and the opening, then Cross to page through the dialogue and the first battle's
/// commands.
fn buttons(frame: u64) -> u16 {
    const START: u16 = 1 << 3;
    const CROSS: u16 = 1 << 14;
    match frame {
        900..2300 if (frame - 900) % 90 < 4 => START,
        2400.. if (frame - 2400) % 60 < 4 => CROSS,
        _ => 0,
    }
}

/// Frames the scripted game gets to win its first battle and return to the field (about 8050).
const NEW_GAME_FRAMES: u64 = 9000;

/// Plays a new game with [`buttons`] until the game has read a `BIN/WORLD00` area after a `BIN/BOSS` battle, or
/// [`NEW_GAME_FRAMES`] have run.
fn play_through_the_first_battle(image: &Path, entries: &[psx_cd::Entry]) -> (Machine, Option<(u64, psx_cd::Entry)>) {
    let mut machine = Machine::boot_image(image).expect("the disc boots");
    let under = |entry: &psx_cd::Entry, folder: &str| entry.path.starts_with(Path::new(folder));
    while machine.frame() < NEW_GAME_FRAMES {
        let next = machine.frame() + 1;
        machine.kernel_mut().buttons = buttons(machine.frame());
        machine.run_until(|machine| machine.frame() >= next, 50_000_000).expect("the game runs without a halt");
        if !next.is_multiple_of(60) {
            continue;
        }
        let files = files_read(&machine, entries);
        let battle = files.iter().position(|(_, entry)| under(entry, "BIN/BOSS"));
        let back_in_the_field = battle.and_then(|battle| {
            files[battle..]
                .iter()
                .find(|(_, entry)| under(entry, "BIN/WORLD00"))
                .map(|(frame, entry)| (*frame, (*entry).clone()))
        });
        if back_in_the_field.is_some() {
            return (machine, back_in_the_field);
        }
    }
    (machine, None)
}

#[test]
fn new_game_reaches_the_field_through_the_first_battle() {
    let Some(image) = disc_image() else {
        eprintln!("BOF3_DISC_DIR is not set; skipping");
        return;
    };
    let mut disc = psx_cd::Image::open(&image).expect("the image opens");
    let entries = disc.entries().expect("the image has a file system");
    let (machine, back_in_the_field) = play_through_the_first_battle(&image, &entries);
    let (frame, area) = back_in_the_field.unwrap_or_else(|| {
        let files = files_read(&machine, &entries);
        let names: Vec<_> = files.iter().map(|(frame, entry)| format!("{frame}: {}", entry.path.display())).collect();
        panic!("no return to the field from a boss battle by frame {}; files read: {names:#?}", machine.frame())
    });
    let textured_polygons = machine
        .gpu_log()
        .iter()
        .filter(|logged| matches!(&logged.command, Command::Draw(draw) if draw.texture.is_some() && draw.opcode < 0x40))
        .count();
    assert!(textured_polygons > 0, "the field and the battle draw textured polygons");
    eprintln!(
        "frame {frame}: back in {} after the first battle; {textured_polygons} textured polygons, {} key-ons",
        area.path.display(),
        machine.key_ons().len()
    );
}

/// Frames the field gets to draw after the game read the area file.
const FIELD_SETTLE_FRAMES: u64 = 120;

/// The displayed frame of the first area once the scripted game is back in it, and the area.
fn first_area_frame(image: &Path, entries: &[psx_cd::Entry]) -> (Frame, psx_cd::Entry) {
    let (mut machine, back_in_the_field) = play_through_the_first_battle(image, entries);
    let (_, area) = back_in_the_field.expect("the scripted game gets back to the field");
    machine.kernel_mut().buttons = 0;
    machine.run_frames(FIELD_SETTLE_FRAMES, FIELD_SETTLE_FRAMES * 1_000_000).expect("the field runs without a halt");
    (machine.display(), area)
}

#[test]
fn first_area_displays_the_same_frame_on_every_run() {
    let Some(image) = disc_image() else {
        eprintln!("BOF3_DISC_DIR is not set; skipping");
        return;
    };
    let entries = psx_cd::Image::open(&image).expect("the image opens").entries().expect("the image has a file system");
    let [(frame, area), (again, _)] = std::thread::scope(|scope| {
        let runs =
            [scope.spawn(|| first_area_frame(&image, &entries)), scope.spawn(|| first_area_frame(&image, &entries))];
        runs.map(|run| run.join().expect("the run finishes"))
    });
    assert!(
        area.path.starts_with(Path::new("BIN/WORLD00")),
        "the first area is in BIN/WORLD00, not {}",
        area.path.display()
    );
    let colors: HashSet<&[u8; 4]> = frame.rgba.as_chunks::<4>().0.iter().collect();
    assert!(frame.width > 0 && frame.height > 0, "the display shows a picture");
    assert!(colors.len() > DISTINCT_COLORS, "the field frame has {} colours", colors.len());
    let hash = |frame: &Frame| {
        let mut hasher = DefaultHasher::new();
        frame.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(&frame), hash(&again), "two runs of the same buttons display the same frame");

    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("retail");
    std::fs::create_dir_all(&folder).expect("the temporary folder is writable");
    let png = psx_formats::gfx::png::rgba_png(frame.width as u32, frame.height as u32, &frame.rgba);
    std::fs::write(folder.join("first-area.png"), png).expect("the frame is written");
    eprintln!(
        "{}: {}x{} frame, {} colours, hash {:016x}",
        area.path.display(),
        frame.width,
        frame.height,
        colors.len(),
        hash(&frame)
    );
}
