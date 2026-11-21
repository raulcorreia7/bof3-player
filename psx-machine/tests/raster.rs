// SPDX-License-Identifier: MIT
//! Draw commands rasterized into VRAM and the displayed frame, on synthetic command lists.

use psx_machine::gpu::{Gpu, VRAM_WIDTH};

/// 8-bit grey levels that are whole 5-bit values.
const GREY_8: u32 = 0x40_4040;
const GREY_16: u32 = 0x80_8080;

/// A GPU with the drawing area over all of VRAM.
fn gpu() -> Gpu {
    let mut gpu = Gpu::new();
    gpu.gp0(0xE300_0000);
    gpu.gp0(0xE400_0000 | 1023 | (511 << 10));
    gpu
}

fn grey(level: u16) -> u16 {
    level | (level << 5) | (level << 10)
}

fn at(gpu: &Gpu, x: usize, y: usize) -> u16 {
    gpu.vram()[y * VRAM_WIDTH + x]
}

fn xy(x: u32, y: u32) -> u32 {
    (y << 16) | x
}

fn written(gpu: &Gpu, width: usize, height: usize) -> usize {
    (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).filter(|&(x, y)| at(gpu, x, y) != 0).count()
}

#[test]
fn triangle_leaves_out_its_lower_right_edges() {
    let mut gpu = gpu();
    gpu.gp0(0x2000_0000 | 0xFF_FFFF);
    for vertex in [xy(0, 0), xy(4, 0), xy(0, 4)] {
        gpu.gp0(vertex);
    }
    assert_eq!(written(&gpu, 8, 8), 4 + 3 + 2 + 1);
    assert_eq!(at(&gpu, 3, 0), grey(31));
    assert_eq!(at(&gpu, 4, 0), 0, "the right edge is not drawn");
    assert_eq!(at(&gpu, 0, 4), 0, "the bottom edge is not drawn");
}

#[test]
fn semi_transparency_modes_mix_with_the_back_pixel() {
    for (mode, expected) in [(0, 12), (1, 24), (2, 8), (3, 18)] {
        let mut gpu = gpu();
        gpu.gp0(0x0200_0000 | GREY_16);
        gpu.gp0(0);
        gpu.gp0(xy(16, 1));
        gpu.gp0(0xE100_0000 | (mode << 5));
        gpu.gp0(0x6A00_0000 | GREY_8); // a semi-transparent 1x1 rectangle
        gpu.gp0(0);
        assert_eq!(at(&gpu, 0, 0), grey(expected), "mode {mode}");
    }
}

#[test]
fn mask_settings_protect_and_mark_pixels() {
    let mut gpu = gpu();
    gpu.gp0(0xE600_0001); // set the mask bit
    gpu.gp0(0x6800_0000 | GREY_8);
    gpu.gp0(0);
    assert_eq!(at(&gpu, 0, 0), 0x8000 | grey(8));
    gpu.gp0(0xE600_0002); // check the mask bit
    gpu.gp0(0x6800_0000 | GREY_16);
    gpu.gp0(0);
    assert_eq!(at(&gpu, 0, 0), 0x8000 | grey(8), "a masked pixel is kept");
}

/// Uploads a 4-bit texel row (indices 0, 1, 2, 3 at page 1) and a CLUT at line 500, then draws
/// a raw 4x1 textured rectangle from it.
fn textured_rect(gpu: &mut Gpu, clut: [u16; 4]) {
    gpu.gp0(0xA000_0000);
    gpu.gp0(xy(0, 500));
    gpu.gp0(xy(4, 1));
    gpu.gp0(u32::from(clut[0]) | u32::from(clut[1]) << 16);
    gpu.gp0(u32::from(clut[2]) | u32::from(clut[3]) << 16);
    gpu.gp0(0xA000_0000);
    gpu.gp0(xy(64, 0));
    gpu.gp0(xy(1, 1));
    gpu.gp0(0x3210);
    gpu.gp0(0xE100_0001); // page x 64, 4-bit
    gpu.gp0(0x6500_0000);
    gpu.gp0(xy(0, 8));
    gpu.gp0((500 << 6) << 16); // CLUT at (0, 500), u = v = 0
    gpu.gp0(xy(4, 1));
}

#[test]
fn four_bit_texels_go_through_the_clut_and_zero_is_transparent() {
    let mut gpu = gpu();
    textured_rect(&mut gpu, [0, grey(1), grey(2), 0x8000 | grey(3)]);
    assert_eq!([0, 1, 2, 3].map(|x| at(&gpu, x, 8)), [0, grey(1), grey(2), 0x8000 | grey(3)]);
}
