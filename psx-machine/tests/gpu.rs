// SPDX-License-Identifier: MIT
//! GPU and DMA on synthetic command lists: what the log records and what lands in VRAM.

mod common;

use psx_machine::Machine;
use psx_machine::gpu::{Clut, Command, Depth, Draw, Shape, TexturePage, VramRect};

fn draws(machine: &Machine) -> Vec<Draw> {
    machine
        .bus()
        .gpu
        .log()
        .iter()
        .filter_map(|entry| match &entry.command {
            Command::Draw(draw) => Some(draw.clone()),
            _ => None,
        })
        .collect()
}

fn io(machine: &mut Machine, address: u32, value: u32) {
    machine.bus_mut().write32(address, value);
}

#[test]
fn image_upload_lands_at_its_rect() {
    let mut machine = Machine::new();
    machine.write_words(0x8003_0000, &[0x0002_0001, 0x0004_0003, 0x0006_0005]);
    io(&mut machine, 0x1F80_10F0, 0x0000_0800);
    io(&mut machine, 0x1F80_1810, 0xA000_0000);
    io(&mut machine, 0x1F80_1810, 0x0064_00C8); // x 200, y 100
    io(&mut machine, 0x1F80_1810, 0x0002_0003); // 3×2
    io(&mut machine, 0x1F80_10A0, 0x0003_0000);
    io(&mut machine, 0x1F80_10A4, 0x0001_0003);
    io(&mut machine, 0x1F80_10A8, 0x0100_0201); // from RAM, request blocks, start

    let gpu = &machine.bus().gpu;
    let rows: Vec<Vec<u16>> = (100..102).map(|y| (199..204).map(|x| gpu.pixel(x, y)).collect()).collect();
    assert_eq!(rows, vec![vec![0, 1, 2, 3, 0], vec![0, 4, 5, 6, 0]]);
    let rect = VramRect { x: 200, y: 100, width: 3, height: 2 };
    assert_eq!(gpu.log()[0].command, Command::Upload { rect, pixels: vec![1, 2, 3, 4, 5, 6] });
    assert_eq!(machine.bus_mut().read32(0x1F80_10A0), 0x0003_000C, "MADR advanced past the blocks");
}

#[test]
fn textured_gouraud_quad_records_page_clut_and_offset() {
    let mut machine = Machine::new();
    let offset = (100u32 & 0x7FF) | ((-50i32 as u32 & 0x7FF) << 11);
    let words = [
        0xE500_0000 | offset,
        0x3C11_2233,
        0x0000_0000,
        0x7C40_0000, // clut x 0, y 497; u 0, v 0
        0x0044_5566,
        0x0000_0020,
        0x0105_001F, // page: x 320, y 0, 15-bit; u 31
        0x0077_8899,
        0x0020_0000,
        0x0000_1F00,
        0x00AA_BBCC,
        0x0020_0020,
        0x0000_1F1F,
    ];
    for word in words {
        io(&mut machine, 0x1F80_1810, word);
    }
    let draws = draws(&machine);
    assert_eq!(draws.len(), 1);
    let draw = &draws[0];
    assert_eq!(draw.shape, Shape::Quad);
    assert!(draw.gouraud);
    assert_eq!(draw.offset, (100, -50));
    let colors: Vec<u32> = draw.vertices.iter().map(|vertex| vertex.color).collect();
    assert_eq!(colors, vec![0x11_2233, 0x44_5566, 0x77_8899, 0xAA_BBCC]);
    let uvs: Vec<(u8, u8)> = draw.vertices.iter().map(|vertex| (vertex.u, vertex.v)).collect();
    assert_eq!(uvs, vec![(0, 0), (31, 0), (0, 31), (31, 31)]);
    let texture = draw.texture.unwrap();
    assert_eq!(texture.clut, Clut { x: 0, y: 497 });
    assert_eq!(texture.page, TexturePage { x: 320, y: 0, depth: Depth::Bits15, blend: 0 });
    assert_eq!(machine.bus().gpu.status() & 0x1FF, 0x105, "the page attribute sets GPUSTAT");
}

#[test]
fn fill_and_copy_change_vram() {
    let mut machine = Machine::new();
    for word in [0x0200_00FF, 0x0002_0013, 0x0001_0005] {
        io(&mut machine, 0x1F80_1810, word);
    }
    let gpu = &machine.bus().gpu;
    assert_eq!((gpu.pixel(15, 2), gpu.pixel(16, 2), gpu.pixel(31, 2), gpu.pixel(32, 2)), (0, 0x1F, 0x1F, 0));
    assert_eq!(gpu.pixel(16, 3), 0);
    for word in [0x8000_0000, 0x0002_0010, 0x0040_0040, 0x0001_0002] {
        io(&mut machine, 0x1F80_1810, word);
    }
    let gpu = &machine.bus().gpu;
    assert_eq!((gpu.pixel(64, 64), gpu.pixel(65, 64), gpu.pixel(66, 64)), (0x1F, 0x1F, 0));
    assert!(matches!(gpu.log()[1].command, Command::Copy { from_x: 16, from_y: 2, .. }));
}

#[test]
fn dma_completion_raises_the_interrupt() {
    let mut machine = Machine::new();
    machine.write_words(0x8003_0000, &[0x00FF_FFFF]);
    io(&mut machine, 0x1F80_10F0, 0x0000_0800);
    io(&mut machine, 0x1F80_10F4, (1 << 23) | (1 << 18));
    io(&mut machine, 0x1F80_10A0, 0x0003_0000);
    io(&mut machine, 0x1F80_10A8, 0x0100_0401);
    let dicr = machine.bus_mut().read32(0x1F80_10F4);
    assert_eq!(dicr >> 24, 0x84, "master flag and the channel 2 flag");
    assert_eq!(machine.bus().irq.status(), 1 << 3);
    io(&mut machine, 0x1F80_10F4, (1 << 23) | (1 << 18) | (1 << 26));
    assert_eq!(machine.bus_mut().read32(0x1F80_10F4) >> 24, 0, "writing the flag acknowledges it");
}
