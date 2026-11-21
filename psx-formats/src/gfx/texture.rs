//! Texture-page and CLUT words of the PlayStation GPU, and the packed texel rows of
//! 4bpp and 8bpp textures.
//!
//! A textured primitive names the texture it samples with a `tpage` word and the
//! palette row it reads with a `clut` word. [`Tpage`] and [`Clut`] decode and encode
//! those words. [`texel_indices`] unpacks a texel row: each VRAM word holds
//! `16 / depth` texel indices, the lowest bits first. The codec knows nothing of
//! which texture or palette a draw uses; that association belongs to the caller.

/// VRAM words per texture-page column step (`tpage` bits 0-3).
pub const PAGE_COLUMN_WORDS: u16 = 64;
/// VRAM rows between the two texture-page rows (`tpage` bit 4).
pub const PAGE_ROW_ROWS: u16 = 256;
/// VRAM words per CLUT column step: a CLUT starts at a multiple of this.
pub const CLUT_COLUMN_WORDS: u16 = 16;
/// The CLUT word holds the VRAM row from this bit up (`clut` bits 6-14).
const CLUT_ROW_SHIFT: u32 = 6;
/// Bits of the CLUT word that hold the VRAM row.
const CLUT_ROW_MASK: u16 = 0x1ff;
/// The `tpage` bit that selects the lower page row.
const PAGE_ROW_BIT: u16 = 0x10;
/// The `tpage` bits that hold the texture color mode (bits 7-8).
const COLOR_SHIFT: u32 = 7;
/// The `tpage` bits that hold the semi-transparency mode (bits 5-6).
const BLEND_SHIFT: u32 = 5;
/// Bits 0-8 of a `tpage` word: the fields this module reads and writes.
const TPAGE_FIELD_MASK: u16 = 0x1ff;

/// The bit depth of a palettized texel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Depth {
    /// Four-bit indices into a 16-color CLUT row.
    Four,
    /// Eight-bit indices into a 256-color CLUT row.
    Eight,
}

impl Depth {
    /// Bits per texel index.
    pub fn bits(self) -> u32 {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
        }
    }

    /// Texel indices packed in one VRAM word.
    pub fn texels_per_word(self) -> usize {
        (u16::BITS / self.bits()) as usize
    }

    /// Colors in one CLUT row of this depth.
    pub fn colors(self) -> usize {
        1 << self.bits()
    }
}

/// The color mode a texture page selects (`tpage` bits 7-8).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    /// Palettized texels read through a CLUT.
    Indexed(Depth),
    /// Direct 15-bit texels; no CLUT is read.
    Direct15,
}

/// The fields of a texture-page word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tpage {
    /// The page's left edge in VRAM words.
    pub x: u16,
    /// The page's top edge in VRAM rows.
    pub y: u16,
    /// The texel color mode.
    pub color: Color,
    /// The semi-transparency mode (bits 5-6), `0` to `3`.
    pub blend: u8,
}

impl Tpage {
    /// Decodes a texture-page word. Returns `None` for color mode `3`, which the GPU
    /// reserves. Dither and display bits (9-15) are not part of the model.
    pub fn from_word(word: u16) -> Option<Self> {
        let color = match (word >> COLOR_SHIFT) & 3 {
            0 => Color::Indexed(Depth::Four),
            1 => Color::Indexed(Depth::Eight),
            2 => Color::Direct15,
            _ => return None,
        };
        Some(Self { x: (word & 0xf) * PAGE_COLUMN_WORDS, y: if word & PAGE_ROW_BIT != 0 { PAGE_ROW_ROWS } else { 0 }, color, blend: ((word >> BLEND_SHIFT) & 3) as u8 })
    }

    /// Encodes the fields back to a texture-page word (bits 0-8 only).
    pub fn word(self) -> u16 {
        let color = match self.color {
            Color::Indexed(Depth::Four) => 0,
            Color::Indexed(Depth::Eight) => 1,
            Color::Direct15 => 2,
        };
        let page_row = if self.y >= PAGE_ROW_ROWS { PAGE_ROW_BIT } else { 0 };
        let page_column = self.x / PAGE_COLUMN_WORDS;
        ((color << COLOR_SHIFT) | (u16::from(self.blend) << BLEND_SHIFT) | page_row | page_column) & TPAGE_FIELD_MASK
    }
}

/// A CLUT's VRAM position: `x` in words (a multiple of 16) and `y` in rows.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Clut {
    /// The first color's VRAM column in words.
    pub x: u16,
    /// The VRAM row.
    pub y: u16,
}

impl Clut {
    /// Decodes a `clut` word (`x / 16` in bits 0-5, the row in bits 6-14).
    pub fn from_word(word: u16) -> Self {
        Self { x: (word & 0x3f) * CLUT_COLUMN_WORDS, y: (word >> CLUT_ROW_SHIFT) & CLUT_ROW_MASK }
    }

    /// The position as a `clut` word, or `None` when `x` is not a multiple of 16 or
    /// either field is outside VRAM.
    pub fn new(x: u16, y: u16) -> Option<Self> {
        let in_vram = usize::from(x) < crate::gfx::vram::VRAM_WIDTH && usize::from(y) < crate::gfx::vram::VRAM_HEIGHT;
        (x.is_multiple_of(CLUT_COLUMN_WORDS) && in_vram).then_some(Self { x, y })
    }

    /// Encodes the position as a `clut` word.
    pub fn word(self) -> u16 {
        (self.x / CLUT_COLUMN_WORDS) | (self.y << CLUT_ROW_SHIFT)
    }
}

/// Packs texel indices into VRAM words, lowest bits first: the inverse of
/// [`texel_indices`]. A short last word keeps its low texels and reads zero above
/// them.
pub fn texel_words(indices: &[u8], depth: Depth) -> Vec<u16> {
    let bits = depth.bits();
    let mask = (1u16 << bits) - 1;
    indices.chunks(depth.texels_per_word()).map(|texels| texels.iter().enumerate().fold(0u16, |word, (texel, &index)| word | ((u16::from(index) & mask) << (texel as u32 * bits)))).collect()
}

/// Unpacks the texel indices of a row of VRAM words, lowest bits first.
pub fn texel_indices(words: &[u16], depth: Depth) -> Vec<u8> {
    let bits = depth.bits();
    let mask = (1u16 << bits) - 1;
    let mut indices = Vec::with_capacity(words.len() * depth.texels_per_word());
    for word in words {
        for texel in 0..depth.texels_per_word() {
            indices.push(((word >> (texel as u32 * bits)) & mask) as u8);
        }
    }
    indices
}

/// The index texel `u` selects from its VRAM word, lowest bits first.
pub fn texel_index(word: u16, u: usize, depth: Depth) -> usize {
    let bits = depth.bits() as usize;
    (usize::from(word) >> ((u % depth.texels_per_word()) * bits)) & ((1 << bits) - 1)
}

/// The colour word the GPU reads at texel `uv` of the page at `page` through `clut`, without a texture window
/// (psx-spx "GPU Texture Caching"). Coordinates wrap at the VRAM edges; a VRAM shorter than 1024x512 reads zero.
pub fn fetch(vram: &[u16], page: [u16; 2], clut: Clut, color: Color, uv: [u8; 2]) -> u16 {
    use super::vram::{VRAM_HEIGHT, VRAM_WIDTH};
    if vram.len() < VRAM_WIDTH * VRAM_HEIGHT {
        return 0;
    }
    let [u, v] = uv.map(usize::from);
    let per_word = match color {
        Color::Indexed(depth) => depth.texels_per_word(),
        Color::Direct15 => 1,
    };
    let word = vram[(usize::from(page[1]) + v) % VRAM_HEIGHT * VRAM_WIDTH + (usize::from(page[0]) + u / per_word) % VRAM_WIDTH];
    let Color::Indexed(depth) = color else { return word };
    vram[usize::from(clut.y) % VRAM_HEIGHT * VRAM_WIDTH + (usize::from(clut.x) + texel_index(word, u, depth)) % VRAM_WIDTH]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_reads_a_texel_through_its_clut() {
        let mut vram = vec![0u16; 1024 * 512];
        vram[256 * 1024 + 64] = 0x4321;
        vram[500 * 1024 + 16 + 3] = 0x1234;
        let clut = Clut { x: 16, y: 500 };
        assert_eq!(fetch(&vram, [64, 256], clut, Color::Indexed(Depth::Four), [2, 0]), 0x1234);
        assert_eq!(fetch(&vram, [64, 256], clut, Color::Direct15, [0, 0]), 0x4321);
        assert_eq!(fetch(&[], [0, 0], clut, Color::Direct15, [0, 0]), 0, "an empty VRAM reads zero");
    }
}
