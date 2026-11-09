//! The standard VAB file: header and body in one file.
//!
//! Sony's *File Formats* manual (chapter 4, "VAB") defines the bank as a header (`.vh`) and a body
//! (`.vb`) of SPU ADPCM samples, and the sound tools also exchange them as one `.vab` file, the
//! header followed by the body. The header's length is fixed by its program count (the 16-bit field at
//! offset `0x12`): the 32-byte header, 128 program records, 16 tones of 32 bytes for each program, and
//! the 256-entry sample size table (see [`super::Bank::header_size`]). The file therefore needs no
//! separator, and [`split`] finds the body by reading that count.
//!
//! # Examples
//!
//! ```
//! use psx_formats::vab::{encode_bank, file};
//!
//! let (header, body) = encode_bank(0, &[], &[vec![0u8; 16]]).unwrap();
//! let whole = file::join(&header, &body);
//! assert_eq!(file::split(&whole).unwrap(), (&header[..], &body[..]));
//! ```

use alloc::vec::Vec;

use super::{Bank, SIZE_ENTRIES, TONE_TABLE, VAB_MAGIC, tone_bytes};
use crate::bytes::u16_le;
use crate::error::{Error, Result};

/// Offset of the 16-bit program count in the header.
const PROGRAM_COUNT_AT: usize = 0x12;

/// The bank as one VAB file: the header followed by the body.
///
/// # Examples
///
/// ```
/// assert_eq!(psx_formats::vab::file::join(b"ab", b"cd"), b"abcd");
/// ```
pub fn join(header: &[u8], body: &[u8]) -> Vec<u8> {
    let mut whole = Vec::with_capacity(header.len() + body.len());
    whole.extend_from_slice(header);
    whole.extend_from_slice(body);
    whole
}

/// A VAB file split into its header (`.vh`) and body (`.vb`) at the header's own length, and
/// checked with [`check`]: both halves must parse as a bank and the header must encode to the bytes
/// it holds.
///
/// # Examples
///
/// ```
/// use psx_formats::{Error, vab::file};
///
/// assert_eq!(file::split(b"MThd"), Err(Error::VabFile("not a VAB file: it does not open with the pBAV header")));
/// ```
pub fn split(file: &[u8]) -> Result<(&[u8], &[u8])> {
    let count = file
        .get(PROGRAM_COUNT_AT..PROGRAM_COUNT_AT + 2)
        .filter(|_| file.starts_with(VAB_MAGIC))
        .map(|_| u16_le(file, PROGRAM_COUNT_AT))
        .ok_or(Error::VabFile("not a VAB file: it does not open with the pBAV header"))?;
    let size = TONE_TABLE + tone_bytes(count) + SIZE_ENTRIES * 2;
    if file.len() < size {
        return Err(Error::VabHeaderSize { length: file.len(), expected: size });
    }
    let (header, body) = file.split_at(size);
    check(header, body)?;
    Ok((header, body))
}

/// The parsed bank when `header` and `body` are a bank: the header parses against the body,
/// encodes back to its own bytes, and the samples it sizes fit the body.
///
/// # Examples
///
/// ```
/// use psx_formats::vab::{encode_bank, file};
///
/// let (header, body) = encode_bank(0, &[], &[vec![0u8; 32]]).unwrap();
/// assert_eq!(file::check(&header, &body).unwrap().vag_count, 1);
/// assert!(file::check(&header, &body[..16]).is_err(), "the samples run past the body");
/// ```
pub fn check(header: &[u8], body: &[u8]) -> Result<Bank> {
    let bank = Bank::parse(header, body)?;
    if bank.encode_header() != header {
        return Err(Error::VabFile("the encoded header is not the header"));
    }
    let sized: usize = bank.vags.iter().map(|vag| vag.size as usize).sum();
    if sized > body.len() {
        return Err(Error::VabSamplesPastBody { sized, body: body.len() });
    }
    Ok(bank)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::vab::encode_bank;

    fn bank() -> (Vec<u8>, Vec<u8>) {
        encode_bank(3, &[], &[vec![0u8; 16], vec![1u8; 32]]).expect("a bank")
    }

    #[test]
    fn a_file_is_its_header_and_body() {
        let (header, body) = bank();
        let whole = join(&header, &body);
        assert_eq!(split(&whole).unwrap(), (&header[..], &body[..]));
    }

    #[test]
    fn a_pair_that_is_not_a_bank_is_refused() {
        let (header, body) = bank();
        assert!(split(b"MThd").is_err());
        assert!(split(&header[..100]).is_err());
        assert_eq!(check(&header, &body[..body.len() - 8]), Err(Error::VabSamplesPastBody { sized: 48, body: 40 }));
        let mut dirty = header.clone();
        dirty[0x1c] = 1;
        assert!(check(&dirty, &body).is_ok(), "reserved bytes are kept, so the header still round-trips");
    }

    #[test]
    fn damaged_input_is_an_error_and_never_a_panic() {
        let (header, body) = bank();
        let whole = join(&header, &body);
        for cut in (0..whole.len()).step_by(7) {
            let _ = split(&whole[..cut]);
        }
        for at in (0..header.len()).step_by(3) {
            let mut bytes = whole.clone();
            bytes[at] ^= 0xff;
            let _ = split(&bytes);
        }
        let mut huge = whole.clone();
        huge[PROGRAM_COUNT_AT] = 0xff;
        huge[PROGRAM_COUNT_AT + 1] = 0xff;
        assert!(split(&huge).is_err(), "a program count of 65535 needs a header the file lacks");
    }
}
