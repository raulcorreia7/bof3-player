// SPDX-License-Identifier: MIT
//! GPU and DMA on synthetic command lists: what the log records and what lands in VRAM.

mod common;

use common::*;
use psx_machine::Machine;
use psx_machine::gpu::{Clut, Command, Depth, Draw, Shape, TexturePage, VramRect};

const AT: u32 = 1;
const T0: u32 = 8;

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
fn linked_list_dma_records_a_textured_rect() {
    let mut program = Vec::new();
    program.extend(li(AT, 0x1F80_0000));
    program.extend(li(T0, 0x0000_0800));
    program.push(sw(T0, 0x10F0, AT)); // DPCR: enable channel 2
    program.extend(li(T0, 0x0003_0000));
    program.push(sw(T0, 0x10A0, AT)); // MADR: the first node
    program.push(sw(0, 0x10A4, AT));
    program.extend(li(T0, 0x0100_0401));
    program.push(sw(T0, 0x10A8, AT)); // CHCR: from RAM, linked list, start
    let (mut machine, end) = machine_with(&program);
    machine.write_words(0x8003_0000, &[0x0003_0100]);
    machine.write_words(0x8003_0100, &[0x05FF_FFFF, 0xE100_00DA, 0x6480_8080, 0x0014_000A, 0x7802_2010, 0x0018_0028]);
    machine.run_until_pc(end, 1000).unwrap();

    let draws = draws(&machine);
    assert_eq!(draws.len(), 1);
    let draw = &draws[0];
    assert_eq!(draw.shape, Shape::Rect { width: 40, height: 24 });
    let texture = draw.texture.expect("textured");
    assert_eq!(texture.page, TexturePage { x: 640, y: 256, depth: Depth::Bits8, blend: 2 });
    assert_eq!(texture.clut, Clut { x: 32, y: 480 });
    assert_eq!((draw.vertices[0].x, draw.vertices[0].y), (10, 20));
    assert_eq!((draw.vertices[0].u, draw.vertices[0].v), (16, 32));
    assert_eq!(machine.bus_mut().read32(0x1F80_10A8) & (1 << 24), 0, "the channel is idle again");
    assert_eq!(machine.bus().dma.transfers[2], 1);
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
fn image_download_reads_vram_back() {
    let mut machine = Machine::new();
    for word in [0xA000_0000, 0x0000_0010, 0x0001_0004, 0x2222_1111, 0x4444_3333] {
        io(&mut machine, 0x1F80_1810, word);
    }
    for word in [0xC000_0000, 0x0000_0010, 0x0001_0004] {
        io(&mut machine, 0x1F80_1810, word);
    }
    assert_ne!(machine.bus().gpu.status() & (1 << 27), 0, "data waits in GPUREAD");
    io(&mut machine, 0x1F80_10F0, 0x0000_0800);
    io(&mut machine, 0x1F80_10A0, 0x0003_0000);
    io(&mut machine, 0x1F80_10A4, 0x0001_0002);
    io(&mut machine, 0x1F80_10A8, 0x0100_0200); // to RAM, request blocks, start
    assert_eq!(machine.bus().peek32(0x8003_0000), Some(0x2222_1111));
    assert_eq!(machine.bus().peek32(0x8003_0004), Some(0x4444_3333));
    assert_eq!(machine.bus().gpu.status() & (1 << 27), 0);
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
fn mask_bit_settings_apply_to_uploads() {
    let mut machine = Machine::new();
    for word in [0xE600_0001, 0xA000_0000, 0, 0x0001_0002, 0x0002_0001] {
        io(&mut machine, 0x1F80_1810, word);
    }
    for word in [0xE600_0002, 0xA000_0000, 0, 0x0001_0002, 0x0004_0003] {
        io(&mut machine, 0x1F80_1810, word);
    }
    let gpu = &machine.bus().gpu;
    assert_eq!((gpu.pixel(0, 0), gpu.pixel(1, 0)), (0x8001, 0x8002), "set mask, then check mask keeps them");
}

#[test]
fn ordering_table_clear_links_backwards() {
    let mut machine = Machine::new();
    io(&mut machine, 0x1F80_10F0, 0x0800_0000);
    io(&mut machine, 0x1F80_10E0, 0x0004_000C);
    io(&mut machine, 0x1F80_10E4, 4);
    io(&mut machine, 0x1F80_10E8, 0x1100_0002);
    let words: Vec<u32> = (0..4).map(|index| machine.bus().peek32(0x8004_0000 + 4 * index).unwrap()).collect();
    assert_eq!(words, vec![0x00FF_FFFF, 0x0004_0000, 0x0004_0004, 0x0004_0008]);
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

#[test]
fn disabled_channel_waits_for_dpcr() {
    let mut machine = Machine::new();
    machine.write_words(0x8003_0000, &[0x00FF_FFFF]);
    io(&mut machine, 0x1F80_10F0, 0);
    io(&mut machine, 0x1F80_10A0, 0x0003_0000);
    io(&mut machine, 0x1F80_10A8, 0x0100_0401);
    assert_eq!(machine.bus().dma.transfers[2], 0);
    io(&mut machine, 0x1F80_10F0, 0x0000_0800);
    assert_eq!(machine.bus().dma.transfers[2], 1);
}

#[test]
fn polyline_ends_at_the_terminator() {
    let mut machine = Machine::new();
    for word in [0x4800_00FF, 0x0000_0000, 0x0010_0010, 0x0020_0000, 0x5555_5555, 0xE100_0000] {
        io(&mut machine, 0x1F80_1810, word);
    }
    let draws = draws(&machine);
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].shape, Shape::Line);
    assert_eq!(draws[0].vertices.len(), 3);
    assert_eq!(draws[0].vertices[2].y, 32);
}

#[test]
fn display_commands_and_queries() {
    let mut machine = Machine::new();
    io(&mut machine, 0x1F80_1814, 0x0300_0000);
    io(&mut machine, 0x1F80_1814, 0x0500_0000 | (240 << 10) | 64);
    io(&mut machine, 0x1F80_1814, 0x0800_0001);
    io(&mut machine, 0x1F80_1814, 0x1000_0007);
    assert_eq!(machine.bus_mut().read32(0x1F80_1810), 2, "GPU version");
    let status = machine.bus().gpu.status();
    assert_eq!(status & (1 << 23), 0, "display enabled");
    assert_eq!((status >> 17) & 3, 1, "horizontal resolution 320");
    assert_eq!(machine.bus().gpu.display_start, (64, 240));
    let display: Vec<u8> = machine
        .bus()
        .gpu
        .log()
        .iter()
        .filter_map(|entry| match entry.command {
            Command::Display { command, .. } => Some(command),
            _ => None,
        })
        .collect();
    assert_eq!(display, vec![3, 5, 8]);
}
