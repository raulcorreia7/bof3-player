//! PSX CLUT (palette) codec.
//!
//! A palette payload is raw little-endian PSX color words
//! (`STP | B5 | G5 | R5`; bit 15 is STP). [`Palette::decode`] turns payload
//! bytes into words and [`Palette::encode`] turns words back into the same
//! bytes. The bank rule is [`validate_words`]: a whole number of 16-color
//! (4bpp) CLUT rows; a bank whose word count also divides by 256 is a whole
//! number of 8bpp rows. Texture mode and the texture-to-palette association are
//! draw-state data, so the codec never asks for them.
//!
//! The RGBA view ([`word_to_rgba`], [`Palette::json`], [`Palette::png_strip`])
//! is for inspection: STP is not alpha, so it survives in the words and in the
//! JSON `stp` flag, not in the RGBA tuple.

use serde::Serialize;

use crate::error::{Error, Result};

/// Colors in one 4bpp CLUT row: 16 words, `0x20` bytes.
pub const ROW_WORDS_4BPP: usize = 16;
/// Colors in one 8bpp CLUT row: 256 words, `0x200` bytes.
pub const ROW_WORDS_8BPP: usize = 256;
/// PSX color words that set STP (semi-transparency) in bit 15.
const STP: u16 = 0x8000;
/// Swatch edge of the PNG strip, in pixels.
const SWATCH_PIXELS: u32 = 16;
/// Rejects a color-word count that cannot be a CLUT block.
pub fn validate_words(words: usize) -> Result<(), Error> {
    if words < ROW_WORDS_4BPP || !words.is_multiple_of(ROW_WORDS_4BPP) {
        return Err(Error::PaletteNotRowAligned(words));
    }
    Ok(())
}

/// Expands a PSX color word to an opaque RGBA color: alpha is always 255. The
/// lossless view is [`word_to_view`].
pub fn word_to_rgba(word: u16) -> [u8; 4] {
    let [red, green, blue] = super::color::expanded(word);
    [red, green, blue, 0xff]
}

/// The view alpha of `0x0000`, which the GPU never draws.
const VIEW_TRANSPARENT: u8 = 0;
/// The view alpha of a word that sets STP (semi-transparency).
const VIEW_STP: u8 = 0x80;
/// The view alpha of every other word.
const VIEW_OPAQUE: u8 = 0xff;

/// Expands a PSX color word to RGBA, with alpha carrying what the channels
/// cannot: `0x0000` is transparent, a word with STP set is half transparent, and
/// every other word is opaque. [`view_to_word`] inverts it exactly for all
/// 65,536 words, so a view PNG holds the whole word.
pub fn word_to_view(word: u16) -> [u8; 4] {
    let alpha = match word {
        0 => VIEW_TRANSPARENT,
        _ if word & STP != 0 => VIEW_STP,
        _ => VIEW_OPAQUE,
    };
    let [red, green, blue, _] = word_to_rgba(word);
    [red, green, blue, alpha]
}

/// The color word of a view pixel, the inverse of [`word_to_view`]. Alpha below
/// 64 is `0x0000` and its channels do not count; alpha from 64 to 191 sets STP;
/// anything higher is opaque. An edited pixel falls into the same bands.
pub fn view_to_word(rgba: [u8; 4]) -> u16 {
    if rgba[3] < 64 {
        return 0;
    }
    let stp = if rgba[3] < 192 { STP } else { 0 };
    rgba_to_word(rgba) | stp
}

/// Packs RGBA into a PSX color word with the top 5 bits of each channel. The
/// low 3 bits and alpha are not representable and are dropped.
pub fn rgba_to_word(rgba: [u8; 4]) -> u16 {
    let channel = |value: u8| u16::from(value >> 3);
    channel(rgba[0]) | (channel(rgba[1]) << 5) | (channel(rgba[2]) << 10)
}

/// A CLUT block: raw little-endian PSX color words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    words: Vec<u16>,
}

#[derive(Serialize)]
struct PaletteJson {
    format: &'static str,
    words: usize,
    rows_4bpp: usize,
    rows_8bpp: Option<usize>,
    colors: Vec<ColorJson>,
}

#[derive(Serialize)]
struct ColorJson {
    index: usize,
    word: u16,
    stp: bool,
    rgba: [u8; 4],
}

impl Palette {
    /// Decodes payload bytes, rejecting shapes that are not a CLUT block.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if !bytes.len().is_multiple_of(2) {
            return Err(Error::PaletteOddByteCount(bytes.len()));
        }
        let words = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        Self::from_words(words)
    }

    /// Validates color words with [`validate_words`] and keeps them.
    pub fn from_words(words: Vec<u16>) -> Result<Self, Error> {
        validate_words(words.len())?;
        Ok(Self { words })
    }

    /// Encodes the bank back to payload bytes; the inverse of [`Self::decode`].
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.words.len() * 2);
        for word in &self.words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    /// The color words in payload order.
    pub fn words(&self) -> &[u16] {
        &self.words
    }

    /// Number of 4bpp CLUT rows; exact, because every constructor validates.
    pub fn rows_4bpp(&self) -> usize {
        self.words.len() / ROW_WORDS_4BPP
    }

    /// Number of 8bpp CLUT rows, or `None` when the bank is not 256-word aligned.
    pub fn rows_8bpp(&self) -> Option<usize> {
        self.words.len().is_multiple_of(ROW_WORDS_8BPP).then_some(self.words.len() / ROW_WORDS_8BPP)
    }

    /// The bank as JSON: geometry plus one record per color word.
    pub fn json(&self) -> String {
        let document = PaletteJson {
            format: "psx-clut",
            words: self.words.len(),
            rows_4bpp: self.rows_4bpp(),
            rows_8bpp: self.rows_8bpp(),
            colors: self.words.iter().enumerate().map(|(index, &word)| ColorJson { index, word, stp: word & STP != 0, rgba: word_to_rgba(word) }).collect(),
        };
        serde_json::to_string_pretty(&document).expect("palette model is serializable")
    }

    /// Renders the bank as an 8-bit RGBA PNG swatch strip: 16 colors per row,
    /// one 4bpp CLUT row per image row, `SWATCH_PIXELS` pixels per swatch.
    pub fn png_strip(&self) -> Vec<u8> {
        let columns = ROW_WORDS_4BPP as u32;
        let width = columns * SWATCH_PIXELS;
        let height = self.rows_4bpp() as u32 * SWATCH_PIXELS;
        let mut pixels = Vec::with_capacity((width * 4 * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let index = (y / SWATCH_PIXELS * columns + x / SWATCH_PIXELS) as usize;
                pixels.extend_from_slice(&word_to_rgba(self.words[index]));
            }
        }
        super::png::rgba_png(width, height, &pixels)
    }
}
