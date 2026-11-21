// SPDX-License-Identifier: MIT
//! GTE commands and registers, driven directly with hand-worked values from psx-spx's formulas.

use psx_machine::gte::{Gte, flag};

/// Command words with `sf` set, as libgte issues them.
const RTPS: u32 = 0x0018_0001;
const NCLIP: u32 = 0x0140_0006;
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
