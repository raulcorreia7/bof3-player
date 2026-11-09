//! Graphics: VRAM uploads, CLUT blocks, texture-page words, the standard TIM file, the conversion between a TIM and an
//! upload, and the PNG output they share.
//! ```
//! use psx_formats::gfx::clut;
//!
//! assert_eq!(clut::word_to_rgba(0x001f), [255, 0, 0, 255]);
//! ```

pub mod clut;
pub mod png;
pub mod texture;
pub mod tim;
pub mod upload;
pub mod vram;
