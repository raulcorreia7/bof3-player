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
const A3: u32 = 7;
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

#[test]
fn string_and_memory_functions() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend(bios(0xA0, 0x1B)); // strlen
    program.push(or(S0, V0, 0));
    program.extend(li(A0, DATA + 0x100));
    program.extend(li(A1, DATA));
    program.extend(bios(0xA0, 0x19)); // strcpy
    program.extend(li(A0, DATA));
    program.extend(li(A1, DATA + 0x100));
    program.extend(bios(0xA0, 0x17)); // strcmp
    program.push(or(S1, V0, 0));
    program.extend(li(A0, DATA + 0x200));
    program.extend(li(A1, DATA));
    program.push(addiu(A2, 0, 3));
    program.extend(bios(0xA0, 0x2A)); // memcpy
    program.push(or(S2, V0, 0));
    let (mut machine, end) = kernel_with(&program);
    machine.write_ram(DATA, b"BREATH\0");
    finish(&mut machine, end);
    assert_eq!(reg(&machine, S0), 6);
    assert_eq!(reg(&machine, S1), 0);
    assert_eq!(reg(&machine, S2), DATA + 0x200, "memcpy returns the destination");
    let at = (DATA & 0x1F_FFFF) as usize;
    assert_eq!(&machine.ram()[at + 0x100..at + 0x107], b"BREATH\0");
    assert_eq!(&machine.ram()[at + 0x200..at + 0x204], b"BRE\0");
}

#[test]
fn printf_formats_into_the_tty() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend(li(A1, 0x8018_5A60));
    program.push(addiu(A2, 0, -7));
    program.extend(li(A3, DATA + 0x40));
    program.extend(bios(0xA0, 0x3F));
    program.extend(li(A0, DATA + 0x40));
    program.extend(bios(0xB0, 0x3F)); // puts
    let (mut machine, end) = kernel_with(&program);
    machine.write_ram(DATA, b"addr=%x %3d [%s]%%\n\0");
    machine.write_ram(DATA + 0x40, b"ok\0");
    finish(&mut machine, end);
    assert_eq!(machine.kernel().tty, "addr=80185a60  -7 [ok]%\nok\n");
}

#[test]
fn setjmp_and_longjmp() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.push(addiu(S0, 0, 11));
    program.extend(bios(0xA0, 0x13)); // setjmp: returns 0, then 5 from longjmp
    program.push(bne(V0, 0, 7)); // CODE+24 -> done
    program.push(nop());
    program.push(addiu(S0, 0, 22)); // clobbered, restored by longjmp
    program.extend(li(A0, DATA));
    program.push(addiu(A1, 0, 5));
    program.extend(bios(0xA0, 0x14)); // longjmp
    program.push(or(S1, V0, 0)); // done
    let (mut machine, end) = kernel_with(&program);
    finish(&mut machine, end);
    assert_eq!(reg(&machine, S1), 5);
    assert_eq!(reg(&machine, S0), 11, "s0 comes back from the buffer");
}

#[test]
fn events_become_ready_when_delivered() {
    let mut program = Vec::new();
    program.extend(li(A0, 0xF400_0001));
    program.push(addiu(A1, 0, 0x100));
    program.push(addiu(A2, 0, 0x2000));
    program.push(addiu(A3, 0, 0));
    program.extend(bios(0xB0, 0x08)); // OpenEvent
    program.push(or(S0, V0, 0));
    program.push(or(A0, S0, 0));
    program.extend(bios(0xB0, 0x0C)); // EnableEvent
    program.push(or(A0, S0, 0));
    program.extend(bios(0xB0, 0x0B)); // TestEvent: not yet
    program.push(or(S1, V0, 0));
    program.extend(li(A0, 0xF400_0001));
    program.push(addiu(A1, 0, 0x100));
    program.extend(bios(0xB0, 0x07)); // DeliverEvent
    program.push(or(A0, S0, 0));
    program.extend(bios(0xB0, 0x0B)); // TestEvent: ready, and resets
    program.push(or(S2, V0, 0));
    let (mut machine, end) = kernel_with(&program);
    finish(&mut machine, end);
    assert_eq!(reg(&machine, S0), 0xF100_0000);
    assert_eq!((reg(&machine, S1), reg(&machine, S2)), (0, 1));
    assert_eq!(machine.kernel().events()[0].status, 0x2000);
}

#[test]
fn critical_sections_mask_interrupts() {
    let program = [
        addiu(A0, 0, 1),
        syscall(), // EnterCriticalSection
        or(S0, V0, 0),
        mfc0(S1, 12),
        addiu(A0, 0, 2),
        syscall(), // ExitCriticalSection
        mfc0(S2, 12),
        nop(),
    ];
    let (mut machine, end) = kernel_with(&program);
    machine.cpu_mut().set_cop0(12, 0x0000_0401);
    finish(&mut machine, end);
    assert_eq!(reg(&machine, S0), 1, "interrupts were enabled");
    assert_eq!(reg(&machine, S1) & 0x401, 0);
    assert_eq!(reg(&machine, S2) & 0x401, 0x401);
}

/// An interrupt runs the priority chain, then the HookEntryInt buffer, whose handler returns through
/// ReturnFromException to the interrupted loop.
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
    let spin = CODE + 4 * program.len() as u32;
    program.push(beq(S2, 0, -1));
    program.push(addiu(S1, S1, 1));
    let (mut machine, _) = kernel_with(&program);
    machine.write_words(CHAIN, &[0, 0, FIRST, 0]);
    machine.write_words(FIRST, &[jr(RA), addiu(S0, S0, 1)]);
    machine.write_words(JMPBUF, &[HANDLER, 0x801F_E000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut handler = Vec::new();
    handler.extend(li(AT, 0x1F80_0000));
    handler.push(sw(0, 0x1070, AT)); // acknowledge
    handler.extend(bios(0xB0, 0x17)); // ReturnFromException
    machine.write_words(HANDLER, &handler);
    machine.run_until_pc(spin, 100).unwrap();
    machine.bus_mut().irq.raise(Line::VBlank);
    machine.run_until(|m| m.kernel().calls.get(&(0xB0, 0x17)).copied() == Some(1), 10_000).unwrap();
    assert_eq!(machine.cpu().pc(), spin, "back in the interrupted loop");
    assert_eq!(reg(&machine, S0), 0, "the chain ran with saved registers, restored after");
}

#[test]
fn chain_first_function_runs_on_interrupt() {
    const CHAIN: u32 = DATA + 0x100;
    const FIRST: u32 = CODE + 0x400;
    let mut program = Vec::new();
    program.push(addiu(A0, 0, 0));
    program.extend(li(A1, CHAIN));
    program.extend(bios(0xC0, 0x02));
    program.extend(li(AT, 0x1F80_0000));
    program.push(addiu(T0, 0, 1));
    program.push(sw(T0, 0x1074, AT));
    program.push(addiu(T0, 0, 0x401));
    program.push(mtc0(T0, 12));
    let spin = CODE + 4 * program.len() as u32;
    program.push(beq(0, 0, -1));
    program.push(nop());
    let (mut machine, _) = kernel_with(&program);
    machine.write_words(CHAIN, &[0, 0, FIRST, 0]);
    let mut first = Vec::new();
    first.extend(li(AT, DATA));
    first.extend([lw(T0, 0, AT), nop(), addiu(T0, T0, 1), sw(T0, 0, AT), jr(RA), addiu(V0, 0, 0)]);
    machine.write_words(FIRST, &first);
    machine.run_until_pc(spin, 100).unwrap();
    machine.run_frames(3, 10_000_000).unwrap();
    machine.run_until(|_| false, 1000).unwrap();
    assert_eq!(machine.bus().peek32(DATA), Some(3), "one call per vertical blank");
    assert_eq!(machine.frame(), 3);
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
fn exec_runs_a_child_and_returns_one() {
    const HEADER: u32 = DATA;
    const CHILD: u32 = 0x8004_0000;
    let mut program = Vec::new();
    program.extend(li(A0, HEADER));
    program.push(addiu(A1, 0, 0));
    program.push(addiu(A2, 0, 0));
    program.extend(bios(0xA0, 0x43));
    program.push(or(S0, V0, 0));
    program.push(or(S1, SP, 0));
    let (mut machine, end) = kernel_with(&program);
    machine.write_words(HEADER, &[CHILD, 0, CHILD, 8, 0, 0, 0, 0, 0x801F_8000, 0x100]);
    machine.write_words(CHILD, &[jr(RA), or(S2, SP, 0)]);
    finish(&mut machine, end);
    assert_eq!(reg(&machine, S0), 1);
    assert_eq!(reg(&machine, S2), 0x801F_8100, "the child ran on the header's stack");
    assert_eq!(reg(&machine, S1), 0x801F_FF00, "the caller's stack came back");
}

#[test]
fn pad_word_follows_the_buttons() {
    let mut program = Vec::new();
    program.extend(li(A0, 0x2000_0001));
    program.extend(li(A1, DATA));
    program.extend(bios(0xB0, 0x15)); // PAD_init
    program.extend(li(AT, 0x1F80_0000));
    program.push(addiu(T0, 0, 1));
    program.push(sw(T0, 0x1074, AT));
    program.push(addiu(T0, 0, 0x401));
    program.push(mtc0(T0, 12));
    let spin = CODE + 4 * program.len() as u32;
    program.push(beq(0, 0, -1));
    program.push(nop());
    let (mut machine, _) = kernel_with(&program);
    machine.run_until_pc(spin, 100).unwrap();
    machine.kernel_mut().buttons = 1 << 3; // Start
    machine.run_frames(1, 10_000_000).unwrap();
    machine.run_until(|_| false, 1000).unwrap();
    let word = machine.bus().peek32(DATA).unwrap();
    assert_eq!(!word & 0xFFFF, 0x0800, "Start is bit 11 of the PsyQ pad word");
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
