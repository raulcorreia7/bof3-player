// SPDX-License-Identifier: MIT
//! The video beam and the three root counters (psx-spx "Timers", "GPU Timings").

use crate::irq::{Irq, Line};

/// CPU clock, in Hz.
pub const CPU_HZ: u64 = 33_868_800;
/// NTSC video clock, in Hz.
const VIDEO_HZ: u64 = 53_693_175;
/// Video cycles per NTSC line.
const VIDEO_CYCLES_PER_LINE: u64 = 3413;
/// Lines per NTSC frame.
pub const LINES_PER_FRAME: u32 = 263;
/// The line on which the vertical blank begins.
const VBLANK_LINE: u32 = 240;
/// Video cycles per dot at 320 pixels across, the width the dot clock assumes.
const VIDEO_CYCLES_PER_DOT: u64 = 8;

#[derive(Clone, Copy, Debug, Default)]
struct Counter {
    value: u32,
    mode: u32,
    target: u32,
    fired: bool,
    /// Source cycles below one count (for the divided clocks).
    fraction: u64,
}

/// The beam position and the root counters.
#[derive(Clone, Debug)]
pub struct Timers {
    counters: [Counter; 3],
    /// Video cycles into the current line.
    video_fraction: u64,
    line: u32,
    last_sync: u64,
    /// Vertical blanks since power-on.
    pub frame: u64,
}

impl Default for Timers {
    fn default() -> Self {
        Self {
            counters: [Counter { mode: 1 << 10, ..Counter::default() }; 3],
            video_fraction: 0,
            line: 0,
            last_sync: 0,
            frame: 0,
        }
    }
}

/// What happened while the timers caught up.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Elapsed {
    /// Vertical blanks that began.
    pub vblanks: u32,
}

impl Timers {
    /// The current beam line, 0 to 262.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// Whether the beam is in the vertical blank.
    #[must_use]
    pub fn in_vblank(&self) -> bool {
        self.line >= VBLANK_LINE
    }

    /// Advances the beam and the counters to CPU cycle `now`.
    pub fn sync(&mut self, now: u64, irq: &mut Irq) -> Elapsed {
        let delta = now.saturating_sub(self.last_sync);
        self.last_sync = now;
        let mut elapsed = Elapsed::default();
        if delta == 0 {
            return elapsed;
        }
        // The beam: CPU cycles to video cycles, then lines.
        let video = delta * VIDEO_HZ / CPU_HZ;
        let total = self.video_fraction + video;
        let lines = total / VIDEO_CYCLES_PER_LINE;
        self.video_fraction = total % VIDEO_CYCLES_PER_LINE;
        for _ in 0..lines {
            self.line += 1;
            if self.line == VBLANK_LINE {
                elapsed.vblanks += 1;
                self.frame += 1;
                irq.raise(Line::VBlank);
                if self.counters[1].mode & 1 != 0 {
                    // Timer 1 sync modes 1 and 2 reset at the vertical blank.
                    if matches!((self.counters[1].mode >> 1) & 3, 1 | 2) {
                        self.counters[1].value = 0;
                    }
                }
            }
            if self.line >= LINES_PER_FRAME {
                self.line = 0;
            }
            if self.counters[1].mode & 0x100 != 0 {
                self.advance(1, 1, irq);
            }
        }
        // Timer 0: system clock or dot clock.
        let ticks0 =
            if self.counters[0].mode & 0x100 != 0 { self.divided(0, video, VIDEO_CYCLES_PER_DOT) } else { delta };
        self.advance(0, ticks0, irq);
        if self.counters[1].mode & 0x100 == 0 {
            self.advance(1, delta, irq);
        }
        // Timer 2: system clock, or system clock / 8; sync modes 0 and 3 stop it.
        let timer2 = self.counters[2].mode;
        let stopped = timer2 & 1 != 0 && matches!((timer2 >> 1) & 3, 0 | 3);
        if !stopped {
            let ticks2 = if timer2 & 0x200 != 0 { self.divided(2, delta, 8) } else { delta };
            self.advance(2, ticks2, irq);
        }
        elapsed
    }

    fn divided(&mut self, index: usize, cycles: u64, divider: u64) -> u64 {
        let total = self.counters[index].fraction + cycles;
        self.counters[index].fraction = total % divider;
        total / divider
    }

    fn advance(&mut self, index: usize, ticks: u64, irq: &mut Irq) {
        if ticks == 0 {
            return;
        }
        let counter = &mut self.counters[index];
        let reset_on_target = counter.mode & (1 << 3) != 0;
        let target = u64::from(counter.target);
        let before = u64::from(counter.value);
        let after = before + ticks;
        let hit_target = (before < target && after >= target) || (reset_on_target && after > target);
        let mut hit_overflow = false;
        let value = if reset_on_target && after > target {
            (after - target - 1) % (target + 1)
        } else if after > 0xFFFF {
            hit_overflow = true;
            after & 0xFFFF
        } else {
            after
        };
        if hit_target {
            counter.mode |= 1 << 11;
        }
        if hit_overflow {
            counter.mode |= 1 << 12;
        }
        let wants = (hit_target && counter.mode & (1 << 4) != 0) || (hit_overflow && counter.mode & (1 << 5) != 0);
        counter.value = value as u32;
        if wants && (counter.mode & (1 << 6) != 0 || !counter.fired) {
            counter.fired = true;
            if counter.mode & (1 << 7) != 0 {
                counter.mode ^= 1 << 10;
            }
            irq.raise(match index {
                0 => Line::Timer0,
                1 => Line::Timer1,
                _ => Line::Timer2,
            });
        }
    }

    /// Reads a counter register at `offset` from 0x1F801100.
    pub fn read(&mut self, offset: u32) -> u32 {
        let index = ((offset >> 4) & 3) as usize;
        if index == 3 {
            return 0;
        }
        let counter = &mut self.counters[index];
        match offset & 0xC {
            0x0 => counter.value,
            0x4 => {
                let mode = counter.mode;
                counter.mode &= !((1 << 11) | (1 << 12));
                mode
            }
            0x8 => counter.target,
            _ => 0,
        }
    }

    /// Writes a counter register at `offset` from 0x1F801100.
    pub fn write(&mut self, offset: u32, value: u32) {
        let index = ((offset >> 4) & 3) as usize;
        if index == 3 {
            return;
        }
        let counter = &mut self.counters[index];
        match offset & 0xC {
            0x0 => counter.value = value & 0xFFFF,
            0x4 => {
                counter.mode = (value & 0x3FF) | (1 << 10);
                counter.value = 0;
                counter.fired = false;
                counter.fraction = 0;
            }
            0x8 => counter.target = value & 0xFFFF,
            _ => {}
        }
    }
}
