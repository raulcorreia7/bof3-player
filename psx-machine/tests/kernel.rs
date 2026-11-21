// SPDX-License-Identifier: MIT
//! The stand-in kernel: function calls, events, interrupts, threads and `Exec`.

mod common;

use common::*;
use psx_machine::irq::Line;
use psx_machine::{Halt, Machine, Stop};

const AT: u32 = 1;
const V0: u32 = 2;
const A0: u32 = 4;
const A1: u32 = 5;
const A2: u32 = 6;
const T0: u32 = 8;
const S0: u32 = 16;
const S1: u32 = 17;
const S2: u32 = 18;
const SP: u32 = 29;
const RA: u32 = 31;

const DATA: u32 = 0x8002_0000;

fn reg(machine: &Machine, index: u32) -> u32 {
    machine.cpu().reg(index as usize)
}

fn finish(machine: &mut Machine, end: u32) {
    assert_eq!(machine.run_until_pc(end, 1_000_000), Ok(Stop::Pc));
}

/// An interrupt runs the priority chain, then the HookEntryInt buffer, whose handler returns
/// through ReturnFromException to the interrupted loop.
#[test]
fn interrupt_runs_chain_hook_and_returns() {
    const CHAIN: u32 = DATA + 0x100;
    const JMPBUF: u32 = DATA + 0x200;
    const FIRST: u32 = CODE + 0x400;
    const HANDLER: u32 = CODE + 0x500;
    let mut program = Vec::new();
    program.extend(li(A0, JMPBUF));
    program.extend(bios(0xB0, 0x19)); // HookEntryInt
    program.push(addiu(A0, 0, 2));
    program.extend(li(A1, CHAIN));
    program.extend(bios(0xC0, 0x02)); // SysEnqIntRP(2, chain)
    program.extend(li(AT, 0x1F80_0000));
    program.push(addiu(T0, 0, 1));
    program.push(sw(T0, 0x1074, AT)); // I_MASK: VBlank
    program.push(addiu(T0, 0, 0x401));
    program.push(mtc0(T0, 12));
    // Spin until the hook handler has counted one interrupt in s2.
    let spin = CODE + 4 * program.len() as u32;
    program.push(beq(S2, 0, -1));
    program.push(addiu(S1, S1, 1));
    let (mut machine, _) = kernel_with(&program);
    // The chain node: next, second function, first function.
    machine.write_words(CHAIN, &[0, 0, FIRST, 0]);
    machine.write_words(FIRST, &[jr(RA), addiu(S0, S0, 1)]);
    // The hook buffer: ra = HANDLER, sp, fp, s0..s7, gp.
    machine.write_words(JMPBUF, &[HANDLER, 0x801F_E000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut handler = Vec::new();
    handler.extend(li(AT, 0x1F80_0000));
    handler.push(sw(0, 0x1070, AT)); // acknowledge
    handler.extend(bios(0xB0, 0x17)); // ReturnFromException
    machine.write_words(HANDLER, &handler);
    machine.run_until_pc(spin, 100).unwrap();
    machine.bus_mut().irq.raise(Line::VBlank);
    // ReturnFromException restores every register the handler touched, so stop on the call.
    machine.run_until(|m| m.kernel().calls.get(&(0xB0, 0x17)).copied() == Some(1), 10_000).unwrap();
    assert_eq!(machine.cpu().pc(), spin, "back in the interrupted loop");
    assert_eq!(reg(&machine, S0), 0, "the chain ran with saved registers, restored after");
}

#[test]
fn threads_switch_and_come_back() {
    const THREAD: u32 = CODE + 0x400;
    let mut program = Vec::new();
    program.extend(li(A0, THREAD));
    program.extend(li(A1, 0x801F_0000));
    program.push(addiu(A2, 0, 0));
    program.extend(bios(0xB0, 0x0E)); // OpenTh
    program.push(or(S0, V0, 0));
    program.push(addiu(S1, 0, 7));
    program.push(or(A0, S0, 0));
    program.extend(bios(0xB0, 0x10)); // ChangeTh: runs the thread, which switches back
    program.push(or(S2, V0, 0));
    let (mut machine, end) = kernel_with(&program);
    let mut thread = Vec::new();
    thread.push(addiu(S1, 0, 99));
    thread.push(or(T0, SP, 0));
    thread.extend(li(AT, DATA));
    thread.push(sw(T0, 0, AT));
    thread.extend(li(A0, 0xFF00_0000));
    thread.extend(bios(0xB0, 0x10)); // back to the main thread
    machine.write_words(THREAD, &thread);
    finish(&mut machine, end);
    assert_eq!(reg(&machine, S0), 0xFF00_0001);
    assert_eq!(reg(&machine, S1), 7, "the main thread's registers came back");
    assert_eq!(reg(&machine, S2), 1);
    assert_eq!(machine.bus().peek32(DATA), Some(0x801F_0000), "the thread ran on its own stack");
}

#[test]
fn missing_function_halts_with_its_number() {
    let (mut machine, end) = kernel_with(&bios(0xA0, 0x99));
    let result = machine.run_until_pc(end, 100);
    assert!(matches!(result, Err(Halt::Bios { table: 0xA0, function: 0x99, .. })), "{result:?}");
}

#[test]
fn boot_executable_starts_at_its_entry() {
    let mut machine = Machine::with_kernel();
    let image = exe(0x8001_0000, &[addiu(S0, 0, 42), jr(RA), nop()]);
    machine.load_executable(&image, Some(0x801F_FF00)).unwrap();
    assert_eq!(machine.cpu().pc(), 0x8001_0000);
    assert_eq!(reg(&machine, SP), 0x801F_FF00);
    assert_eq!(machine.run_until_pc(0, 100), Err(Halt::Returned));
    assert_eq!(reg(&machine, S0), 42);
}
