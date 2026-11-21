//! Field access in the byte orders the formats use.
//!
//! The VAB tables are little-endian and the SEP headers big-endian, so the
//! readers and writers live together here rather than once per module. Readers panic when `at`
//! runs past the slice; callers check lengths first.

use alloc::vec::Vec;

/// Reads a little-endian `u16`.
/// # Panics
///
/// Panics when `at + 2` runs past the slice.
pub fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// Reads a little-endian signed 16-bit field.
/// # Panics
///
/// Panics when `at + 2` runs past the slice.
pub fn i16_le(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// Reads a little-endian `u32`.
/// # Panics
///
/// Panics when `at + 4` runs past the slice.
pub fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Reads a little-endian `u32`, or `None` when `at + 4` runs past the slice.
pub fn try_u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32_le(bytes.get(at..at.checked_add(4)?)?, 0))
}

/// Reads a big-endian `u16`.
/// # Panics
///
/// Panics when `at + 2` runs past the slice.
pub fn u16_be(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

/// Reads a big-endian `u32`.
/// # Panics
///
/// Panics when `at + 4` runs past the slice.
pub fn u32_be(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Reads a big-endian 24-bit value.
/// # Panics
///
/// Panics when `at + 3` runs past the slice.
pub fn u24_be(bytes: &[u8], at: usize) -> u32 {
    (u32::from(bytes[at]) << 16) | (u32::from(bytes[at + 1]) << 8) | u32::from(bytes[at + 2])
}

/// Appends a little-endian `u16`.
pub fn push_u16_le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a little-endian signed 16-bit field.
pub fn push_i16_le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a little-endian `u32`.
pub fn push_u32_le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a big-endian `u16`.
pub fn push_u16_be(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends a big-endian `u32`.
pub fn push_u32_be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends a big-endian 24-bit value.
pub fn push_u24_be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes()[1..]);
}
