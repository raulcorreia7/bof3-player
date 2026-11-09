// SPDX-License-Identifier: MIT
//! GTE commands and registers, driven directly with hand-worked values from psx-spx's formulas.

use psx_machine::gte::{Gte, flag};

/// Command words with `sf` set, as libgte issues them.
const RTPS: u32 = 0x0018_0001;
const RTPT: u32 = 0x0028_0030;
const NCLIP: u32 = 0x0140_0006;
const AVSZ3: u32 = 0x0158_002D;
const AVSZ4: u32 = 0x0168_002E;
const NCDS: u32 = 0x00E8_0413;
const SQR_SF0: u32 = 0x00A0_0428;
const SF: u32 = 1 << 19;
const LM: u32 = 1 << 10;

fn mvmva(matrix: u32, vector: u32, translation: u32) -> u32 {
    0x0040_0012 | (matrix << 17) | (vector << 15) | (translation << 13)
}

fn pair(low: i16, high: i16) -> u32 {
    u32::from(low as u16) | (u32::from(high as u16) << 16)
}

fn signed(value: u32) -> (i16, i16) {
    (value as i16, (value >> 16) as i16)
}

/// Identity rotation, no translation, the screen centre at (160, 120), H = 200.
fn camera() -> Gte {
    let mut gte = Gte::default();
    gte.write_control(0, 0x1000);
    gte.write_control(2, 0x1000);
    gte.write_control(4, 0x1000);
    gte.write_control(24, 160 << 16);
    gte.write_control(25, 120 << 16);
    gte.write_control(26, 200);
    gte
}

fn vertex(gte: &mut Gte, index: usize, [x, y, z]: [i16; 3]) {
    gte.write_data(2 * index, pair(x, y));
    gte.write_data(2 * index + 1, z as u16 as u32);
}

#[test]
fn rtps_projects_and_saturates_depth_cueing() {
    let mut gte = camera();
    gte.write_control(27, 0x10); // DQA
    gte.write_control(28, 0x0100_0000); // DQB
    vertex(&mut gte, 0, [100, -50, 200]);
    gte.command(RTPS).unwrap();
    assert_eq!([9, 10, 11].map(|r| gte.read_data(r) as i32), [100, -50, 200]);
    assert_eq!(gte.read_data(19), 200, "SZ3");
    // H / SZ3 = 1.0, so the vertex lands at its x and y plus the screen offset.
    assert_eq!(signed(gte.read_data(14)), (260, 70));
    // 10000h × 10h + 1000000h = 1100000h; IR0 = 1100h saturates to 1000h.
    assert_eq!(gte.read_data(24), 0x0110_0000);
    assert_eq!(gte.read_data(8), 0x1000);
    assert_eq!(gte.read_control(31), flag::IR0, "IR0 is not an error bit");
}

#[test]
fn rtpt_fills_the_fifos_and_flags_a_vertex_at_the_near_plane() {
    let mut gte = camera();
    gte.write_control(28, 0x0080_0000);
    vertex(&mut gte, 0, [100, -50, 200]);
    vertex(&mut gte, 1, [0, 0, 400]);
    vertex(&mut gte, 2, [-100, 100, 100]);
    gte.command(RTPT).unwrap();
    assert_eq!([16, 17, 18, 19].map(|r| gte.read_data(r)), [0, 200, 400, 100]);
    assert_eq!(signed(gte.read_data(12)), (260, 70));
    assert_eq!(signed(gte.read_data(13)), (160, 120));
    // SZ3 = H/2: the division saturates to 1FFFFh.
    assert_eq!(signed(gte.read_data(14)), (-40, 319));
    assert_eq!(gte.read_data(15), gte.read_data(14), "SXYP mirrors SXY2");
    assert_eq!(gte.read_data(8), 0x800, "depth cueing from the last vertex");
    assert_eq!(gte.read_control(31), 0x8000_0000 | flag::DIVIDE);
}

#[test]
fn division_follows_the_unr_approximation() {
    let mut gte = camera();
    gte.write_control(24, 0);
    gte.write_control(25, 0);
    gte.write_control(27, 1); // MAC0 = quotient × 1
    // FE3Fh / 7F20h comes out of the reciprocal as 20000h and clamps to 1FFFFh without a flag.
    gte.write_control(26, 0xFE3F);
    vertex(&mut gte, 0, [0, 0, 0x7F20]);
    gte.command(RTPS).unwrap();
    assert_eq!(gte.read_data(24), 0x1_FFFF);
    assert_eq!(gte.read_control(31) & flag::DIVIDE, 0);
    // 1 / 1 takes the largest seed and is exact.
    gte.write_control(26, 1);
    vertex(&mut gte, 0, [0, 0, 1]);
    gte.command(RTPS).unwrap();
    assert_eq!(gte.read_data(24), 0x1_0000);
    // A vertex behind the camera saturates SZ3 to 0 and overflows the division.
    vertex(&mut gte, 0, [0, 0, -5]);
    gte.command(RTPS).unwrap();
    assert_eq!(gte.read_data(19), 0);
    assert_eq!(gte.read_data(24), 0x1_FFFF);
    assert_eq!(gte.read_control(31), 0x8000_0000 | flag::Z | flag::DIVIDE);
}

#[test]
fn nclip_gives_the_winding() {
    let mut gte = Gte::default();
    for (register, xy) in [(12, pair(0, 0)), (13, pair(10, 0)), (14, pair(0, 10))] {
        gte.write_data(register, xy);
    }
    gte.command(NCLIP).unwrap();
    assert_eq!(gte.read_data(24) as i32, 100);
    gte.write_data(13, pair(0, 10));
    gte.write_data(14, pair(10, 0));
    gte.command(NCLIP).unwrap();
    assert_eq!(gte.read_data(24) as i32, -100);
}

#[test]
fn average_z_scales_the_fifo_into_otz() {
    let mut gte = Gte::default();
    for (register, z) in [(16, 100), (17, 100), (18, 200), (19, 300)] {
        gte.write_data(register, z);
    }
    gte.write_control(29, 0x155);
    gte.write_control(30, 0x100);
    gte.command(AVSZ3).unwrap();
    assert_eq!((gte.read_data(24), gte.read_data(7)), (0x155 * 600, 49));
    gte.command(AVSZ4).unwrap();
    assert_eq!((gte.read_data(24), gte.read_data(7)), (0x100 * 700, 43));
    gte.write_control(29, 0xFFFF);
    gte.command(AVSZ3).unwrap();
    assert_eq!(gte.read_data(7), 0);
    assert_eq!(gte.read_control(31), 0x8000_0000 | flag::Z);
}

#[test]
fn mvmva_multiplies_adds_and_saturates() {
    let mut gte = Gte::default();
    gte.write_control(0, 0x1000); // RT11, RT12
    gte.write_control(2, 0x2000); // RT22, RT23
    gte.write_control(4, 0xF000); // RT33 = -1.0
    for (register, value) in [(5, 1), (6, 2), (7, 3), (21, 5), (22, 5), (23, 5)] {
        gte.write_control(register, value);
    }
    vertex(&mut gte, 1, [10, 20, 30]);
    gte.command(mvmva(0, 1, 0) | SF).unwrap();
    assert_eq!([25, 26, 27].map(|r| gte.read_data(r) as i32), [11, 42, -27]);
    assert_eq!([9, 10, 11].map(|r| gte.read_data(r) as i32), [11, 42, -27]);
    assert_eq!(gte.read_control(31), 0);
    // lm clamps IR3 at 0 and flags it; bit 22 is outside the error summary.
    gte.command(mvmva(0, 1, 0) | SF | LM).unwrap();
    assert_eq!(gte.read_data(11), 0);
    assert_eq!(gte.read_control(31), flag::IR[2]);
    // The far-colour translation keeps only the last two columns.
    gte.command(mvmva(0, 1, 2) | SF).unwrap();
    assert_eq!([25, 26, 27].map(|r| gte.read_data(r) as i32), [0, 40, -30]);
    // A sum past 43 bits flags MAC1.
    gte.write_control(0, 0x7FFF);
    gte.write_control(5, 0x7FFF_FFFF);
    vertex(&mut gte, 0, [0x7FFF, 0, 0]);
    gte.command(mvmva(0, 0, 0) | SF).unwrap();
    assert_ne!(gte.read_control(31) & flag::MAC_POSITIVE[0], 0);
    assert_ne!(gte.read_control(31) & 0x8000_0000, 0);
}

#[test]
fn ncds_lights_tints_and_fades_to_the_far_colour() {
    let mut gte = Gte::default();
    for register in [8, 16] {
        // Light and light-colour matrices: identity.
        gte.write_control(register, 0x1000);
        gte.write_control(register + 2, 0x1000);
        gte.write_control(register + 4, 0x1000);
    }
    vertex(&mut gte, 0, [0x1000, 0, 0]);
    gte.write_data(6, 0x3080_8080);
    gte.command(NCDS).unwrap();
    // R × IR1 SHL 4 = 800000h, SAR 12 = 800h, / 16 = 80h.
    assert_eq!(gte.read_data(25), 0x800);
    assert_eq!(gte.read_data(22), 0x3000_0080);
    // Halfway to a black far colour.
    gte.write_data(8, 0x800);
    gte.command(NCDS).unwrap();
    assert_eq!(gte.read_data(22), 0x3000_0040);
    assert_eq!(gte.read_data(21), 0x3000_0080, "the FIFO moved down");
}

#[test]
fn sqr_saturates_ir_and_sets_the_error_bit() {
    let mut gte = Gte::default();
    gte.write_data(9, 0x7FFF);
    gte.command(SQR_SF0).unwrap();
    assert_eq!(gte.read_data(25), 0x3FFF_0001);
    assert_eq!(gte.read_data(9), 0x7FFF);
    assert_eq!(gte.read_control(31), 0x8000_0000 | flag::IR[0]);
}

#[test]
fn register_quirks() {
    let mut gte = Gte::default();
    // H reads back sign-extended.
    gte.write_control(26, 0x8000);
    assert_eq!(gte.read_control(26), 0xFFFF_8000);
    // IRGB expands to IR1–IR3; ORGB collapses them back, saturating.
    gte.write_data(28, 0x7FFF);
    assert_eq!([9, 10, 11].map(|r| gte.read_data(r)), [0xF80; 3]);
    gte.write_data(9, 0xFFFF_8000);
    assert_eq!(gte.read_data(29), 0x7FE0);
    // LZCR counts leading zeroes, or ones for a negative value.
    gte.write_data(30, 0x0000_FFFF);
    assert_eq!(gte.read_data(31), 16);
    gte.write_data(30, 0xFF00_0000);
    assert_eq!(gte.read_data(31), 8);
    // Writing SXYP pushes the screen FIFO.
    gte.write_data(14, pair(1, 2));
    gte.write_data(15, pair(3, 4));
    assert_eq!((gte.read_data(13), gte.read_data(14)), (pair(1, 2), pair(3, 4)));
    // FLAG keeps bits 12–30 and derives bit 31.
    gte.write_control(31, 0xFFFF_FFFF);
    assert_eq!(gte.read_control(31), 0xFFFF_F000);
    gte.write_control(31, flag::IR0);
    assert_eq!(gte.read_control(31), flag::IR0);
}
