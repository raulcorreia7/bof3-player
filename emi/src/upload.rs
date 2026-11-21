//! The type-3 load argument: where a VRAM upload lands.

use psx_formats::gfx::vram::Descriptor;

/// The bits of a load argument that a [`Descriptor`] owns: column, row and rectangles per row.
const PLACEMENT_BITS: u32 = 0x3f1f_3f00;

/// Unpacks a TOC load argument.
pub fn descriptor(load_argument: u32) -> Descriptor {
    Descriptor { column: ((load_argument >> 24) & 0x3f) as u8, row: ((load_argument >> 16) & 0x1f) as u8, chunks_per_row: ((load_argument >> 8) & 0x3f) as u8 }
}

/// `base` with the placement in the bits it owns; every other bit stays, so an unknown field survives an edit.
pub fn load_argument(descriptor: Descriptor, base: u32) -> u32 {
    base & !PLACEMENT_BITS | u32::from(descriptor.column) << 24 | u32::from(descriptor.row) << 16 | u32::from(descriptor.chunks_per_row) << 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_argument_round_trips_and_keeps_its_other_bits() {
        let argument = 0x1a08_0200 | 0x00c0_00ff;
        let placed = descriptor(argument);
        assert_eq!((placed.column, placed.row, placed.chunks_per_row), (0x1a, 0x08, 0x02));
        assert_eq!(load_argument(placed, argument), argument);
    }
}
