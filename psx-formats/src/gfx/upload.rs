//! Conversion between a TIM file and a type-3 VRAM upload.
//! The TIM layout is Sony's *File Formats* manual, chapter 3 ("TIM"); the frame buffer is psx-spx "GPU Video Memory
//! (VRAM)".
//! ```
//! use psx_formats::gfx::upload::{from_tim, tim_direct};
//! use psx_formats::gfx::vram::{Descriptor, RECTANGLE_BYTES};
//!
//! let payload: Vec<u8> = (0..RECTANGLE_BYTES).map(|byte| byte as u8).collect();
//! let place = Descriptor { column: 2, row: 1, chunks_per_row: 1 };
//! let tim = tim_direct(place, &payload).unwrap();
//! let back = from_tim(&tim).unwrap();
//! assert_eq!((back.payload, back.placement), (payload, place));
//! ```

use super::texture::Depth;
use super::tim::{Block, File, Mode};
use super::vram::{self, Descriptor, RECTANGLE_SIDE};
use crate::error::{Error, Result};

/// The largest column a load argument names (6 bits).
const MAX_COLUMN: usize = 0x3f;
/// The largest row a load argument names (5 bits).
const MAX_ROW: usize = 0x1f;
/// The largest rectangles-per-row count a load argument names (6 bits).
const MAX_PER_ROW: usize = 0x3f;

/// A TIM read as a type-3 upload: the payload its image makes and the placement its header states.
/// ```
/// use psx_formats::gfx::upload::{Upload, from_tim, tim_direct};
/// use psx_formats::gfx::vram::{Descriptor, RECTANGLE_BYTES};
///
/// let tim = tim_direct(Descriptor { column: 0, row: 0, chunks_per_row: 1 }, &vec![0; RECTANGLE_BYTES]).unwrap();
/// let upload: Upload = from_tim(&tim).unwrap();
/// assert_eq!(upload.height, 32);
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upload {
    /// The type-3 payload.
    pub payload: Vec<u8>,
    /// The texel depth, or `None` for a 15-bit image.
    pub depth: Option<Depth>,
    /// The upload's place, from the image block's frame buffer position and width.
    pub placement: Descriptor,
    /// The image block's height in rows.
    pub height: u16,
    /// The CLUT block, when the file has one.
    pub clut: Option<Block>,
}

/// The TIM of an upload drawn with one CLUT row: the upload's words as the image and `clut_words` as a one-row CLUT at
/// (`clut_x`, `clut_y`) in the frame buffer.
/// ```
/// use psx_formats::gfx::texture::Depth;
/// use psx_formats::gfx::upload::tim_with_clut;
/// use psx_formats::gfx::vram::{Descriptor, RECTANGLE_BYTES};
///
/// let place = Descriptor { column: 22, row: 8, chunks_per_row: 1 };
/// let tim = tim_with_clut(place, &vec![0; RECTANGLE_BYTES], Depth::Four, 160, 483, &[0x8000; 16]).unwrap();
/// assert_eq!((tim.image.x, tim.image.y), (704, 256));
/// assert_eq!(tim.clut.unwrap().width, 16);
/// ```
pub fn tim_with_clut(
    descriptor: Descriptor,
    payload: &[u8],
    depth: Depth,
    clut_x: u16,
    clut_y: u16,
    clut_words: &[u16],
) -> Result<File> {
    let width = u16::try_from(clut_words.len()).map_err(|_| Error::Tim("a CLUT row is over 65535 words".into()))?;
    let clut = Block { x: clut_x, y: clut_y, width, height: 1, words: clut_words.to_vec() };
    let mode = match depth {
        Depth::Four => Mode::Four,
        Depth::Eight => Mode::Eight,
    };
    Ok(File { mode, clut: Some(clut), image: image_block(descriptor, payload)? })
}

/// The TIM of an upload with no CLUT: its words as a 15-bit image at the place its descriptor gives (the pixel mode `2`
/// has no CLUT block).
/// ```
/// use psx_formats::gfx::tim::Mode;
/// use psx_formats::gfx::upload::tim_direct;
/// use psx_formats::gfx::vram::{Descriptor, RECTANGLE_BYTES};
///
/// let tim = tim_direct(Descriptor { column: 0, row: 0, chunks_per_row: 1 }, &vec![0; RECTANGLE_BYTES]).unwrap();
/// assert_eq!((tim.mode, tim.clut), (Mode::Fifteen, None));
/// assert!(tim_direct(Descriptor { column: 0, row: 0, chunks_per_row: 1 }, &[0; 3]).is_err());
/// ```
pub fn tim_direct(descriptor: Descriptor, payload: &[u8]) -> Result<File> {
    Ok(File { mode: Mode::Fifteen, clut: None, image: image_block(descriptor, payload)? })
}

/// The image block of an upload: its word grid at its frame buffer place.
fn image_block(descriptor: Descriptor, payload: &[u8]) -> Result<Block> {
    let grid = vram::word_grid(payload, descriptor)?;
    let size = |value: usize| {
        u16::try_from(value).map_err(|_| Error::Tim("the upload is over 65535 words wide or tall".into()))
    };
    Ok(Block {
        x: descriptor.x(),
        y: descriptor.y(),
        width: size(grid.width)?,
        height: size(grid.height)?,
        words: grid.words,
    })
}

/// Reads a TIM as an upload: the image block's words are the payload, and its position and width are the placement.
/// ```
/// use psx_formats::gfx::tim::{Block, File, Mode};
/// use psx_formats::gfx::upload::from_tim;
///
/// let image = Block { x: 1, y: 0, width: 32, height: 32, words: vec![0; 1024] };
/// let off_grid = File { mode: Mode::Fifteen, clut: None, image };
/// assert!(from_tim(&off_grid).unwrap_err().to_string().contains("multiples of 32"));
/// ```
pub fn from_tim(file: &File) -> Result<Upload> {
    let image = &file.image;
    let side = RECTANGLE_SIDE;
    let (x, y) = (usize::from(image.x), usize::from(image.y));
    let (width, height) = (usize::from(image.width), usize::from(image.height));
    let shaped = [x, y, width, height].iter().all(|value| value % side == 0) && width != 0 && height != 0;
    if !shaped {
        return Err(Error::Tim(format!(
            "the image is {width} by {height} words at ({x}, {y}); an upload is whole {side} by {side} word \
             rectangles on a {side}-word grid, so size and place the image to multiples of {side}"
        )));
    }
    if x / side > MAX_COLUMN || y / side > MAX_ROW {
        return Err(Error::Tim(format!(
            "the image at ({x}, {y}) is past the place a load argument can name (column {MAX_COLUMN}, row {MAX_ROW})"
        )));
    }
    let chunks_per_row = Some(width / side)
        .filter(|per_row| *per_row <= MAX_PER_ROW)
        .ok_or_else(|| Error::Tim(format!("{width} words per row is more than a load argument can name")))?;
    if image.words.len() != width * height {
        return Err(Error::Tim(format!("the image holds {} words, not {width} by {height}", image.words.len())));
    }
    let placement =
        Descriptor { column: (x / side) as u8, row: (y / side) as u8, chunks_per_row: chunks_per_row as u8 };
    let payload = vram::payload_from_grid(width, height, &image.words)?;
    let depth = match file.mode {
        Mode::Four => Some(Depth::Four),
        Mode::Eight => Some(Depth::Eight),
        Mode::Fifteen => None,
    };
    Ok(Upload { payload, depth, placement, height: image.height, clut: file.clut.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-rectangle sheet at column 22, row 8, one rectangle to a row, with distinct words.
    const LOAD: u32 = (22 << 24) | (8 << 16) | (1 << 8);

    fn place() -> Descriptor {
        Descriptor::from_load_argument(LOAD)
    }

    fn sheet() -> Vec<u8> {
        (0..2 * 1024u16).flat_map(|word| word.wrapping_mul(7).to_le_bytes()).collect()
    }

    fn row() -> Vec<u16> {
        (0..16u16).map(|word| 0x8000 | word).collect()
    }

    #[test]
    fn a_joined_upload_goes_through_a_tim_and_back() {
        let tim = tim_with_clut(place(), &sheet(), Depth::Four, 160, 483, &row()).unwrap();
        let file = File::read(&tim.write()).unwrap();
        assert_eq!(file.mode, Mode::Four);
        assert_eq!((file.image.x, file.image.y, file.image.width, file.image.height), (704, 256, 32, 64));
        let clut = file.clut.as_ref().expect("a CLUT block");
        assert_eq!((clut.x, clut.y, clut.width, clut.height), (160, 483, 16, 1));

        let back = from_tim(&file).unwrap();
        assert_eq!(back.payload, sheet());
        assert_eq!(back.depth, Some(Depth::Four));
        assert_eq!(back.placement.load_argument(LOAD), LOAD);
        assert_eq!(back.clut.expect("the CLUT").words, row());
    }

    #[test]
    fn an_unjoined_upload_has_no_clut() {
        let tim = tim_direct(place(), &sheet()).unwrap();
        assert_eq!((tim.mode, tim.clut.clone()), (Mode::Fifteen, None));
        let back = from_tim(&File::read(&tim.write()).unwrap()).unwrap();
        assert_eq!((back.payload, back.depth, back.clut), (sheet(), None, None));
    }

    #[test]
    fn a_moved_image_changes_only_the_placement_bits() {
        let base = LOAD | 0x00c0_00ff;
        let mut file = tim_direct(Descriptor::from_load_argument(base), &sheet()).unwrap();
        file.image.x = 64;
        file.image.y = 32;
        let back = from_tim(&file).unwrap();
        assert_eq!(back.placement.load_argument(base), 0x0201_0100 | 0x00c0_00ff);
    }

    #[test]
    fn an_image_off_the_upload_grid_is_refused() {
        let mut file = tim_direct(place(), &sheet()).unwrap();
        file.image.x += 1;
        assert!(from_tim(&file).unwrap_err().to_string().contains("multiples of 32"));

        let mut small = tim_direct(place(), &sheet()).unwrap();
        small.image = Block { width: 16, height: 16, words: vec![0; 256], ..small.image };
        assert!(from_tim(&small).is_err());

        let mut far = tim_direct(place(), &sheet()).unwrap();
        far.image.x = 64 * 32;
        assert!(from_tim(&far).unwrap_err().to_string().contains("past the place"));

        let mut short = tim_direct(place(), &sheet()).unwrap();
        short.image.words.pop();
        assert!(from_tim(&short).is_err(), "the word count must match the size");
    }

    #[test]
    fn an_upload_that_is_not_whole_rectangles_is_refused() {
        assert!(tim_direct(place(), &[]).is_err());
        assert!(tim_direct(place(), &sheet()[..100]).is_err());
        assert!(tim_with_clut(place(), &sheet(), Depth::Eight, 0, 0, &vec![0; 70_000]).is_err());
    }

    #[test]
    fn damaged_tims_are_errors_and_never_panics() {
        let bytes = tim_with_clut(place(), &sheet(), Depth::Eight, 0, 480, &row()).unwrap().write();
        for cut in 0..bytes.len() {
            if let Ok(file) = File::read(&bytes[..cut]) {
                let _ = from_tim(&file);
            }
        }
        for at in 0..64 {
            for flip in [0x01, 0x20, 0xff] {
                let mut damaged = bytes.clone();
                damaged[at] ^= flip;
                if let Ok(file) = File::read(&damaged) {
                    let _ = from_tim(&file);
                }
            }
        }
    }
}
