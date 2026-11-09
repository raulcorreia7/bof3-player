// SPDX-License-Identifier: MIT
//! The MDEC: the motion decoder that turns run-length coded DCT blocks into pixels for movies
//! (psx-spx "MDEC I/O Ports", "MDEC Commands", "MDEC Decompression"). A command decodes as soon as
//! its last parameter arrives, and its pixels wait in an output queue that has no size limit.
//! The inverse DCT follows psx-spx's reference decoder, so a pixel can differ from the console's
//! by a step or two: against the console dumps of the `ps1-tests` MDEC programs, 49 of 64 bytes of
//! an 8-bit block and 30 of 32 bytes of a 4-bit block match.

use std::collections::VecDeque;

/// Raster position of each zigzag index.
const ZAGZIG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52,
    45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
/// The halfword that ends a block, and pads the stream between blocks.
const END_OF_BLOCK: u16 = 0xFE00;

/// Pixel format of a decode command, bits 27–28.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Depth {
    /// 4-bit monochrome.
    Mono4,
    /// 8-bit monochrome.
    Mono8,
    /// 24-bit colour.
    Rgb24,
    /// 15-bit colour.
    Rgb15,
}

/// The MDEC registers, tables and queues.
#[derive(Clone, Debug)]
pub struct Mdec {
    command: u32,
    remaining: u32,
    parameters: Vec<u32>,
    luma: [u8; 64],
    chroma: [u8; 64],
    scale: [i16; 64],
    output: VecDeque<u32>,
    control: u32,
    /// Macroblocks decoded since power-on.
    pub macroblocks: u64,
}

impl Default for Mdec {
    fn default() -> Self {
        Self { command: 0, remaining: 0, parameters: Vec::new(), luma: [1; 64], chroma: [1; 64], scale: [0; 64], output: VecDeque::new(), control: 0, macroblocks: 0 }
    }
}

impl Mdec {
    /// Words waiting to be read.
    #[must_use]
    pub fn output_len(&self) -> usize {
        self.output.len()
    }

    /// The status register at 0x1F801824.
    #[must_use]
    pub fn status(&self) -> u32 {
        let busy = self.remaining > 0 || !self.output.is_empty();
        let mut status = u32::from(self.output.is_empty()) << 31;
        status |= u32::from(busy) << 29;
        status |= u32::from(self.control & (1 << 30) != 0 && self.remaining > 0) << 28;
        status |= u32::from(self.control & (1 << 29) != 0 && !self.output.is_empty()) << 27;
        status |= ((self.command >> 25) & 0xF) << 23;
        status |= 4 << 16;
        status | (self.remaining.wrapping_sub(1) & 0xFFFF)
    }

    /// A write to the control register at 0x1F801824: bit 31 resets, bits 29–30 enable the DMA
    /// requests.
    pub fn write_control(&mut self, value: u32) {
        if value & (1 << 31) != 0 {
            self.command = 0;
            self.remaining = 0;
            self.parameters.clear();
            self.output.clear();
        }
        self.control = value & 0x6000_0000;
    }

    /// A word for the command register at 0x1F801820, from the CPU or DMA channel 0.
    pub fn write(&mut self, value: u32) {
        if self.remaining == 0 {
            self.command = value;
            self.parameters.clear();
            self.remaining = match value >> 29 {
                2 if value & 1 != 0 => 32,
                2 => 16,
                3 => 32,
                _ => value & 0xFFFF,
            };
        } else {
            self.parameters.push(value);
            self.remaining -= 1;
        }
        if self.remaining == 0 {
            self.execute();
        }
    }

    /// The next output word, from the data register at 0x1F801820 or DMA channel 1.
    pub fn read(&mut self) -> u32 {
        self.output.pop_front().unwrap_or(0)
    }

    fn execute(&mut self) {
        let bytes: Vec<u8> = self.parameters.iter().flat_map(|word| word.to_le_bytes()).collect();
        match self.command >> 29 {
            1 => self.decode(),
            2 => {
                self.luma.copy_from_slice(&bytes[..64]);
                if self.command & 1 != 0 {
                    self.chroma.copy_from_slice(&bytes[64..128]);
                }
            }
            3 => {
                for (index, entry) in self.scale.iter_mut().enumerate() {
                    *entry = i16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]);
                }
            }
            _ => {}
        }
    }

    fn depth(&self) -> Depth {
        match (self.command >> 27) & 3 {
            0 => Depth::Mono4,
            1 => Depth::Mono8,
            2 => Depth::Rgb24,
            _ => Depth::Rgb15,
        }
    }

    fn decode(&mut self) {
        let halfwords: Vec<u16> = self.parameters.iter().flat_map(|word| [*word as u16, (*word >> 16) as u16]).collect();
        let mut stream = halfwords.into_iter();
        let signed = self.command & (1 << 26) != 0;
        let bit15 = if self.command & (1 << 25) != 0 { 0x8000 } else { 0 };
        let depth = self.depth();
        let mut out = Output::default();
        loop {
            match depth {
                Depth::Mono4 | Depth::Mono8 => {
                    let Some(y) = self.block(&mut stream, false) else { break };
                    for value in y {
                        let value = value.clamp(-128, 127);
                        let value = if signed { value as u8 } else { (value as u8) ^ 0x80 };
                        if depth == Depth::Mono8 {
                            out.byte(value);
                        } else {
                            // Four-bit output rounds to the nearest step and saturates.
                            out.nibble(((u16::from(value) + 8) >> 4).min(15) as u8);
                        }
                    }
                }
                Depth::Rgb24 | Depth::Rgb15 => {
                    let Some(cr) = self.block(&mut stream, true) else { break };
                    let Some(cb) = self.block(&mut stream, true) else { break };
                    let mut pixels = [[0u8; 3]; 256];
                    for (x0, y0) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
                        let Some(y) = self.block(&mut stream, false) else { return self.finish(out) };
                        colour(&mut pixels, &y, &cr, &cb, x0, y0, signed);
                    }
                    for [r, g, b] in pixels {
                        if depth == Depth::Rgb24 {
                            out.byte(r);
                            out.byte(g);
                            out.byte(b);
                        } else {
                            let pixel = u16::from(r >> 3) | (u16::from(g >> 3) << 5) | (u16::from(b >> 3) << 10);
                            out.half(pixel | bit15);
                        }
                    }
                }
            }
            self.macroblocks += 1;
        }
        self.finish(out);
    }

    fn finish(&mut self, out: Output) {
        self.output.extend(out.words);
        if out.fill > 0 {
            self.output.push_back(out.partial);
        }
    }

    /// Decodes one 8×8 block; `None` when the stream ends first.
    fn block(&self, stream: &mut impl Iterator<Item = u16>, chroma: bool) -> Option<[i32; 64]> {
        let table = if chroma { &self.chroma } else { &self.luma };
        let mut first = stream.next()?;
        while first == END_OF_BLOCK {
            first = stream.next()?;
        }
        let mut coefficients = [0i32; 64];
        let q_scale = i32::from(first >> 10);
        let mut k = 0usize;
        let mut halfword = first;
        let mut value = signed10(halfword) * i32::from(table[0]);
        loop {
            if q_scale == 0 {
                value = signed10(halfword) * 2;
            }
            coefficients[if q_scale > 0 { ZAGZIG[k] } else { k }] = value.clamp(-0x400, 0x3FF);
            // The 64th coefficient completes the block without an end code.
            if k == 63 {
                break;
            }
            halfword = stream.next()?;
            k += usize::from(halfword >> 10) + 1;
            if k > 63 {
                break;
            }
            value = (signed10(halfword) * i32::from(table[k]) * q_scale + 4) >> 3;
        }
        Some(self.idct(coefficients))
    }

    /// The inverse DCT of psx-spx "MDEC Decompression": two passes over the scale table, each
    /// rounded to 13 fraction bits.
    fn idct(&self, block: [i32; 64]) -> [i32; 64] {
        let mut source = block;
        let mut target = [0i32; 64];
        for _ in 0..2 {
            for x in 0..8 {
                for y in 0..8 {
                    let sum: i64 = (0..8).map(|z| i64::from(source[y + z * 8]) * i64::from(self.scale[x + z * 8] >> 3)).sum();
                    target[x + y * 8] = ((sum + 0xFFF) >> 13) as i32;
                }
            }
            std::mem::swap(&mut source, &mut target);
        }
        source
    }
}

/// Converts one luma block and the macroblock's chroma into RGB at (`x0`, `y0`).
fn colour(pixels: &mut [[u8; 3]; 256], y: &[i32; 64], cr: &[i32; 64], cb: &[i32; 64], x0: usize, y0: usize, signed: bool) {
    for row in 0..8 {
        for column in 0..8 {
            let chroma = (x0 + column) / 2 + ((y0 + row) / 2) * 8;
            let (r, b) = (f64::from(cr[chroma]), f64::from(cb[chroma]));
            let luma = y[column + row * 8];
            let g = (-0.3437 * b - 0.7143 * r) as i32;
            let (r, b) = ((1.402 * r) as i32, (1.772 * b) as i32);
            let channel = |value: i32| {
                let value = (luma + value).clamp(-128, 127) as u8;
                if signed { value } else { value ^ 0x80 }
            };
            pixels[x0 + column + (y0 + row) * 16] = [channel(r), channel(g), channel(b)];
        }
    }
}

fn signed10(halfword: u16) -> i32 {
    (i32::from(halfword & 0x3FF) << 22) >> 22
}

/// Packs output bytes into little-endian words.
#[derive(Default)]
struct Output {
    words: Vec<u32>,
    partial: u32,
    fill: u32,
}

impl Output {
    fn bits(&mut self, value: u32, width: u32) {
        self.partial |= value << self.fill;
        self.fill += width;
        if self.fill == 32 {
            self.words.push(self.partial);
            self.partial = 0;
            self.fill = 0;
        }
    }

    fn byte(&mut self, value: u8) {
        self.bits(u32::from(value), 8);
    }

    fn nibble(&mut self, value: u8) {
        self.bits(u32::from(value & 0xF), 4);
    }

    fn half(&mut self, value: u16) {
        self.bits(u32::from(value), 16);
    }
}
