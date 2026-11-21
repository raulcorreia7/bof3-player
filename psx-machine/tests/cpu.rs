// SPDX-License-Identifier: MIT
//! Instruction tests on synthetic programs: arithmetic, loads and stores with their delay slot,
//! branches and exceptions.

mod common;

use common::*;
use psx_machine::cpu::cop0;
use psx_machine::irq::Line;

const AT: u32 = 1;
const V0: u32 = 2;
const V1: u32 = 3;
const A0: u32 = 4;
const A1: u32 = 5;
const A2: u32 = 6;
const A3: u32 = 7;
const T0: u32 = 8;
const T1: u32 = 9;
const T2: u32 = 10;
const S0: u32 = 16;
const T8: u32 = 24;
const T9: u32 = 25;
const K0: u32 = 26;
const K1: u32 = 27;
const RA: u32 = 31;

const DATA: u32 = 0x8002_0000;

/// An exception handler at 0x80000080 that copies EPC to t9 and CAUSE to t8, then returns to
/// `EPC + skip`.
fn install_handler(machine: &mut psx_machine::Machine, skip: i32) {
    machine.write_words(0x8000_0080, &[mfc0(K0, 14), mfc0(K1, 13), nop(), or(T9, K0, 0), or(T8, K1, 0), addiu(K0, K0, skip), jr(K0), rfe()]);
}

fn reg(machine: &psx_machine::Machine, index: u32) -> u32 {
    machine.cpu().reg(index as usize)
}

#[test]
fn arithmetic_and_logic() {
    let mut program = Vec::new();
    program.extend(li(T0, 0x1234_5678));
    program.extend(li(T1, 0xF0F0_F0F0));
    program.extend([
        addu(A0, T0, T1),
        subu(A1, T0, T1),
        and(A2, T0, T1),
        or(A3, T0, T1),
        xor(V0, T0, T1),
        nor(V1, T0, T1),
        slt(S0, T1, T0),
        sltu(S0 + 1, T1, T0),
        addiu(S0 + 2, 0, -1),
        sltiu(S0 + 3, 0, -1),
        slti(S0 + 4, T1, 0),
        andi(S0 + 5, T1, 0xFF00),
        xori(S0 + 6, T0, 0xFFFF),
        sll(T8, T0, 4),
        srl(T9, T1, 4),
        sra(K0, T1, 4),
    ]);
    let machine = run(&program);
    assert_eq!(reg(&machine, A0), 0x1234_5678u32.wrapping_add(0xF0F0_F0F0));
    assert_eq!(reg(&machine, A1), 0x1234_5678u32.wrapping_sub(0xF0F0_F0F0));
    assert_eq!(reg(&machine, A2), 0x1030_5070);
    assert_eq!(reg(&machine, A3), 0xF2F4_F6F8);
    assert_eq!(reg(&machine, V0), 0xE2C4_A688);
    assert_eq!(reg(&machine, V1), !0xF2F4_F6F8);
    assert_eq!(reg(&machine, S0), 1, "signed: 0xF0F0F0F0 is negative");
    assert_eq!(reg(&machine, S0 + 1), 0, "unsigned: 0xF0F0F0F0 is larger");
    assert_eq!(reg(&machine, S0 + 2), 0xFFFF_FFFF);
    assert_eq!(reg(&machine, S0 + 3), 1, "sltiu sign-extends its immediate");
    assert_eq!(reg(&machine, S0 + 4), 1);
    assert_eq!(reg(&machine, S0 + 5), 0xF000);
    assert_eq!(reg(&machine, S0 + 6), 0x1234_A987);
    assert_eq!(reg(&machine, T8), 0x2345_6780);
    assert_eq!(reg(&machine, T9), 0x0F0F_0F0F);
    assert_eq!(reg(&machine, K0), 0xFF0F_0F0F);
}

#[test]
fn multiply_and_divide() {
    let mut program = Vec::new();
    program.extend(li(T0, 0xFFFF_FFFE)); // -2
    program.extend(li(T1, 0x0000_0003));
    program.extend([mult(T0, T1), mfhi(A0), mflo(A1), multu(T0, T1), mfhi(A2), mflo(A3), div(T0, T1), mfhi(V0), mflo(V1), divu(T0, T1), mfhi(S0), mflo(S0 + 1)]);
    let machine = run(&program);
    assert_eq!((reg(&machine, A0), reg(&machine, A1)), (0xFFFF_FFFF, 0xFFFF_FFFA));
    assert_eq!((reg(&machine, A2), reg(&machine, A3)), (2, 0xFFFF_FFFA));
    assert_eq!((reg(&machine, V0), reg(&machine, V1)), (0xFFFF_FFFE, 0), "-2 / 3 truncates to 0");
    assert_eq!((reg(&machine, S0), reg(&machine, S0 + 1)), (0xFFFF_FFFE % 3, 0xFFFF_FFFE / 3));
}

#[test]
fn load_lands_after_the_delay_slot() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend([addiu(T0, 0, 1), lw(T0, 0, A0), addu(T1, T0, 0), addu(T2, T0, 0)]);
    let (mut machine, end) = machine_with(&program);
    machine.write_words(DATA, &[0xCAFE_F00D]);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!(reg(&machine, T1), 1, "the delay slot sees the old value");
    assert_eq!(reg(&machine, T2), 0xCAFE_F00D);
}

#[test]
fn branch_delay_slot_runs_and_links() {
    let program = [
        beq(0, 0, 3),    // CODE+0 -> CODE+16
        addiu(T0, 0, 1), // delay slot runs
        addiu(T1, 0, 1), // skipped
        addiu(T1, 0, 2), // skipped
        bne(0, 0, 4),    // CODE+16 not taken
        addiu(T2, 0, 3), // delay slot runs
        jal(CODE + 40),  // CODE+24
        nop(),           // CODE+28
        j(CODE + 52),    // CODE+32 return lands here
        nop(),           // CODE+36
        jr(RA),          // CODE+40
        addiu(A0, 0, 9), // CODE+44 delay slot of the return
        nop(),           // CODE+48
        bltzal(0, 10),   // CODE+52 not taken, links anyway
        nop(),
    ];
    let machine = run(&program);
    assert_eq!(reg(&machine, T0), 1);
    assert_eq!(reg(&machine, T1), 0);
    assert_eq!(reg(&machine, T2), 3);
    assert_eq!(reg(&machine, A0), 9);
    assert_eq!(reg(&machine, RA), CODE + 60);
}

#[test]
fn overflow_traps_without_writing() {
    let mut program = Vec::new();
    program.extend(li(T0, 0x7FFF_FFFF));
    program.extend([addiu(T1, 0, 7), add(T1, T0, T0), addi(T2, T0, 1), sub(V0, 0, 0), addu(A0, T0, T0)]);
    let (mut machine, end) = machine_with(&program);
    install_handler(&mut machine, 4);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!(reg(&machine, T1), 7, "add does not write on overflow");
    assert_eq!(reg(&machine, T2), 0);
    assert_eq!((reg(&machine, T8) >> 2) & 0x1F, 12);
    assert_eq!(reg(&machine, T9), CODE + 16, "EPC is the second faulting instruction");
    assert_eq!(reg(&machine, A0), 0xFFFF_FFFE, "addu wraps");
}

#[test]
fn exception_pushes_and_rfe_pops_the_mode_stack() {
    let (mut machine, end) = machine_with(&[brk(), nop()]);
    install_handler(&mut machine, 4);
    machine.cpu_mut().set_cop0(cop0::SR, 0b00_0001);
    machine.run_until_pc(0x8000_0084, 100).unwrap();
    assert_eq!(machine.cpu().cop0(cop0::SR) & 0x3F, 0b00_0100);
    machine.run_until_pc(end, 100).unwrap();
    assert_eq!(machine.cpu().cop0(cop0::SR) & 0x3F, 0b00_0001);
    assert_eq!((reg(&machine, T8) >> 2) & 0x1F, 9);
}

#[test]
fn misaligned_accesses_raise_address_errors() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA + 2));
    program.extend([lw(T0, 0, A0), nop(), or(S0, T8, 0), sh(T0, 1, A0), nop()]);
    let (mut machine, end) = machine_with(&program);
    install_handler(&mut machine, 4);
    machine.run_until_pc(CODE + 20, 1000).unwrap();
    assert_eq!((reg(&machine, S0) >> 2) & 0x1F, 4);
    assert_eq!(machine.cpu().cop0(cop0::BADVADDR), DATA + 2);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!((reg(&machine, T8) >> 2) & 0x1F, 5);
    assert_eq!(machine.cpu().cop0(cop0::BADVADDR), DATA + 3);
}

#[test]
fn interrupt_enters_the_handler_between_instructions() {
    let mut program = Vec::new();
    program.extend(li(AT, 0x1F80_0000));
    program.extend([
        addiu(T0, 0, 1),
        sw(T0, 0x1074, AT), // I_MASK: VBlank
        addiu(T0, 0, 0x0401),
        mtc0(T0, 12), // SR: IEc and IM2
        nop(),
        addiu(V0, 0, 1),
        addiu(V0, V0, 1),
        addiu(V0, V0, 1),
    ]);
    let (mut machine, end) = machine_with(&program);
    machine.write_words(
        0x8000_0080,
        &[
            mfc0(K0, 14),
            mfc0(K1, 13),
            lui(AT, 0x1F80),
            sw(0, 0x1070, AT), // acknowledge every line
            or(T9, K0, 0),
            or(T8, K1, 0),
            jr(K0),
            rfe(),
        ],
    );
    machine.run_until_pc(CODE + 4 * 8, 100).unwrap();
    machine.bus_mut().irq.raise(Line::VBlank);
    machine.run_until_pc(end, 100).unwrap();
    assert_eq!((reg(&machine, T8) >> 2) & 0x1F, 0);
    assert_ne!(reg(&machine, T8) & (1 << 10), 0, "CAUSE shows IP2");
    assert_eq!(reg(&machine, T9), CODE + 4 * 8, "EPC is the instruction not yet run");
    assert_eq!(reg(&machine, V0), 3, "the interrupted code resumes exactly once");
    assert_eq!(machine.bus().irq.status(), 0);
}
