//! The SEQ file: one sequence with its own header.
//! Sony's *File Formats* manual (chapter 4, "SEQ") gives the file as `pQES`, a 32-bit version, the resolution (16
//! bits), the tempo (24 bits), the rhythm (numerator and denominator bytes) and the score data to the end of the file.
//! The manual gives the version field as four bytes and states no value; Sony's sequencer files carry 1, which is what
//! [`encode`] writes.
//! ```
//! use psx_formats::sep::{Event, Package, Sequence, seq};
//!
//! let end = Event { status: 0xff, meta_type: 0x2f, status_byte: true, ..Event::default() };
//! let sequence = Sequence { seq_id: 7, resolution: 48, events: vec![end], ..Sequence::default() };
//! let file = seq::encode(&sequence);
//! assert_eq!(&file[..4], b"pQES");
//! assert_eq!(seq::read(&file, 7).unwrap(), sequence);
//! ```

use alloc::vec::Vec;

use super::{FILE_HEADER, Package, SEP_MAGIC, SEQ_HEADER, Sequence};
use crate::error::{Error, Result};

/// The version a SEQ file declares when written.
pub const VERSION: u32 = 1;
/// A SEQ file's header: magic, version, resolution, tempo and rhythm.
pub const HEADER_BYTES: usize = 15;
/// Offset of the resolution, tempo and rhythm (7 bytes) in a SEQ file.
const SHARED_AT: usize = 8;
/// Bytes of a sequence header from the resolution to the rhythm; a SEQ file shares them.
const SHARED_BYTES: usize = 7;
/// Offset of the shared bytes inside a package's sequence header (after the 16-bit id).
const SEQUENCE_SHARED_AT: usize = 2;

/// One sequence as a SEQ file.
/// ```
/// let file = psx_formats::sep::seq::encode(&Default::default());
/// assert_eq!(file.len(), psx_formats::sep::seq::HEADER_BYTES, "a header and no events");
/// ```
pub fn encode(sequence: &Sequence) -> Vec<u8> {
    let single = Package { sequences: alloc::vec![sequence.clone()] }.encode();
    let header = &single[FILE_HEADER..FILE_HEADER + SEQ_HEADER];
    let mut file = Vec::with_capacity(single.len());
    file.extend_from_slice(SEP_MAGIC);
    file.extend_from_slice(&VERSION.to_be_bytes());
    file.extend_from_slice(&header[SEQUENCE_SHARED_AT..SEQUENCE_SHARED_AT + SHARED_BYTES]);
    file.extend_from_slice(&single[FILE_HEADER + SEQ_HEADER..]);
    file
}

/// Sequence `index` of a stored package as a SEQ file.
/// ```
/// use psx_formats::sep::{Package, seq};
///
/// let package = Package { sequences: vec![Default::default()] }.encode();
/// assert!(seq::export(&package, 0).is_ok());
/// assert!(seq::export(&package, 1).is_err());
/// ```
pub fn export(package: &[u8], index: usize) -> Result<Vec<u8>> {
    let parsed = Package::parse(package)?;
    let count = parsed.sequences.len();
    let sequence = parsed.sequences.get(index).ok_or(Error::SepSequenceOutOfRange { index, count })?;
    Ok(encode(sequence))
}

/// The sequence a SEQ file holds, given the id its package assigns.
/// ```
/// use psx_formats::sep::seq;
///
/// assert!(seq::read(b"MThd", 0).is_err());
/// ```
pub fn read(file: &[u8], seq_id: u16) -> Result<Sequence> {
    if file.len() < HEADER_BYTES || &file[..4] != SEP_MAGIC {
        return Err(Error::SeqFile("it does not open with the pQES magic and a header"));
    }
    let score = &file[HEADER_BYTES..];
    let size = u32::try_from(score.len()).map_err(|_| Error::SeqFile("the score is over 4 GiB"))?;
    let mut single = Vec::with_capacity(FILE_HEADER + SEQ_HEADER + score.len());
    single.extend_from_slice(SEP_MAGIC);
    single.extend_from_slice(&0u16.to_be_bytes());
    single.extend_from_slice(&seq_id.to_be_bytes());
    single.extend_from_slice(&file[SHARED_AT..SHARED_AT + SHARED_BYTES]);
    single.extend_from_slice(&size.to_be_bytes());
    single.extend_from_slice(score);
    let mut package = Package::validate(&single)?;
    package.sequences.pop().ok_or(Error::SeqFile("the file holds no sequence"))
}

/// A package from SEQ files, one sequence per part and in order, each with the id the package gives it.
/// ```
/// use psx_formats::sep::{Event, Sequence, seq};
///
/// let end = Event { status: 0xff, meta_type: 0x2f, status_byte: true, ..Event::default() };
/// let file = seq::encode(&Sequence { events: vec![end], ..Sequence::default() });
/// let package = seq::package(&[(&file, 3)]).unwrap();
/// assert_eq!(package.sequences[0].seq_id, 3);
/// ```
pub fn package(parts: &[(&[u8], u16)]) -> Result<Package> {
    let sequences = parts.iter().map(|&(file, seq_id)| read(file, seq_id)).collect::<Result<Vec<_>>>()?;
    Ok(Package { sequences })
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::sep::Event;

    fn note(delta: u32, status: u8) -> Event {
        Event { delta, status, data1: 60, data2: 100, status_byte: true, ..Event::default() }
    }

    fn end() -> Event {
        Event { status: 0xff, meta_type: 0x2f, status_byte: true, ..Event::default() }
    }

    fn sequence(seq_id: u16) -> Sequence {
        Sequence {
            seq_id,
            resolution: 48,
            tempo_us: 500_000,
            time_num: 4,
            time_den: 2,
            events: vec![note(0, 0x90), note(24, 0x80), end()],
        }
    }

    fn stored() -> Vec<u8> {
        Package { sequences: vec![sequence(0), sequence(7)] }.encode()
    }

    #[test]
    fn a_sequence_is_a_seq_file_with_the_manual_layout() {
        let file = export(&stored(), 1).unwrap();
        assert_eq!(&file[..4], b"pQES");
        assert_eq!(&file[4..8], &[0, 0, 0, 1], "version, 4 bytes");
        assert_eq!(&file[8..10], &48u16.to_be_bytes(), "resolution of a quarter note");
        assert_eq!(&file[10..13], &[0x07, 0xa1, 0x20], "tempo, 3 bytes");
        assert_eq!(&file[13..15], &[4, 2], "rhythm");
        assert_eq!(&file[file.len() - 3..], &[0xff, 0x2f, 0x00], "end of SEQ");
    }

    #[test]
    fn seq_files_rebuild_the_package_with_their_ids() {
        let package = stored();
        let first = export(&package, 0).unwrap();
        let second = export(&package, 1).unwrap();
        assert_eq!(self::package(&[(&first, 0), (&second, 7)]).unwrap().encode(), package);
        assert_eq!(export(&package, 2), Err(Error::SepSequenceOutOfRange { index: 2, count: 2 }));
    }

    #[test]
    fn a_file_that_is_not_a_seq_is_refused() {
        assert_eq!(read(b"MThd", 0), Err(Error::SeqFile("it does not open with the pQES magic and a header")));
        let mut broken = export(&stored(), 0).unwrap();
        broken.truncate(broken.len() - 1);
        assert!(read(&broken, 0).is_err(), "the end of SEQ is cut");
        let no_end = encode(&Sequence { events: vec![note(0, 0x90)], ..sequence(0) });
        assert!(matches!(read(&no_end, 0), Err(Error::SepRule(_))), "no end of track");
    }

    #[test]
    fn damaged_input_is_an_error_and_never_a_panic() {
        let file = export(&stored(), 1).unwrap();
        for cut in 0..file.len() {
            let _ = read(&file[..cut], 0);
        }
        for at in 0..file.len() {
            for flip in [0x01, 0x80, 0xff] {
                let mut bytes = file.clone();
                bytes[at] ^= flip;
                let _ = read(&bytes, 0);
            }
        }
        let stored = stored();
        for cut in 0..stored.len() {
            let _ = export(&stored[..cut], 0);
            let _ = Package::validate(&stored[..cut]);
        }
    }
}
