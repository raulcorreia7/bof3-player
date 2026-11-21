//! The two ways a 15-bit colour word becomes 8-bit channels; the retail outputs pin each.

/// The 8-bit channels of a colour word with each 5-bit channel widened by replicating its top bits, so 31 is 255.
pub fn expanded(word: u16) -> [u8; 3] {
    [0, 5, 10].map(|shift| expand_channel((word >> shift) & 0x1f))
}

/// The 8-bit channels of a colour word with each 5-bit channel shifted up by three (the GPU's modulation input), so 31 is 248.
pub fn shifted(word: u16) -> [u8; 3] {
    [0, 5, 10].map(|shift| (((word >> shift) & 0x1f) as u8) << 3)
}

/// One 5-bit channel widened by replicating its top three bits.
fn expand_channel(channel: u16) -> u8 {
    ((channel << 3) | (channel >> 2)) as u8
}
