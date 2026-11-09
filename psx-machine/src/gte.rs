// SPDX-License-Identifier: MIT
//! The geometry transformation engine (coprocessor 2): its registers and commands, written from
//! psx-spx "Geometry Transformation Engine (GTE)". Commands finish at once; the machine does not
//! model their cycle counts.

use crate::Halt;

/// The `FLAG` bits.
pub mod flag {
    /// `MAC1`, `MAC2`, `MAC3` larger than 43 bits and positive (bit 30 for `MAC1`, down to 28).
    pub const MAC_POSITIVE: [u32; 3] = [1 << 30, 1 << 29, 1 << 28];
    /// `MAC1`, `MAC2`, `MAC3` larger than 43 bits and negative.
    pub const MAC_NEGATIVE: [u32; 3] = [1 << 27, 1 << 26, 1 << 25];
    /// `IR1`, `IR2`, `IR3` saturated.
    pub const IR: [u32; 3] = [1 << 24, 1 << 23, 1 << 22];
    /// Colour FIFO red, green, blue saturated to 0..FFh.
    pub const COLOUR: [u32; 3] = [1 << 21, 1 << 20, 1 << 19];
    /// `SZ3` or `OTZ` saturated to 0..FFFFh.
    pub const Z: u32 = 1 << 18;
    /// The perspective division overflowed.
    pub const DIVIDE: u32 = 1 << 17;
    /// `MAC0` larger than 31 bits and positive.
    pub const MAC0_POSITIVE: u32 = 1 << 16;
    /// `MAC0` larger than 31 bits and negative.
    pub const MAC0_NEGATIVE: u32 = 1 << 15;
    /// `SX2` saturated to -400h..3FFh.
    pub const SX: u32 = 1 << 14;
    /// `SY2` saturated to -400h..3FFh.
    pub const SY: u32 = 1 << 13;
    /// `IR0` saturated to 0..1000h.
    pub const IR0: u32 = 1 << 12;
    /// Bit 31 is the OR of these.
    pub const ERROR: u32 = 0x7F87_E000;
    /// The bits software can write.
    pub const WRITABLE: u32 = 0x7FFF_F000;
}

/// The reciprocal seeds of the perspective division (psx-spx "GTE Division Inaccuracy").
const UNR_TABLE: [u8; 0x101] = {
    let mut table = [0u8; 0x101];
    let mut index = 0;
    while index < table.len() {
        let value = (0x40000 / (index as i32 + 0x100) + 1) / 2 - 0x101;
        table[index] = if value > 0 { value as u8 } else { 0 };
        index += 1;
    }
    table
};

/// A 3×3 matrix of 1.3.12 fixed-point elements, row by row.
type Matrix = [i16; 9];

/// The fields of a command word.
#[derive(Clone, Copy, Debug)]
struct Fields {
    /// `sf`: shift products right by 12.
    shift: u32,
    /// `lm`: saturate `IR1`–`IR3` to 0 instead of -8000h.
    lm: bool,
}

/// The GTE registers.
#[derive(Clone, Debug, Default)]
pub struct Gte {
    vectors: [[i16; 3]; 3],
    rgbc: [u8; 4],
    otz: u16,
    ir: [i16; 4],
    sxy: [[i16; 2]; 3],
    sz: [u16; 4],
    colours: [[u8; 4]; 3],
    res1: u32,
    mac: [i32; 4],
    lzcs: u32,
    rotation: Matrix,
    translation: [i32; 3],
    light: Matrix,
    background: [i32; 3],
    light_colour: Matrix,
    far_colour: [i32; 3],
    offset: [i32; 2],
    h: u16,
    dqa: i16,
    dqb: i32,
    zsf3: i16,
    zsf4: i16,
    flag: u32,
}

fn low(value: u32) -> i16 {
    value as i16
}

fn high(value: u32) -> i16 {
    (value >> 16) as i16
}

fn pair(low: i16, high: i16) -> u32 {
    u32::from(low as u16) | (u32::from(high as u16) << 16)
}

fn read_matrix(matrix: &Matrix, index: usize) -> u32 {
    if index == 4 { matrix[8] as i32 as u32 } else { pair(matrix[2 * index], matrix[2 * index + 1]) }
}

fn write_matrix(matrix: &mut Matrix, index: usize, value: u32) {
    if index == 4 {
        matrix[8] = low(value);
    } else {
        matrix[2 * index] = low(value);
        matrix[2 * index + 1] = high(value);
    }
}

impl Gte {
    /// Data register `index`, as `MFC2` and `SWC2` read it.
    #[must_use]
    pub fn read_data(&self, index: usize) -> u32 {
        match index {
            0 | 2 | 4 => pair(self.vectors[index / 2][0], self.vectors[index / 2][1]),
            1 | 3 | 5 => self.vectors[index / 2][2] as i32 as u32,
            6 => u32::from_le_bytes(self.rgbc),
            7 => u32::from(self.otz),
            8..=11 => self.ir[index - 8] as i32 as u32,
            12..=14 => pair(self.sxy[index - 12][0], self.sxy[index - 12][1]),
            15 => pair(self.sxy[2][0], self.sxy[2][1]),
            16..=19 => u32::from(self.sz[index - 16]),
            20..=22 => u32::from_le_bytes(self.colours[index - 20]),
            23 => self.res1,
            24..=27 => self.mac[index - 24] as u32,
            28 | 29 => {
                let five = |value: i16| (i32::from(value) >> 7).clamp(0, 0x1F) as u32;
                five(self.ir[1]) | (five(self.ir[2]) << 5) | (five(self.ir[3]) << 10)
            }
            30 => self.lzcs,
            _ => {
                let lzcs = self.lzcs;
                if (lzcs as i32) < 0 { lzcs.leading_ones() } else { lzcs.leading_zeros() }
            }
        }
    }

    /// Writes data register `index`, as `MTC2` and `LWC2` do.
    pub fn write_data(&mut self, index: usize, value: u32) {
        match index {
            0 | 2 | 4 => {
                self.vectors[index / 2][0] = low(value);
                self.vectors[index / 2][1] = high(value);
            }
            1 | 3 | 5 => self.vectors[index / 2][2] = low(value),
            6 => self.rgbc = value.to_le_bytes(),
            7 => self.otz = value as u16,
            8..=11 => self.ir[index - 8] = low(value),
            12..=14 => self.sxy[index - 12] = [low(value), high(value)],
            15 => self.push_sxy(low(value), high(value)),
            16..=19 => self.sz[index - 16] = value as u16,
            20..=22 => self.colours[index - 20] = value.to_le_bytes(),
            23 => self.res1 = value,
            24..=27 => self.mac[index - 24] = value as i32,
            28 => {
                for channel in 0..3 {
                    self.ir[channel + 1] = (((value >> (5 * channel)) & 0x1F) << 7) as i16;
                }
            }
            30 => self.lzcs = value,
            _ => {}
        }
    }

    /// Control register `index`, as `CFC2` reads it.
    #[must_use]
    pub fn read_control(&self, index: usize) -> u32 {
        match index {
            0..=4 => read_matrix(&self.rotation, index),
            5..=7 => self.translation[index - 5] as u32,
            8..=12 => read_matrix(&self.light, index - 8),
            13..=15 => self.background[index - 13] as u32,
            16..=20 => read_matrix(&self.light_colour, index - 16),
            21..=23 => self.far_colour[index - 21] as u32,
            24 | 25 => self.offset[index - 24] as u32,
            // Reads sign-extend H although it is unsigned.
            26 => self.h as i16 as i32 as u32,
            27 => self.dqa as i32 as u32,
            28 => self.dqb as u32,
            29 => self.zsf3 as i32 as u32,
            30 => self.zsf4 as i32 as u32,
            _ => self.flag | (u32::from(self.flag & flag::ERROR != 0) << 31),
        }
    }

    /// Writes control register `index`, as `CTC2` does.
    pub fn write_control(&mut self, index: usize, value: u32) {
        match index {
            0..=4 => write_matrix(&mut self.rotation, index, value),
            5..=7 => self.translation[index - 5] = value as i32,
            8..=12 => write_matrix(&mut self.light, index - 8, value),
            13..=15 => self.background[index - 13] = value as i32,
            16..=20 => write_matrix(&mut self.light_colour, index - 16, value),
            21..=23 => self.far_colour[index - 21] = value as i32,
            24 | 25 => self.offset[index - 24] = value as i32,
            26 => self.h = value as u16,
            27 => self.dqa = low(value),
            28 => self.dqb = value as i32,
            29 => self.zsf3 = low(value),
            30 => self.zsf4 = low(value),
            _ => self.flag = value & flag::WRITABLE,
        }
    }

    /// Runs the command in the low 25 bits of a `COP2` instruction.
    ///
    /// # Errors
    ///
    /// [`Halt::Gte`] for an opcode psx-spx lists as having no function.
    pub fn command(&mut self, command: u32) -> Result<(), Halt> {
        let fields = Fields { shift: if command & (1 << 19) != 0 { 12 } else { 0 }, lm: command & (1 << 10) != 0 };
        self.flag = 0;
        match command & 0x3F {
            0x01 => self.rtps(0, fields, true),
            0x06 => self.nclip(),
            0x0C => self.op(fields),
            0x10 => self.dpcs(fields, self.rgbc),
            0x11 => self.intpl(fields),
            0x12 => self.mvmva(command, fields),
            0x13 => self.ncds(0, fields),
            0x14 => self.cdp(fields),
            0x16 => (0..3).for_each(|vector| self.ncds(vector, fields)),
            0x1B => self.nccs(0, fields),
            0x1C => self.cc(fields),
            0x1E => self.ncs(0, fields),
            0x20 => (0..3).for_each(|vector| self.ncs(vector, fields)),
            0x28 => self.sqr(fields),
            0x29 => self.dcpl(fields),
            0x2A => (0..3).for_each(|_| self.dpcs(fields, self.colours[0])),
            0x2D => self.avsz(i64::from(self.zsf3), &[1, 2, 3]),
            0x2E => self.avsz(i64::from(self.zsf4), &[0, 1, 2, 3]),
            0x30 => (0..3).for_each(|vector| self.rtps(vector, fields, vector == 2)),
            0x3D => self.gpf(fields),
            0x3E => self.gpl(fields),
            0x3F => (0..3).for_each(|vector| self.nccs(vector, fields)),
            _ => return Err(Halt::Gte { command }),
        }
        Ok(())
    }

    /// Adds `terms` the way the 44-bit accumulator of `MAC1`–`MAC3` does: each partial sum sets
    /// the overflow flags of accumulator `index` (0–2) and wraps to 44 bits.
    fn accumulate(&mut self, index: usize, terms: &[i64]) -> i64 {
        let mut sum = 0i64;
        for term in terms {
            sum += term;
            if sum >= 1 << 43 {
                self.flag |= flag::MAC_POSITIVE[index];
            } else if sum < -(1 << 43) {
                self.flag |= flag::MAC_NEGATIVE[index];
            }
            sum = (sum << 20) >> 20;
        }
        sum
    }

    fn set_mac0(&mut self, value: i64) {
        if value > i64::from(i32::MAX) {
            self.flag |= flag::MAC0_POSITIVE;
        } else if value < i64::from(i32::MIN) {
            self.flag |= flag::MAC0_NEGATIVE;
        }
        self.mac[0] = value as i32;
    }

    /// Saturates `value` into `IR1`–`IR3` (`index` 0–2) and flags it when it did not fit.
    fn saturate_ir(&mut self, index: usize, value: i64, lm: bool) -> i16 {
        let min = if lm { 0 } else { -0x8000 };
        if value < min || value > 0x7FFF {
            self.flag |= flag::IR[index];
        }
        value.clamp(min, 0x7FFF) as i16
    }

    /// Stores the three accumulator `sums` shifted by `sf` into `MAC1`–`MAC3`, and from there,
    /// saturated, into `IR1`–`IR3`.
    fn store(&mut self, sums: [i64; 3], fields: Fields) {
        for (index, sum) in sums.into_iter().enumerate() {
            self.mac[index + 1] = (sum >> fields.shift) as i32;
            self.ir[index + 1] = self.saturate_ir(index, i64::from(self.mac[index + 1]), fields.lm);
        }
    }

    /// `matrix × vector + translation × 1000h`, into `MAC1`–`MAC3` and `IR1`–`IR3`.
    fn transform(&mut self, matrix: &Matrix, vector: [i16; 3], translation: [i32; 3], fields: Fields) {
        let mut sums = [0; 3];
        for (row, sum) in sums.iter_mut().enumerate() {
            let terms = [
                i64::from(translation[row]) << 12,
                i64::from(matrix[3 * row]) * i64::from(vector[0]),
                i64::from(matrix[3 * row + 1]) * i64::from(vector[1]),
                i64::from(matrix[3 * row + 2]) * i64::from(vector[2]),
            ];
            *sum = self.accumulate(row, &terms);
        }
        self.store(sums, fields);
    }

    fn ir_vector(&self) -> [i16; 3] {
        [self.ir[1], self.ir[2], self.ir[3]]
    }

    fn push_sxy(&mut self, x: i16, y: i16) {
        self.sxy[0] = self.sxy[1];
        self.sxy[1] = self.sxy[2];
        self.sxy[2] = [x, y];
    }

    fn push_sz(&mut self, value: i64) {
        if !(0..=0xFFFF).contains(&value) {
            self.flag |= flag::Z;
        }
        self.sz = [self.sz[1], self.sz[2], self.sz[3], value.clamp(0, 0xFFFF) as u16];
    }

    /// Pushes `MAC1`–`MAC3` / 16 and the code byte of `RGBC` onto the colour FIFO.
    fn push_colour(&mut self) {
        let mut colour = self.rgbc;
        for (channel, value) in colour.iter_mut().take(3).enumerate() {
            let level = self.mac[channel + 1] >> 4;
            if !(0..=0xFF).contains(&level) {
                self.flag |= flag::COLOUR[channel];
            }
            *value = level.clamp(0, 0xFF) as u8;
        }
        self.colours = [self.colours[1], self.colours[2], colour];
    }

    /// The perspective division `H / SZ3` as the hardware's reciprocal approximation computes
    /// it, in 16 fraction bits.
    fn divide(&mut self) -> i64 {
        let (h, sz3) = (u64::from(self.h), u64::from(self.sz[3]));
        if h >= sz3 * 2 {
            self.flag |= flag::DIVIDE;
            return 0x1FFFF;
        }
        let shift = (sz3 as u16).leading_zeros();
        let numerator = h << shift;
        let divisor = sz3 << shift;
        let seed = u64::from(UNR_TABLE[((divisor - 0x7FC0) >> 7) as usize]) + 0x101;
        let divisor = (0x200_0080 - divisor * seed) >> 8;
        let reciprocal = (0x80 + divisor * seed) >> 8;
        (((numerator * reciprocal) + 0x8000) >> 16).min(0x1FFFF) as i64
    }

    /// RTPS on vector `index`; depth cueing (`MAC0`, `IR0`) only when `last`.
    fn rtps(&mut self, index: usize, fields: Fields, last: bool) {
        let vector = self.vectors[index];
        let mut sums = [0i64; 3];
        for (row, sum) in sums.iter_mut().enumerate() {
            let terms = [
                i64::from(self.translation[row]) << 12,
                i64::from(self.rotation[3 * row]) * i64::from(vector[0]),
                i64::from(self.rotation[3 * row + 1]) * i64::from(vector[1]),
                i64::from(self.rotation[3 * row + 2]) * i64::from(vector[2]),
            ];
            *sum = self.accumulate(row, &terms);
            self.mac[row + 1] = (*sum >> fields.shift) as i32;
        }
        self.ir[1] = self.saturate_ir(0, i64::from(self.mac[1]), fields.lm);
        self.ir[2] = self.saturate_ir(1, i64::from(self.mac[2]), fields.lm);
        // The IR3 flag compares MAC3 SAR 12 with the lm=0 range whatever sf and lm say.
        let z = sums[2] >> 12;
        if !(-0x8000..=0x7FFF).contains(&z) {
            self.flag |= flag::IR[2];
        }
        let min = if fields.lm { 0 } else { -0x8000 };
        self.ir[3] = i64::from(self.mac[3]).clamp(min, 0x7FFF) as i16;
        self.push_sz(z);
        let scale = self.divide();
        let x = scale * i64::from(self.ir[1]) + i64::from(self.offset[0]);
        self.set_mac0(x);
        let y = scale * i64::from(self.ir[2]) + i64::from(self.offset[1]);
        self.set_mac0(y);
        let (sx, sy) = (x >> 16, y >> 16);
        if !(-0x400..=0x3FF).contains(&sx) {
            self.flag |= flag::SX;
        }
        if !(-0x400..=0x3FF).contains(&sy) {
            self.flag |= flag::SY;
        }
        self.push_sxy(sx.clamp(-0x400, 0x3FF) as i16, sy.clamp(-0x400, 0x3FF) as i16);
        if last {
            let depth = scale * i64::from(self.dqa) + i64::from(self.dqb);
            self.set_mac0(depth);
            let ir0 = depth >> 12;
            if !(0..=0x1000).contains(&ir0) {
                self.flag |= flag::IR0;
            }
            self.ir[0] = ir0.clamp(0, 0x1000) as i16;
        }
    }

    fn nclip(&mut self) {
        let [[x0, y0], [x1, y1], [x2, y2]] = self.sxy.map(|[x, y]| [i64::from(x), i64::from(y)]);
        self.set_mac0(x0 * y1 + x1 * y2 + x2 * y0 - x0 * y2 - x1 * y0 - x2 * y1);
    }

    fn avsz(&mut self, factor: i64, stages: &[usize]) {
        let sum: i64 = stages.iter().map(|&stage| i64::from(self.sz[stage])).sum();
        let value = factor * sum;
        self.set_mac0(value);
        let otz = value >> 12;
        if !(0..=0xFFFF).contains(&otz) {
            self.flag |= flag::Z;
        }
        self.otz = otz.clamp(0, 0xFFFF) as u16;
    }

    fn op(&mut self, fields: Fields) {
        let [i1, i2, i3] = self.ir_vector().map(i64::from);
        let [d1, d2, d3] = [self.rotation[0], self.rotation[4], self.rotation[8]].map(i64::from);
        let sums = [
            self.accumulate(0, &[i3 * d2, -(i2 * d3)]),
            self.accumulate(1, &[i1 * d3, -(i3 * d1)]),
            self.accumulate(2, &[i2 * d1, -(i1 * d2)]),
        ];
        self.store(sums, fields);
    }

    fn sqr(&mut self, fields: Fields) {
        let ir = self.ir_vector().map(i64::from);
        let sums = [0, 1, 2].map(|index| self.accumulate(index, &[ir[index] * ir[index]]));
        self.store(sums, fields);
    }

    fn mvmva(&mut self, command: u32, fields: Fields) {
        let rgbc_red = i16::from(self.rgbc[0]) << 4;
        let matrix = match (command >> 17) & 3 {
            0 => self.rotation,
            1 => self.light,
            2 => self.light_colour,
            _ => {
                let (rt13, rt22) = (self.rotation[2], self.rotation[4]);
                [-rgbc_red, rgbc_red, self.ir[0], rt13, rt13, rt13, rt22, rt22, rt22]
            }
        };
        let vector = match (command >> 15) & 3 {
            3 => self.ir_vector(),
            index => self.vectors[index as usize],
        };
        match (command >> 13) & 3 {
            0 => self.transform(&matrix, vector, self.translation, fields),
            1 => self.transform(&matrix, vector, self.background, fields),
            2 => {
                // The far colour is not added: the first column's sum only sets flags.
                let mut sums = [0; 3];
                for (row, sum) in sums.iter_mut().enumerate() {
                    let first =
                        [i64::from(self.far_colour[row]) << 12, i64::from(matrix[3 * row]) * i64::from(vector[0])];
                    let discarded = self.accumulate(row, &first);
                    self.saturate_ir(row, discarded >> fields.shift, false);
                    let rest = [
                        i64::from(matrix[3 * row + 1]) * i64::from(vector[1]),
                        i64::from(matrix[3 * row + 2]) * i64::from(vector[2]),
                    ];
                    *sum = self.accumulate(row, &rest);
                }
                self.store(sums, fields);
            }
            _ => self.transform(&matrix, vector, [0; 3], fields),
        }
    }

    /// `LLM × V` then `BK × 1000h + LCM × IR`: the light reaching vector `index`.
    fn lighting(&mut self, index: usize, fields: Fields) {
        self.transform(&self.light.clone(), self.vectors[index], [0; 3], fields);
        self.colour_matrix(fields);
    }

    fn colour_matrix(&mut self, fields: Fields) {
        self.transform(&self.light_colour.clone(), self.ir_vector(), self.background, fields);
    }

    /// `[R × IR1, G × IR2, B × IR3] SHL 4` from `RGBC`.
    fn tint(&mut self) -> [i64; 3] {
        let ir = self.ir_vector();
        [0, 1, 2].map(|index| self.accumulate(index, &[(i64::from(self.rgbc[index]) * i64::from(ir[index])) << 4]))
    }

    /// `MAC + (FC − MAC) × IR0`, before the `sf` shift.
    fn depth_cue(&mut self, sums: [i64; 3], fields: Fields) -> [i64; 3] {
        let mut out = [0; 3];
        for index in 0..3 {
            let towards = self.accumulate(index, &[i64::from(self.far_colour[index]) << 12, -sums[index]]);
            // IR takes the difference through the 32-bit MAC register.
            let step = self.saturate_ir(index, i64::from((towards >> fields.shift) as i32), false);
            self.ir[index + 1] = step;
            out[index] = self.accumulate(index, &[i64::from(step) * i64::from(self.ir[0]), sums[index]]);
        }
        out
    }

    fn finish(&mut self, sums: [i64; 3], fields: Fields) {
        self.store(sums, fields);
        self.push_colour();
    }

    fn ncs(&mut self, index: usize, fields: Fields) {
        self.lighting(index, fields);
        self.push_colour();
    }

    fn nccs(&mut self, index: usize, fields: Fields) {
        self.lighting(index, fields);
        let sums = self.tint();
        self.finish(sums, fields);
    }

    fn ncds(&mut self, index: usize, fields: Fields) {
        self.lighting(index, fields);
        let sums = self.tint();
        let sums = self.depth_cue(sums, fields);
        self.finish(sums, fields);
    }

    fn cc(&mut self, fields: Fields) {
        self.colour_matrix(fields);
        let sums = self.tint();
        self.finish(sums, fields);
    }

    fn cdp(&mut self, fields: Fields) {
        self.colour_matrix(fields);
        let sums = self.tint();
        let sums = self.depth_cue(sums, fields);
        self.finish(sums, fields);
    }

    fn dcpl(&mut self, fields: Fields) {
        let sums = self.tint();
        let sums = self.depth_cue(sums, fields);
        self.finish(sums, fields);
    }

    /// DPCS with the colour in `colour`; DPCT passes the bottom of the colour FIFO.
    fn dpcs(&mut self, fields: Fields, colour: [u8; 4]) {
        let sums = [0, 1, 2].map(|index| i64::from(colour[index]) << 16);
        let sums = self.depth_cue(sums, fields);
        self.finish(sums, fields);
    }

    fn intpl(&mut self, fields: Fields) {
        let sums = self.ir_vector().map(|value| i64::from(value) << 12);
        let sums = self.depth_cue(sums, fields);
        self.finish(sums, fields);
    }

    fn gpf(&mut self, fields: Fields) {
        let (ir, ir0) = (self.ir_vector(), i64::from(self.ir[0]));
        let sums = [0, 1, 2].map(|index| self.accumulate(index, &[i64::from(ir[index]) * ir0]));
        self.finish(sums, fields);
    }

    fn gpl(&mut self, fields: Fields) {
        let (ir, ir0, mac) = (self.ir_vector(), i64::from(self.ir[0]), self.mac);
        let sums = [0, 1, 2].map(|index| {
            self.accumulate(index, &[i64::from(mac[index + 1]) << fields.shift, i64::from(ir[index]) * ir0])
        });
        self.finish(sums, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::UNR_TABLE;

    #[test]
    fn unr_table_matches_the_published_rows() {
        assert_eq!(UNR_TABLE[..8], [0xFF, 0xFD, 0xFB, 0xF9, 0xF7, 0xF5, 0xF3, 0xF1]);
        assert_eq!(UNR_TABLE[0x76..0x79], [0x5D, 0x5D, 0x5C]);
        assert_eq!(UNR_TABLE[0xF8..], [0x03, 0x03, 0x02, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00]);
    }
}
