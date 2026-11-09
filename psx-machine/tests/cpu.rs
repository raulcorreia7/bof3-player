// SPDX-License-Identifier: MIT
//! Instruction tests on synthetic programs: arithmetic, loads and stores with their delay slot, branches and
//! exceptions.

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

/// An exception handler at 0x80000080 that copies EPC to t9 and CAUSE to t8, then returns to `EPC + skip`.
fn install_handler(machine: &mut psx_machine::Machine, skip: i32) {
    machine.write_words(
        0x8000_0080,
        &[mfc0(K0, 14), mfc0(K1, 13), nop(), or(T9, K0, 0), or(T8, K1, 0), addiu(K0, K0, skip), jr(K0), rfe()],
    );
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
fn variable_shifts_use_low_five_bits() {
    let mut program = Vec::new();
    program.extend(li(T0, 0x8000_0001));
    program.extend([addiu(T1, 0, 33), sllv(A0, T0, T1), srlv(A1, T0, T1), srav(A2, T0, T1)]);
    let machine = run(&program);
    assert_eq!(reg(&machine, A0), 0x0000_0002);
    assert_eq!(reg(&machine, A1), 0x4000_0000);
    assert_eq!(reg(&machine, A2), 0xC000_0000);
}

#[test]
fn register_zero_stays_zero() {
    let machine = run(&[addiu(0, 0, 5), lui(0, 0x1234), addu(A0, 0, 0)]);
    assert_eq!(reg(&machine, 0), 0);
    assert_eq!(reg(&machine, A0), 0);
}

#[test]
fn multiply_and_divide() {
    let mut program = Vec::new();
    program.extend(li(T0, 0xFFFF_FFFE)); // -2
    program.extend(li(T1, 0x0000_0003));
    program.extend([
        mult(T0, T1),
        mfhi(A0),
        mflo(A1),
        multu(T0, T1),
        mfhi(A2),
        mflo(A3),
        div(T0, T1),
        mfhi(V0),
        mflo(V1),
        divu(T0, T1),
        mfhi(S0),
        mflo(S0 + 1),
    ]);
    let machine = run(&program);
    assert_eq!((reg(&machine, A0), reg(&machine, A1)), (0xFFFF_FFFF, 0xFFFF_FFFA));
    assert_eq!((reg(&machine, A2), reg(&machine, A3)), (2, 0xFFFF_FFFA));
    assert_eq!((reg(&machine, V0), reg(&machine, V1)), (0xFFFF_FFFE, 0), "-2 / 3 truncates to 0");
    assert_eq!((reg(&machine, S0), reg(&machine, S0 + 1)), (0xFFFF_FFFE % 3, 0xFFFF_FFFE / 3));
}

#[test]
fn division_edge_cases() {
    let mut program = Vec::new();
    program.extend(li(T0, 0x8000_0000));
    program.extend([
        addiu(T1, 0, -1),
        div(T0, T1),
        mfhi(A0),
        mflo(A1),
        addiu(T2, 0, 7),
        div(T2, 0),
        mfhi(A2),
        mflo(A3),
        div(T1, 0),
        mflo(V0),
        divu(T2, 0),
        mfhi(V1),
        mflo(S0),
        mthi(T2),
        mtlo(T1),
        mfhi(S0 + 1),
        mflo(S0 + 2),
    ]);
    let machine = run(&program);
    assert_eq!((reg(&machine, A0), reg(&machine, A1)), (0, 0x8000_0000));
    assert_eq!((reg(&machine, A2), reg(&machine, A3)), (7, 0xFFFF_FFFF), "positive / 0");
    assert_eq!(reg(&machine, V0), 1, "negative / 0");
    assert_eq!((reg(&machine, V1), reg(&machine, S0)), (7, 0xFFFF_FFFF), "unsigned / 0");
    assert_eq!((reg(&machine, S0 + 1), reg(&machine, S0 + 2)), (7, 0xFFFF_FFFF));
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
fn write_in_the_delay_slot_wins_over_the_load() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend([lw(T0, 0, A0), addiu(T0, 0, 5), nop()]);
    let (mut machine, end) = machine_with(&program);
    machine.write_words(DATA, &[0x1111_1111]);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!(reg(&machine, T0), 5);
}

#[test]
fn back_to_back_loads_to_one_register() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend([lw(T0, 0, A0), lw(T0, 4, A0), addu(T1, T0, 0), addu(T2, T0, 0)]);
    let (mut machine, end) = machine_with(&program);
    machine.write_words(DATA, &[0x1111_1111, 0x2222_2222]);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!(reg(&machine, T1), 0, "the second load replaces the first, still in flight");
    assert_eq!(reg(&machine, T2), 0x2222_2222);
}

#[test]
fn byte_and_halfword_loads_extend() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend([lb(T0, 0, A0), lbu(T1, 0, A0), lh(T2, 2, A0), lhu(A1, 2, A0), lb(A2, 1, A0), nop()]);
    let (mut machine, end) = machine_with(&program);
    machine.write_words(DATA, &[0x8001_7F80]);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!(reg(&machine, T0), 0xFFFF_FF80);
    assert_eq!(reg(&machine, T1), 0x80);
    assert_eq!(reg(&machine, T2), 0xFFFF_8001);
    assert_eq!(reg(&machine, A1), 0x8001);
    assert_eq!(reg(&machine, A2), 0x7F);
}

#[test]
fn stores_of_each_width() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend(li(T0, 0xAABB_CCDD));
    program.extend([sw(T0, 0, A0), sh(T0, 4, A0), sb(T0, 7, A0), sb(T0, 8, A0)]);
    let machine = run(&program);
    let at = (DATA & 0x1F_FFFF) as usize;
    assert_eq!(&machine.ram()[at..at + 9], &[0xDD, 0xCC, 0xBB, 0xAA, 0xDD, 0xCC, 0, 0xDD, 0xDD]);
}

#[test]
fn unaligned_word_load_and_store() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend([
        lwr(T0, 1, A0),
        lwl(T0, 4, A0),
        nop(),
        sw(T0, 0x10, A0),
        lwr(T1, 2, A0),
        lwl(T1, 5, A0),
        nop(),
        swr(T0, 0x21, A0),
        swl(T0, 0x24, A0),
    ]);
    let (mut machine, end) = machine_with(&program);
    machine.write_ram(DATA, &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]);
    machine.run_until_pc(end, 1000).unwrap();
    assert_eq!(reg(&machine, T0), 0x4433_2211);
    assert_eq!(reg(&machine, T1), 0x5544_3322);
    let at = (DATA & 0x1F_FFFF) as usize + 0x20;
    assert_eq!(&machine.ram()[at..at + 6], &[0, 0x11, 0x22, 0x33, 0x44, 0]);
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
fn register_jumps_and_conditional_branches() {
    let mut program = Vec::new();
    program.extend(li(T0, CODE + 32));
    program.extend([
        jalr(A0, T0),     // CODE+8
        nop(),            // CODE+12
        addiu(T1, 0, -1), // CODE+16 back from the call
        bltz(T1, 2),      // CODE+20 taken -> CODE+32? no: +4+8 = CODE+32
        nop(),
        nop(),
        jr(A0),
        addiu(V0, V0, 1),
    ]);
    let (mut machine, _) = machine_with(&program);
    machine.run_until_pc(CODE + 20, 100).unwrap();
    assert_eq!(reg(&machine, A0), CODE + 16, "jalr links to the instruction after its delay slot");
    machine.run_until_pc(CODE + 32, 100).unwrap();
    assert_eq!(reg(&machine, T1), 0xFFFF_FFFF);
    assert_eq!(reg(&machine, V0), 1);

    let mut program = Vec::new();
    program.extend([addiu(T0, 0, 0), blez(T0, 2), nop(), addiu(T1, 0, 1), bgtz(T0, 2), nop(), addiu(T2, 0, 1)]);
    program.extend([bgez(T0, 2), nop(), addiu(A1, 0, 1), bgezal(T0, 2), nop(), addiu(A2, 0, 1)]);
    let machine = run(&program);
    assert_eq!(reg(&machine, T1), 0, "blez taken on zero");
    assert_eq!(reg(&machine, T2), 1, "bgtz not taken on zero");
    assert_eq!(reg(&machine, A1), 0, "bgez taken on zero");
    assert_eq!(reg(&machine, A2), 0, "bgezal taken on zero");
    assert_eq!(reg(&machine, RA), CODE + 4 * 12);
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
fn syscall_in_a_delay_slot_reports_the_branch() {
    let program = [nop(), beq(0, 0, 4), syscall(), nop()];
    let (mut machine, _) = machine_with(&program);
    machine.run_until_pc(0x8000_0080, 100).unwrap();
    let cause = machine.cpu().cop0(cop0::CAUSE);
    assert_eq!((cause >> 2) & 0x1F, 8);
    assert_ne!(cause & (1 << 31), 0, "BD is set");
    assert_eq!(machine.cpu().cop0(cop0::EPC), CODE + 4);
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
fn boot_exception_vector_when_bev_is_set() {
    let (mut machine, _) = machine_with(&[syscall()]);
    machine.cpu_mut().set_cop0(cop0::SR, 1 << 22);
    machine.step().unwrap();
    assert_eq!(machine.cpu().pc(), 0xBFC0_0180);
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
fn jump_to_a_misaligned_address_faults_on_fetch() {
    let mut program = Vec::new();
    program.extend(li(T0, CODE + 2));
    program.extend([jr(T0), nop()]);
    let (mut machine, _) = machine_with(&program);
    machine.run_until_pc(0x8000_0080, 100).unwrap();
    assert_eq!((machine.cpu().cop0(cop0::CAUSE) >> 2) & 0x1F, 4);
    assert_eq!(machine.cpu().cop0(cop0::EPC), CODE + 2);
    assert_eq!(machine.cpu().cop0(cop0::BADVADDR), CODE + 2);
}

#[test]
fn reserved_and_unusable_coprocessor_instructions() {
    let (mut machine, _) = machine_with(&[0xFC00_0000]);
    machine.run_until_pc(0x8000_0080, 10).unwrap();
    assert_eq!((machine.cpu().cop0(cop0::CAUSE) >> 2) & 0x1F, 10);

    let (mut machine, _) = machine_with(&[mtc2(T0, 0)]);
    machine.run_until_pc(0x8000_0080, 10).unwrap();
    let cause = machine.cpu().cop0(cop0::CAUSE);
    assert_eq!(((cause >> 2) & 0x1F, (cause >> 28) & 3), (11, 2), "GTE disabled in SR");

    let (mut machine, _) = machine_with(&[0x4400_0000]);
    machine.run_until_pc(0x8000_0080, 10).unwrap();
    let cause = machine.cpu().cop0(cop0::CAUSE);
    assert_eq!(((cause >> 2) & 0x1F, (cause >> 28) & 3), (11, 1), "no coprocessor 1");
}

#[test]
fn gte_registers_move_when_enabled() {
    let mut program = Vec::new();
    program.extend(li(T0, 0x1234_5678));
    program.extend(li(A0, DATA));
    program.extend([mtc2(T0, 0), ctc2(T0, 5), mfc2(T1, 0), cfc2(T2, 5), lwc2(24, 0, A0), swc2(24, 4, A0), nop()]);
    let (mut machine, end) = machine_with(&program);
    machine.cpu_mut().set_cop0(cop0::SR, 1 << 30);
    machine.write_words(DATA, &[0xDEAD_BEEF]);
    machine.run_until_pc(end, 100).unwrap();
    assert_eq!((reg(&machine, T1), reg(&machine, T2)), (0x1234_5678, 0x1234_5678));
    assert_eq!(machine.bus().peek32(DATA + 4), Some(0xDEAD_BEEF));
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

#[test]
fn memory_mirrors_and_scratchpad() {
    let mut program = Vec::new();
    program.extend(li(A0, 0xA000_1000));
    program.extend(li(A1, 0x8060_1000)); // the fourth 2 MB mirror
    program.extend(li(A2, 0x1F80_0000));
    program.extend(li(T0, 0x5555_AAAA));
    program.extend([sw(T0, 0, A0), lw(T1, 0, A1), sw(T0, 0x3FC, A2), lw(T2, 0x3FC, A2), nop()]);
    let machine = run(&program);
    assert_eq!(reg(&machine, T1), 0x5555_AAAA);
    assert_eq!(reg(&machine, T2), 0x5555_AAAA);
    assert_eq!(&machine.bus().scratchpad()[0x3FC..], &0x5555_AAAAu32.to_le_bytes());
    assert_eq!(machine.bus().peek32(0x0000_1000), Some(0x5555_AAAA));
}

#[test]
fn isolated_cache_drops_stores() {
    let mut program = Vec::new();
    program.extend(li(A0, DATA));
    program.extend(li(T0, 0x0001_0000));
    program.extend([mtc0(T0, 12), addiu(T1, 0, 7), sw(T1, 0, A0), mtc0(0, 12), nop(), lw(T2, 0, A0), nop()]);
    let machine = run(&program);
    assert_eq!(reg(&machine, T2), 0);
}

#[test]
fn mfc0_has_a_load_delay() {
    let mut program = Vec::new();
    program.extend(li(T0, 0x0000_0401));
    program.extend([mtc0(T0, 12), addiu(T1, 0, 3), mfc0(T1, 12), or(T2, T1, 0), or(A0, T1, 0)]);
    let machine = run(&program);
    assert_eq!(reg(&machine, T2), 3);
    assert_eq!(reg(&machine, A0), 0x401);
    let _ = (V1, A3);
}
