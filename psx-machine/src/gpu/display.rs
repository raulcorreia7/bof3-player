// SPDX-License-Identifier: MIT
//! The displayed picture, written from psx-spx "GPU Display Control Commands" (`GP1(05h)` to `GP1(08h)`), "GPU Video
//! Memory (VRAM)" (15- and 24-bit display) and "GPU Timings" (dotclocks).

use super::{Gpu, VRAM_HEIGHT, VRAM_WIDTH};

/// The picture the GPU sends to the television, as 8-bit RGBA.
/// It is the display area of VRAM: `width` pixels from the `GP1(05h)` start, the count psx-spx gives for the `GP1(06h)`
/// range, by `height` lines from the `GP1(07h)` range.
/// 15-bit pixels widen each 5-bit channel `c` to `c << 3 | c >> 2`, so that full white is 255; psx-spx does not give
/// the console's analogue output levels.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Frame {
    /// Pixels per line.
    pub width: usize,
    /// Lines.
    pub height: usize,
    /// `width * height` pixels, four bytes each (red, green, blue, alpha), row-major.
    pub rgba: Vec<u8>,
}

/// psx-spx "GPU Timings", Dotclocks: video clocks per pixel for `GP1(08h)` horizontal resolutions 256, 320, 512 and
/// 640.
const CLOCKS_PER_PIXEL: [u32; 4] = [10, 8, 5, 4];
/// The same, for the 368-pixel resolution (`GP1(08h)` bit 6).
const CLOCKS_PER_PIXEL_368: u32 = 7;

pub(super) fn frame(gpu: &Gpu) -> Frame {
    let mode = gpu.display_mode;
    let clocks = if mode & 0x40 != 0 { CLOCKS_PER_PIXEL_368 } else { CLOCKS_PER_PIXEL[(mode & 3) as usize] };
    let (x1, x2) = (u32::from(gpu.horizontal_range.0), u32::from(gpu.horizontal_range.1));
    let (y1, y2) = (usize::from(gpu.vertical_range.0), usize::from(gpu.vertical_range.1));
    // psx-spx "GP1(06h)": "(((X2-X1)/cycles_per_pix)+2) AND NOT 3" pixels per line.
    let width = if x2 > x1 { (((x2 - x1) / clocks + 2) & !3) as usize } else { 0 };
    let interlaced_480 = mode & 0x24 == 0x24;
    let height = y2.saturating_sub(y1) * if interlaced_480 { 2 } else { 1 };
    let mut rgba = vec![0; width * height * 4];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[3] = 0xFF;
    }
    if gpu.display_disabled || width == 0 {
        return Frame { width, height, rgba };
    }
    let (start_x, start_y) = (usize::from(gpu.display_start.0), usize::from(gpu.display_start.1));
    let vram = gpu.vram();
    for (line, row) in rgba.chunks_exact_mut(width * 4).enumerate() {
        let y = (start_y + line) % VRAM_HEIGHT;
        let source = &vram[y * VRAM_WIDTH..(y + 1) * VRAM_WIDTH];
        for (column, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let rgb = if mode & 0x10 != 0 {
                // 24-bit: three bytes per pixel, packed across the 16-bit VRAM words of the line.
                let byte = |index: usize| {
                    let word = source[(index / 2) % VRAM_WIDTH];
                    (word >> ((index % 2) * 8)) as u8
                };
                let at = start_x * 2 + column * 3;
                [byte(at), byte(at + 1), byte(at + 2)]
            } else {
                let word = source[(start_x + column) % VRAM_WIDTH];
                let widen = |shift: u16| {
                    let channel = ((word >> shift) & 0x1F) as u8;
                    channel << 3 | channel >> 2
                };
                [widen(0), widen(5), widen(10)]
            };
            pixel[..3].copy_from_slice(&rgb);
        }
    }
    Frame { width, height, rgba }
}
