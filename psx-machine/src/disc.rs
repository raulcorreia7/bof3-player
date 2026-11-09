// SPDX-License-Identifier: MIT
//! The disc the CD-ROM drive reads: raw sectors by address, and the track list.

use std::path::Path;

/// Bytes of a raw sector: sync, header, subheader, data and error correction.
pub const SECTOR_SIZE: usize = 2352;
/// Sectors before logical address 0 (the two-second lead-in, 00:02:00).
pub const LEAD_IN: u32 = 150;

/// One track of the disc.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Track {
    /// Track number, from 1.
    pub number: u8,
    /// First sector, as a logical address.
    pub start: u32,
    /// Whether it is a CD-DA track.
    pub audio: bool,
}

/// Something the drive can read sectors from.
pub trait Disc {
    /// The raw sector at logical address `lba` (0 is 00:02:00), or `None` past the end.
    fn sector(&mut self, lba: u32) -> Option<Box<[u8; SECTOR_SIZE]>>;

    /// The tracks, in order.
    fn tracks(&self) -> Vec<Track>;

    /// The first sector past the last track.
    fn end(&self) -> u32;
}

/// A disc image opened through `psx-cd`: a CUE sheet, a raw `.bin` or a cooked `.iso`.
pub struct ImageDisc {
    image: psx_cd::Image,
    tracks: Vec<Track>,
    end: u32,
}

impl ImageDisc {
    /// Opens the image at `path`.
    ///
    /// # Errors
    ///
    /// The `psx-cd` error when the image cannot be opened or its CUE sheet read.
    pub fn open(path: impl AsRef<Path>) -> psx_cd::Result<Self> {
        let image = psx_cd::Image::open(path.as_ref())?;
        let mut tracks = Vec::new();
        let mut end = 0;
        if let Some(sheet) = image.cue_sheet()? {
            let mut file_base = 0u32;
            let mut current_file = None;
            for track in &sheet.tracks {
                if current_file.as_ref() != Some(&track.file) {
                    if let Some(previous) = &current_file {
                        file_base += file_sectors(previous);
                    }
                    current_file = Some(track.file.clone());
                }
                tracks.push(Track { number: track.number, start: file_base + track.index01, audio: track.mode == psx_cd::cue::TrackMode::Audio });
            }
            if let Some(file) = &current_file {
                end = file_base + file_sectors(file);
            }
        } else {
            let length = std::fs::metadata(path.as_ref())?.len();
            let size = if image.is_raw() { SECTOR_SIZE as u64 } else { psx_cd::ISO_SECTOR_SIZE };
            end = (length / size) as u32;
            tracks.push(Track { number: 1, start: 0, audio: false });
        }
        Ok(Self { image, tracks, end })
    }

    /// The image, for reading its file table.
    pub fn image_mut(&mut self) -> &mut psx_cd::Image {
        &mut self.image
    }
}

fn file_sectors(path: &Path) -> u32 {
    std::fs::metadata(path).map_or(0, |meta| (meta.len() / SECTOR_SIZE as u64) as u32)
}

impl Disc for ImageDisc {
    fn sector(&mut self, lba: u32) -> Option<Box<[u8; SECTOR_SIZE]>> {
        let data_end = self.tracks.iter().find(|track| track.audio).map_or(self.end, |track| track.start);
        if lba >= data_end {
            return None;
        }
        let mut sector = Box::new([0u8; SECTOR_SIZE]);
        let entry = psx_cd::Entry { path: "".into(), lba, size: psx_cd::ISO_SECTOR_SIZE as u32, is_directory: false, extended_attributes: 0, is_xa: true, xa: None };
        if self.image.is_raw() {
            self.image.read_entry_range(&entry, 0, &mut sector[16..]).ok()?;
        } else {
            self.image.read_entry_range(&entry, 0, &mut sector[24..24 + 2048]).ok()?;
        }
        write_header(&mut sector, lba);
        Some(sector)
    }

    fn tracks(&self) -> Vec<Track> {
        self.tracks.clone()
    }

    fn end(&self) -> u32 {
        self.end
    }
}

/// Fills the sync pattern and the header (minute, second, frame in BCD, mode 2) of a sector.
pub fn write_header(sector: &mut [u8; SECTOR_SIZE], lba: u32) {
    sector[0] = 0;
    sector[1..11].fill(0xFF);
    sector[11] = 0;
    let (minute, second, frame) = to_msf(lba);
    sector[12] = bcd(minute);
    sector[13] = bcd(second);
    sector[14] = bcd(frame);
    sector[15] = 2;
}

/// Minute, second and frame of logical address `lba`.
#[must_use]
pub fn to_msf(lba: u32) -> (u8, u8, u8) {
    let absolute = lba + LEAD_IN;
    ((absolute / 4500) as u8, ((absolute / 75) % 60) as u8, (absolute % 75) as u8)
}

/// Logical address of a minute, second and frame (binary, not BCD).
#[must_use]
pub fn from_msf(minute: u8, second: u8, frame: u8) -> u32 {
    (u32::from(minute) * 60 + u32::from(second)) * 75 + u32::from(frame)
}

/// A byte in binary-coded decimal.
#[must_use]
pub fn bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}

/// The binary value of a BCD byte.
#[must_use]
pub fn from_bcd(value: u8) -> u8 {
    (value >> 4) * 10 + (value & 0xF)
}

/// Sectors held in memory, for tests and tools.
#[derive(Clone, Debug, Default)]
pub struct MemoryDisc {
    /// The data sectors from logical address 0, each 2352 bytes with its header filled in.
    pub sectors: Vec<Box<[u8; SECTOR_SIZE]>>,
}

impl MemoryDisc {
    /// A disc whose data track holds `data` as Mode 2 Form 1 sectors of 2048 bytes.
    #[must_use]
    pub fn from_data(data: &[u8]) -> Self {
        let sectors = data
            .chunks(2048)
            .enumerate()
            .map(|(index, chunk)| {
                let mut sector = Box::new([0u8; SECTOR_SIZE]);
                write_header(&mut sector, index as u32);
                sector[24..24 + chunk.len()].copy_from_slice(chunk);
                sector
            })
            .collect();
        Self { sectors }
    }
}

impl Disc for MemoryDisc {
    fn sector(&mut self, lba: u32) -> Option<Box<[u8; SECTOR_SIZE]>> {
        self.sectors.get(lba as usize).cloned()
    }

    fn tracks(&self) -> Vec<Track> {
        vec![Track { number: 1, start: 0, audio: false }]
    }

    fn end(&self) -> u32 {
        self.sectors.len() as u32
    }
}
