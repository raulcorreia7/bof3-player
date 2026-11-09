// SPDX-License-Identifier: MIT
//! Runs programs from JaCzekanski's MIT-licensed `ps1-tests` suite when `PS1_TESTS_DIR` names an
//! unpacked release (the folder holding `dma/`, `cpu/` and the rest): those that report their own
//! results, and the GPU programs whose VRAM after drawing is compared with the `vram.png` the
//! suite ships, a dump from a console. Skips otherwise; the suite is not part of the repository.

use std::path::{Path, PathBuf};

use psx_formats::gfx::png::{self, Picture};
use psx_machine::Machine;
use psx_machine::gpu::{VRAM_HEIGHT, VRAM_WIDTH};

/// Frames a GPU program gets to draw its picture.
const DRAW_FRAMES: u64 = 120;

/// Runs `program` for `frames` and returns the machine.
fn run_machine(program: &str, frames: u64) -> Option<(Machine, PathBuf)> {
    let root = PathBuf::from(std::env::var_os("PS1_TESTS_DIR")?);
    let bytes = std::fs::read(root.join(program)).expect("the suite has the program");
    let mut machine = Machine::with_kernel();
    machine.load_executable(&bytes, None).expect("a PS-X EXE");
    machine.run_frames(frames, frames * 2_000_000).expect("the program runs without a halt");
    Some((machine, root))
}

/// Runs `program` for `frames` and returns what it printed.
fn run(program: &str, frames: u64) -> Option<String> {
    run_machine(program, frames).map(|(machine, _)| machine.kernel().tty.clone())
}

/// The reference VRAM dump as 15-bit colours, mask bit clear: each 8-bit channel's top five bits.
fn reference_vram(path: &Path) -> Vec<u16> {
    let picture = png::read(&std::fs::read(path).expect("the suite has the reference")).expect("a PNG");
    let rgba: Vec<[u8; 4]> = match picture {
        Picture::Rgba(image) => image.pixels.as_chunks::<4>().0.to_vec(),
        Picture::Indexed(image) => image.indices.iter().map(|&index| image.colors[usize::from(index)]).collect(),
    };
    assert_eq!(rgba.len(), VRAM_WIDTH * VRAM_HEIGHT, "{} is a 1024x512 VRAM dump", path.display());
    rgba.iter().map(|[r, g, b, _]| u16::from(r >> 3) | u16::from(g >> 3) << 5 | u16::from(b >> 3) << 10).collect()
}

/// A rectangle of VRAM: left, top, width, height.
type Area = (usize, usize, usize, usize);

/// All of VRAM.
const WHOLE_VRAM: Area = (0, 0, VRAM_WIDTH, VRAM_HEIGHT);

/// Runs the GPU program `gpu/<name>/<name>.exe` and counts the pixels in `area` whose colour
/// differs from its `vram.png`; the mask bit is not compared, the dumps do not carry it. On a
/// difference the machine's VRAM is written to `target/tmp/ps1_tests/<name>.png` to look at.
fn vram_differences(name: &str, area: Area) -> Option<usize> {
    let (machine, root) = run_machine(&format!("gpu/{name}/{name}.exe"), DRAW_FRAMES)?;
    let reference = reference_vram(&root.join("gpu").join(name).join("vram.png"));
    let (left, top, width, height) = area;
    let differences = (top..top + height).flat_map(|y| (left..left + width).map(move |x| y * VRAM_WIDTH + x)).filter(|&at| machine.vram()[at] & 0x7FFF != reference[at]).count();
    if differences > 0 {
        let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ps1_tests");
        std::fs::create_dir_all(&folder).expect("the temporary folder is writable");
        let rgba: Vec<u8> = machine.vram().iter().flat_map(|&pixel| [0, 5, 10].map(|shift| (((pixel >> shift) & 0x1F) as u8) << 3).into_iter().chain([0xFF])).collect();
        std::fs::write(folder.join(format!("{name}.png")), png::rgba_png(VRAM_WIDTH as u32, VRAM_HEIGHT as u32, &rgba)).expect("the VRAM picture is written");
    }
    Some(differences)
}

/// Asserts that `gpu/<name>` leaves `known` pixels of `area` different from its reference.
fn assert_vram_matches(name: &str, area: Area, known: usize) {
    let Some(differences) = vram_differences(name, area) else {
        eprintln!("PS1_TESTS_DIR is not set; skipping gpu/{name}");
        return;
    };
    assert_eq!(differences, known, "gpu/{name}: pixels that differ from vram.png");
}

fn assert_passes(program: &str, passes: usize) {
    let Some(tty) = run(program, 120) else {
        eprintln!("PS1_TESTS_DIR is not set; skipping {program}");
        return;
    };
    let failures: Vec<&str> = tty.lines().filter(|line| line.starts_with("fail")).collect();
    assert!(failures.is_empty(), "{program}: {failures:?}");
    assert_eq!(tty.lines().filter(|line| line.starts_with("pass")).count(), passes, "{program}:\n{tty}");
}

#[test]
fn ordering_table_clear_suite_passes() {
    assert_passes("dma/otc-test/otc-test.exe", 15);
}

#[test]
fn dma_priority_control_suite_passes() {
    assert_passes("dma/dpcr/dpcr.exe", 2);
}

#[test]
fn gte_suite_passes() {
    let Some(tty) = run("gte/test-all/test-all.exe", 600) else {
        eprintln!("PS1_TESTS_DIR is not set; skipping");
        return;
    };
    assert!(tty.contains("Passed tests: 1150\nFailed tests: 0\n"), "{tty}");
}

#[test]
fn gpu_draw_mode_register_suite_passes() {
    assert_passes("gpu/gp0-e1/gp0-e1.exe", 10);
}

#[test]
fn gpu_mask_bit_suite_passes() {
    assert_passes("gpu/mask-bit/mask-bit.exe", 5);
}

#[test]
fn gpu_clipping_matches_the_console() {
    assert_vram_matches("clipping", WHOLE_VRAM, 0);
}

#[test]
fn gpu_clut_cache_matches_the_console() {
    assert_vram_matches("clut-cache", WHOLE_VRAM, 0);
}

/// The 64 pixels are the closing segments of the two Gouraud poly-lines, from vertex 3 back to
/// vertex 1: the program never sets vertex 3's colour (it calls `setRGB2` twice), so the console
/// drew it in whatever its stack held.
#[test]
fn gpu_lines_match_the_console() {
    assert_vram_matches("lines", WHOLE_VRAM, 64);
}

#[test]
fn gpu_quad_seams_match_the_console() {
    assert_vram_matches("quad", WHOLE_VRAM, 0);
}

#[test]
fn gpu_rectangles_match_the_console() {
    assert_vram_matches("rectangles", WHOLE_VRAM, 0);
}

#[test]
fn gpu_texture_flip_matches_the_console() {
    assert_vram_matches("texture-flip", WHOLE_VRAM, 0);
}

#[test]
fn gpu_texture_overflow_matches_the_console() {
    assert_vram_matches("texture-overflow", WHOLE_VRAM, 0);
}

/// Only the 320x240 screen: the program clears nothing else, and the rest of the dump is VRAM
/// the console held before the program ran.
#[test]
fn gpu_transparency_matches_the_console() {
    assert_vram_matches("transparency", (0, 0, 320, 240), 0);
}

#[test]
fn gpu_gouraud_triangle_matches_the_console() {
    assert_vram_matches("triangle", WHOLE_VRAM, 0);
}

#[test]
fn gpu_uv_and_colour_interpolation_match_the_console() {
    assert_vram_matches("uv-interpolation", WHOLE_VRAM, 0);
}

#[test]
fn gpu_overlapping_vram_copies_match_the_console() {
    assert_vram_matches("vram-to-vram-overlap", WHOLE_VRAM, 0);
}
