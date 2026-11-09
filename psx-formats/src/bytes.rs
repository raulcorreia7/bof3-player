//! Field access in the byte orders the formats use.
//! ```
//! use psx_formats::bytes;
//!
//! let mut out = Vec::new();
//! bytes::push_u16_le(&mut out, 0x1234);
//! assert_eq!(bytes::u16_le(&out, 0), 0x1234);
//! ```

use alloc::vec::Vec;

/// Reads a little-endian `u16`.
/// ```
/// assert_eq!(psx_formats::bytes::u16_le(&[0x34, 0x12], 0), 0x1234);
/// ```
pub fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// Reads a little-endian signed 16-bit field.
/// ```
/// assert_eq!(psx_formats::bytes::i16_le(&[0xfe, 0xff], 0), -2);
/// ```
pub fn i16_le(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// Reads a little-endian `u32`.
/// ```
/// assert_eq!(psx_formats::bytes::u32_le(&[1, 0, 0, 0], 0), 1);
/// ```
pub fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Reads a big-endian `u16`.
/// ```
/// assert_eq!(psx_formats::bytes::u16_be(&[0x12, 0x34], 0), 0x1234);
/// ```
pub fn u16_be(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

/// Reads a big-endian `u32`.
/// ```
/// assert_eq!(psx_formats::bytes::u32_be(&[0, 0, 1, 0], 0), 0x100);
/// ```
pub fn u32_be(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Reads a big-endian 24-bit value.
/// ```
/// assert_eq!(psx_formats::bytes::u24_be(&[0x12, 0x34, 0x56], 0), 0x0012_3456);
/// ```
pub fn u24_be(bytes: &[u8], at: usize) -> u32 {
    (u32::from(bytes[at]) << 16) | (u32::from(bytes[at + 1]) << 8) | u32::from(bytes[at + 2])
}

/// Appends a little-endian `u16`.
/// ```
/// let mut out = Vec::new();
/// psx_formats::bytes::push_u16_le(&mut out, 0x1234);
/// assert_eq!(out, [0x34, 0x12]);
/// ```
pub fn push_u16_le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a little-endian signed 16-bit field.
/// ```
/// let mut out = Vec::new();
/// psx_formats::bytes::push_i16_le(&mut out, -2);
/// assert_eq!(out, [0xfe, 0xff]);
/// ```
pub fn push_i16_le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a little-endian `u32`.
/// ```
/// let mut out = Vec::new();
/// psx_formats::bytes::push_u32_le(&mut out, 1);
/// assert_eq!(out, [1, 0, 0, 0]);
/// ```
pub fn push_u32_le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a big-endian `u16`.
/// ```
/// let mut out = Vec::new();
/// psx_formats::bytes::push_u16_be(&mut out, 0x1234);
/// assert_eq!(out, [0x12, 0x34]);
/// ```
pub fn push_u16_be(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends a big-endian `u32`.
/// ```
/// let mut out = Vec::new();
/// psx_formats::bytes::push_u32_be(&mut out, 0x100);
/// assert_eq!(out, [0, 0, 1, 0]);
/// ```
pub fn push_u32_be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends a big-endian 24-bit value.
/// ```
/// let mut out = Vec::new();
/// psx_formats::bytes::push_u24_be(&mut out, 0x0012_3456);
/// assert_eq!(out, [0x12, 0x34, 0x56]);
/// ```
pub fn push_u24_be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes()[1..]);
}
