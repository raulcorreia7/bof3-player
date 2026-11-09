// SPDX-License-Identifier: MIT
//! Timers and the beam, the CD-ROM controller and the SPU, driven through their ports.

mod common;

use psx_machine::Machine;
use psx_machine::disc::MemoryDisc;
use psx_machine::exe::SystemConfig;
use psx_machine::irq::Line;
use psx_machine::timers::CPU_HZ;

const CDROM: u32 = 0x1F80_1800;

fn idle(machine: &mut Machine, instructions: u64) {
    // A tight loop in RAM keeps the CPU busy while the devices run.
    machine.write_words(0x8001_0000, &[common::j(0x8001_0000), common::nop()]);
    machine.cpu_mut().set_cop0(12, 0);
    machine.cpu_mut().set_pc(0x8001_0000);
    machine.run_until(|_| false, instructions).unwrap();
}

#[test]
fn frames_follow_the_ntsc_rate() {
    let mut machine = Machine::new();
    idle(&mut machine, 0);
    machine.run_frames(60, 100_000_000).unwrap();
    let seconds = machine.bus().cycles as f64 / CPU_HZ as f64;
    assert!((seconds - 60.0 / 59.94).abs() < 0.02, "60 frames took {seconds} s");
    assert_ne!(machine.bus().irq.status() & Line::VBlank as u16, 0);
}

#[test]
fn timer_two_interrupts_on_its_target() {
    let mut machine = Machine::new();
    idle(&mut machine, 0);
    let bus = machine.bus_mut();
    bus.write32(0x1F80_1128, 1000); // target
    bus.write32(0x1F80_1124, (1 << 3) | (1 << 4) | (1 << 6)); // reset at target, IRQ, repeat
    idle(&mut machine, 2_000);
    assert_ne!(machine.bus().irq.status() & Line::Timer2 as u16, 0);
    let value = machine.bus_mut().read32(0x1F80_1120);
    assert!(value <= 1000, "the counter wrapped at its target: {value}");
    let mode = machine.bus_mut().read32(0x1F80_1124);
    assert_ne!(mode & (1 << 11), 0, "reached-target flag");
    assert_eq!(machine.bus_mut().read32(0x1F80_1124) & (1 << 11), 0, "reading clears it");
}

#[test]
fn timer_one_counts_lines() {
    let mut machine = Machine::new();
    idle(&mut machine, 0);
    machine.bus_mut().write32(0x1F80_1114, 0x100); // horizontal blank clock
    machine.run_frames(1, 10_000_000).unwrap();
    let start = machine.bus_mut().read32(0x1F80_1110);
    machine.run_frames(1, 10_000_000).unwrap();
    let lines = machine.bus_mut().read32(0x1F80_1110).wrapping_sub(start) & 0xFFFF;
    assert_eq!(lines, 263);
}

fn cd_command(machine: &mut Machine, command: u8, parameters: &[u8]) {
    let bus = machine.bus_mut();
    bus.write8(CDROM, 0);
    for &parameter in parameters {
        bus.write8(CDROM + 2, parameter);
    }
    bus.write8(CDROM + 1, command);
}

/// Waits for the controller's next interrupt; returns its type and response.
fn cd_wait(machine: &mut Machine) -> (u8, Vec<u8>) {
    machine.run_until(|m| m.bus().irq.status() & Line::Cdrom as u16 != 0, 50_000_000).unwrap();
    let bus = machine.bus_mut();
    bus.write8(CDROM, 1);
    let interrupt = bus.read8(CDROM + 3) & 7;
    let mut response = Vec::new();
    while bus.read8(CDROM) & 0x20 != 0 {
        response.push(bus.read8(CDROM + 1));
    }
    bus.write8(CDROM + 3, 0x1F);
    bus.irq.acknowledge(!(Line::Cdrom as u16));
    (interrupt, response)
}

#[test]
fn cd_get_id_answers_twice() {
    let mut machine = Machine::new();
    machine.insert_disc(Box::new(MemoryDisc::from_data(&[0; 4096])));
    idle(&mut machine, 0);
    cd_command(&mut machine, 0x01, &[]);
    assert_eq!(cd_wait(&mut machine), (3, vec![0x02]));
    cd_command(&mut machine, 0x1A, &[]);
    assert_eq!(cd_wait(&mut machine).0, 3);
    let (interrupt, response) = cd_wait(&mut machine);
    assert_eq!(interrupt, 2);
    assert_eq!(&response[4..], b"SCEA");
}

#[test]
fn cd_read_delivers_sectors_through_dma() {
    let data: Vec<u8> = (0..3 * 2048).map(|index| (index * 7 % 251) as u8).collect();
    let mut machine = Machine::new();
    machine.insert_disc(Box::new(MemoryDisc::from_data(&data)));
    idle(&mut machine, 0);
    cd_command(&mut machine, 0x0E, &[0x80]); // double speed, 2048-byte sectors
    assert_eq!(cd_wait(&mut machine).0, 3);
    cd_command(&mut machine, 0x02, &[0x00, 0x02, 0x01]); // 00:02:01 = sector 1
    assert_eq!(cd_wait(&mut machine).0, 3);
    cd_command(&mut machine, 0x06, &[]);
    assert_eq!(cd_wait(&mut machine), (3, vec![0x22]), "motor on, reading");
    assert_eq!(cd_wait(&mut machine), (1, vec![0x22]));
    // Request the sector, then pull it with DMA channel 3.
    let bus = machine.bus_mut();
    bus.write8(CDROM, 0);
    bus.write8(CDROM + 3, 0x80);
    bus.write32(0x1F80_10F0, 0x8000);
    bus.write32(0x1F80_10B0, 0x0003_0000);
    bus.write32(0x1F80_10B4, 0x0001_0200);
    bus.write32(0x1F80_10B8, 0x1100_0000);
    let at = 0x3_0000;
    assert_eq!(&machine.ram()[at..at + 2048], &data[2048..4096]);
    assert_eq!(cd_wait(&mut machine), (1, vec![0x22]), "the next sector follows");
    cd_command(&mut machine, 0x09, &[]);
    let (_, first) = cd_wait(&mut machine);
    let (interrupt, second) = loop {
        let reply = cd_wait(&mut machine);
        if reply.0 != 1 {
            break reply;
        }
    };
    assert_eq!(first, vec![0x22]);
    assert_eq!((interrupt, second), (2, vec![0x02]), "paused: no longer reading");
}

#[test]
fn spu_key_on_records_the_voice() {
    let mut machine = Machine::new();
    let bus = machine.bus_mut();
    let voice = 0x1F80_1C00 + 3 * 0x10;
    bus.write16(voice, 0x3FFF);
    bus.write16(voice + 2, 0x2000);
    bus.write16(voice + 4, 0x1000);
    bus.write16(voice + 6, 0x0200);
    bus.write16(voice + 8, 0x80FF);
    bus.write16(voice + 0xA, 0x1FC5);
    bus.write16(voice + 0xE, 0x0210);
    bus.write16(0x1F80_1D88, 1 << 3);
    let key_ons = machine.key_ons();
    assert_eq!(key_ons.len(), 1);
    let key_on = key_ons[0];
    assert_eq!((key_on.voice, key_on.start, key_on.repeat, key_on.pitch), (3, 0x1000, 0x1080, 0x1000));
    assert_eq!((key_on.adsr1, key_on.adsr2, key_on.volume_left, key_on.volume_right), (0x80FF, 0x1FC5, 0x3FFF, 0x2000));
}

#[test]
fn spu_dma_fills_sound_ram_at_the_transfer_address() {
    let mut machine = Machine::new();
    machine.write_words(0x8003_0000, &[0x4433_2211, 0x8877_6655]);
    let bus = machine.bus_mut();
    bus.write16(0x1F80_1DA6, 0x0100); // byte 0x800
    bus.write16(0x1F80_1DAA, 0x0020); // DMA write
    assert_eq!(bus.read16(0x1F80_1DAE) & 0x3F, 0x20, "SPUSTAT follows SPUCNT");
    bus.write32(0x1F80_10F0, 0x8_0000);
    bus.write32(0x1F80_10C0, 0x0003_0000);
    bus.write32(0x1F80_10C4, 0x0001_0002);
    bus.write32(0x1F80_10C8, 0x0100_0201);
    let spu = &machine.bus().spu;
    assert_eq!(&spu.ram()[0x800..0x808], &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
    assert_eq!(spu.uploads().len(), 1);
    assert_eq!((spu.uploads()[0].address, spu.uploads()[0].bytes), (0x800, 8));
}

#[test]
fn system_config_names_the_boot_file_and_stack() {
    let config = SystemConfig::parse("BOOT = cdrom:\\SLUS_000.01;1\r\nTCB = 4\r\nEVENT = 16\r\nSTACK = 0X801FFF00\r\n");
    assert_eq!(config.boot, "SLUS_000.01");
    assert_eq!(config.stack, Some(0x801F_FF00));
}
