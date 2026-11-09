// SPDX-License-Identifier: MIT
//! The CD-ROM controller at 0x1F801800: its four indexed ports, the command set the PsyQ library uses, the response and
//! interrupt sequence, and sector reads into the data FIFO for the CPU or DMA channel 3 (psx-spx "CDROM Controller I/O
//! Ports", "CDROM Controller Command Summary", "CDROM - Response Timings").

use std::collections::VecDeque;

use crate::disc::{self, Disc, SECTOR_SIZE};
use crate::irq::{Irq, Line};

/// Cycles from a command to its first response (psx-spx gives about 0xC4E1 on average).
const FIRST_RESPONSE: u64 = 0xC4E1;
/// Cycles from the first response to the second for the slow commands.
const SECOND_RESPONSE: u64 = 0x1_0000;
/// Cycles per sector at single speed: the CPU clock over 75 sectors a second.
const SECTOR_CYCLES: u64 = crate::timers::CPU_HZ / 75;

const STAT_ERROR: u8 = 1 << 0;
const STAT_MOTOR: u8 = 1 << 1;
const STAT_READING: u8 = 1 << 5;
const STAT_SEEKING: u8 = 1 << 6;
const STAT_PLAYING: u8 = 1 << 7;

const MODE_DOUBLE_SPEED: u8 = 1 << 7;
const MODE_XA_ADPCM: u8 = 1 << 6;
const MODE_WHOLE_SECTOR: u8 = 1 << 5;
const MODE_XA_FILTER: u8 = 1 << 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Activity {
    Idle,
    Reading,
    Playing,
}

#[derive(Clone, Debug)]
struct Pending {
    due: u64,
    interrupt: u8,
    response: Vec<u8>,
    /// A sector that becomes readable with this interrupt.
    sector: Option<Box<[u8; SECTOR_SIZE]>>,
    /// The second half of a command: set the stat byte when it is delivered.
    finish: Option<Activity>,
}

/// A command the code sent, for the run log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandRecord {
    /// Vertical blanks counted before the command.
    pub frame: u64,
    /// The command byte.
    pub command: u8,
    /// Its parameters.
    pub parameters: Vec<u8>,
}

/// The CD-ROM controller and the disc in it.
pub struct Cdrom {
    disc: Option<Box<dyn Disc>>,
    index: u8,
    parameters: VecDeque<u8>,
    response: VecDeque<u8>,
    data: VecDeque<u8>,
    enable: u8,
    flags: u8,
    pending: VecDeque<Pending>,
    /// The sector last delivered, waiting for a request to load it into the data FIFO.
    sector: Option<Box<[u8; SECTOR_SIZE]>>,
    mode: u8,
    activity: Activity,
    motor: bool,
    seek_target: u32,
    position: u32,
    next_sector_due: u64,
    filter: (u8, u8),
    busy_until: u64,
    now: u64,
    /// Commands received, in order.
    pub commands: Vec<CommandRecord>,
    /// XA audio sectors the drive sent to the SPU instead of the CPU.
    pub xa_sectors: u64,
    /// Data sectors delivered to the CPU.
    pub sectors_read: u64,
    /// The frame number stamped on new log entries.
    pub frame: u64,
}

impl Default for Cdrom {
    fn default() -> Self {
        Self::new()
    }
}

impl Cdrom {
    /// A drive with no disc.
    #[must_use]
    pub fn new() -> Self {
        Self {
            disc: None,
            index: 0,
            parameters: VecDeque::new(),
            response: VecDeque::new(),
            data: VecDeque::new(),
            // The BIOS boot leaves every interrupt enabled; libcd writes the register only to clear flags left pending.
            enable: 0x1F,
            flags: 0,
            pending: VecDeque::new(),
            sector: None,
            mode: 0,
            activity: Activity::Idle,
            motor: true,
            seek_target: 0,
            position: 0,
            next_sector_due: 0,
            filter: (0, 0),
            busy_until: 0,
            now: 0,
            commands: Vec::new(),
            xa_sectors: 0,
            sectors_read: 0,
            frame: 0,
        }
    }

    /// Puts a disc in the drive.
    pub fn insert(&mut self, disc: Box<dyn Disc>) {
        self.disc = Some(disc);
    }

    /// The disc in the drive.
    pub fn disc_mut(&mut self) -> Option<&mut (dyn Disc + 'static)> {
        self.disc.as_deref_mut()
    }

    fn stat(&self) -> u8 {
        let mut stat = if self.motor { STAT_MOTOR } else { 0 };
        stat |= match self.activity {
            Activity::Idle => 0,
            Activity::Reading => STAT_READING,
            Activity::Playing => STAT_PLAYING,
        };
        if self.disc.is_none() {
            stat |= 1 << 4;
        }
        stat
    }

    /// Reads port `port` (0 to 3) at CPU cycle `now`.
    pub fn read(&mut self, port: u32, now: u64) -> u8 {
        self.now = now;
        match port {
            0 => {
                let mut status = self.index;
                status |= u8::from(self.parameters.is_empty()) << 3;
                status |= u8::from(self.parameters.len() < 16) << 4;
                status |= u8::from(!self.response.is_empty()) << 5;
                status |= u8::from(!self.data.is_empty()) << 6;
                status |= u8::from(now < self.busy_until) << 7;
                status
            }
            1 => self.response.pop_front().unwrap_or(0),
            2 => self.data.pop_front().unwrap_or(0),
            _ => {
                if self.index & 1 == 0 {
                    0xE0 | self.enable
                } else {
                    0xE0 | self.flags
                }
            }
        }
    }

    /// One word of the data FIFO, as DMA channel 3 reads it.
    pub fn read_data_word(&mut self) -> u32 {
        let mut word = [0u8; 4];
        for byte in &mut word {
            *byte = self.data.pop_front().unwrap_or(0);
        }
        u32::from_le_bytes(word)
    }

    /// Writes port `port` (0 to 3) at CPU cycle `now`.
    pub fn write(&mut self, port: u32, value: u8, now: u64) {
        self.now = now;
        match (port, self.index) {
            (0, _) => self.index = value & 3,
            (1, 0) => self.command(value),
            (2, 0) => {
                if self.parameters.len() < 16 {
                    self.parameters.push_back(value);
                }
            }
            (2, 1) => self.enable = value & 0x1F,
            (3, 0) => {
                if value & 0x80 != 0 {
                    if self.data.is_empty()
                        && let Some(sector) = self.sector.take()
                    {
                        let range = if self.mode & MODE_WHOLE_SECTOR != 0 { 12..12 + 0x924 } else { 24..24 + 0x800 };
                        self.data.extend(&sector[range]);
                    }
                } else {
                    self.data.clear();
                }
            }
            (3, 1) => {
                self.flags &= !(value & 0x1F);
                if value & 0x40 != 0 {
                    self.parameters.clear();
                }
                if self.flags == 0 {
                    self.response.clear();
                }
            }
            _ => {}
        }
    }

    fn respond(&mut self, delay: u64, interrupt: u8, response: Vec<u8>) {
        let due = self.pending.back().map_or(self.now, |last| last.due.max(self.now)) + delay;
        self.pending.push_back(Pending { due, interrupt, response, sector: None, finish: None });
    }

    fn respond_later(&mut self, interrupt: u8, finish: Activity) {
        let due = self.pending.back().map_or(self.now, |last| last.due) + SECOND_RESPONSE;
        self.pending.push_back(Pending { due, interrupt, response: Vec::new(), sector: None, finish: Some(finish) });
    }

    fn command(&mut self, command: u8) {
        let parameters: Vec<u8> = self.parameters.drain(..).collect();
        self.commands.push(CommandRecord { frame: self.frame, command, parameters: parameters.clone() });
        self.busy_until = self.now + FIRST_RESPONSE;
        // A new command replaces the replies of the one before, except a read in progress.
        self.pending.retain(|pending| pending.sector.is_some());
        let stat = self.stat();
        let parameter = |index: usize| parameters.get(index).copied().unwrap_or(0);
        match command {
            0x01 => self.respond(FIRST_RESPONSE, 3, vec![stat]),
            0x02 => {
                self.seek_target = disc::from_msf(
                    disc::from_bcd(parameter(0)),
                    disc::from_bcd(parameter(1)),
                    disc::from_bcd(parameter(2)),
                )
                .saturating_sub(disc::LEAD_IN);
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
            }
            0x03 => {
                self.activity = Activity::Playing;
                self.position = self.seek_target;
                self.respond(FIRST_RESPONSE, 3, vec![self.stat()]);
            }
            0x06 | 0x1B => {
                self.pending.clear();
                self.activity = Activity::Reading;
                self.position = self.seek_target;
                self.respond(FIRST_RESPONSE, 3, vec![self.stat()]);
                self.next_sector_due = self.now + FIRST_RESPONSE + self.sector_cycles();
            }
            0x07 => {
                self.motor = true;
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
                self.respond_later(2, Activity::Idle);
            }
            0x08 => {
                self.pending.clear();
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
                self.activity = Activity::Idle;
                self.motor = false;
                self.respond_later(2, Activity::Idle);
            }
            0x09 => {
                self.pending.clear();
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
                self.activity = Activity::Idle;
                self.respond_later(2, Activity::Idle);
            }
            0x0A => {
                self.pending.clear();
                self.mode = 0x20;
                self.activity = Activity::Idle;
                self.motor = true;
                self.respond(FIRST_RESPONSE, 3, vec![self.stat()]);
                self.respond_later(2, Activity::Idle);
            }
            0x0B | 0x0C => self.respond(FIRST_RESPONSE, 3, vec![stat]),
            0x0D => {
                self.filter = (parameter(0), parameter(1));
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
            }
            0x0E => {
                self.mode = parameter(0);
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
            }
            0x0F => self.respond(FIRST_RESPONSE, 3, vec![stat, self.mode, 0, self.filter.0, self.filter.1]),
            0x10 => {
                let position = self.position.saturating_sub(1);
                let header = self.disc.as_mut().and_then(|disc| disc.sector(position));
                let response = header.map_or(vec![0; 8], |sector| sector[12..20].to_vec());
                self.respond(FIRST_RESPONSE, 3, response);
            }
            0x11 => {
                let (track, start) = self.track_of(self.position);
                let (rm, rs, rf) = disc::to_msf(self.position.saturating_sub(start).saturating_sub(disc::LEAD_IN));
                let (am, as_, af) = disc::to_msf(self.position);
                let response = vec![
                    disc::bcd(track),
                    1,
                    disc::bcd(rm),
                    disc::bcd(rs),
                    disc::bcd(rf),
                    disc::bcd(am),
                    disc::bcd(as_),
                    disc::bcd(af),
                ];
                self.respond(FIRST_RESPONSE, 3, response);
            }
            0x13 => {
                let last = self.disc.as_ref().map_or(1, |disc| disc.tracks().len() as u8);
                self.respond(FIRST_RESPONSE, 3, vec![stat, 1, disc::bcd(last)]);
            }
            0x14 => {
                let track = disc::from_bcd(parameter(0));
                let start = self.disc.as_ref().map_or(0, |disc| {
                    if track == 0 {
                        disc.end()
                    } else {
                        disc.tracks().iter().find(|t| t.number == track).map_or(0, |t| t.start)
                    }
                });
                let (minute, second, _) = disc::to_msf(start);
                self.respond(FIRST_RESPONSE, 3, vec![stat, disc::bcd(minute), disc::bcd(second)]);
            }
            0x15 | 0x16 => {
                self.pending.clear();
                self.activity = Activity::Idle;
                self.position = self.seek_target;
                self.respond(FIRST_RESPONSE, 3, vec![stat | STAT_SEEKING]);
                self.respond_later(2, Activity::Idle);
            }
            0x19 if parameter(0) == 0x20 => self.respond(FIRST_RESPONSE, 3, vec![0x94, 0x09, 0x19, 0xC0]),
            0x1A => {
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
                let due = self.pending.back().map_or(self.now, |last| last.due) + SECOND_RESPONSE;
                self.pending.push_back(Pending {
                    due,
                    interrupt: 2,
                    response: vec![0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'A'],
                    sector: None,
                    finish: None,
                });
            }
            0x1E => {
                self.respond(FIRST_RESPONSE, 3, vec![stat]);
                self.respond_later(2, Activity::Idle);
            }
            _ => self.respond(FIRST_RESPONSE, 5, vec![stat | STAT_ERROR, 0x40]),
        }
    }

    fn track_of(&self, position: u32) -> (u8, u32) {
        self.disc
            .as_ref()
            .and_then(|disc| disc.tracks().into_iter().rev().find(|track| track.start <= position))
            .map_or((1, 0), |track| (track.number, track.start))
    }

    fn sector_cycles(&self) -> u64 {
        if self.mode & MODE_DOUBLE_SPEED != 0 { SECTOR_CYCLES / 2 } else { SECTOR_CYCLES }
    }

    /// Delivers the responses and sectors due by CPU cycle `now`.
    pub fn sync(&mut self, now: u64, irq: &mut Irq) {
        self.now = now;
        if self.activity == Activity::Reading && now >= self.next_sector_due && self.pending.len() < 2 {
            self.next_sector_due = now + self.sector_cycles();
            self.read_next_sector();
        }
        if self.flags != 0 {
            return;
        }
        let Some(front) = self.pending.front() else { return };
        if front.due > now {
            return;
        }
        let Some(mut pending) = self.pending.pop_front() else { return };
        if let Some(finish) = pending.finish {
            self.activity = finish;
            pending.response = vec![self.stat()];
        }
        if pending.sector.is_some() {
            self.sector = pending.sector.take();
            self.sectors_read += 1;
        }
        self.response = pending.response.into();
        self.flags = pending.interrupt;
        if self.flags & self.enable != 0 {
            irq.raise(Line::Cdrom);
        }
    }

    fn read_next_sector(&mut self) {
        let position = self.position;
        self.position += 1;
        let Some(sector) = self.disc.as_mut().and_then(|disc| disc.sector(position)) else {
            self.activity = Activity::Idle;
            self.pending.push_back(Pending {
                due: self.now,
                interrupt: 5,
                response: vec![self.stat() | STAT_ERROR, 0x04],
                sector: None,
                finish: None,
            });
            return;
        };
        let submode = sector[18];
        let audio = submode & 0x04 != 0 && submode & 0x20 != 0;
        if audio && self.mode & MODE_XA_ADPCM != 0 {
            // Real-time XA audio goes to the SPU, not to the CPU.
            self.xa_sectors += 1;
            return;
        }
        if audio && self.mode & MODE_XA_FILTER != 0 {
            return;
        }
        self.pending.push_back(Pending {
            due: self.now,
            interrupt: 1,
            response: vec![self.stat()],
            sector: Some(sector),
            finish: None,
        });
    }
}
