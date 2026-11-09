// SPDX-License-Identifier: MIT
//! An encoder for the MIPS I instructions the synthetic test programs use.
#![allow(dead_code)]

use psx_machine::Machine;

fn r(rs: u32, rt: u32, rd: u32, shamt: u32, funct: u32) -> u32 {
    (rs << 21) | (rt << 16) | (rd << 11) | (shamt << 6) | funct
}

fn i(op: u32, rs: u32, rt: u32, imm: i32) -> u32 {
    (op << 26) | (rs << 21) | (rt << 16) | (imm as u32 & 0xFFFF)
}

pub fn nop() -> u32 {
    0
}
pub fn sll(rd: u32, rt: u32, sa: u32) -> u32 {
    r(0, rt, rd, sa, 0x00)
}
pub fn srl(rd: u32, rt: u32, sa: u32) -> u32 {
    r(0, rt, rd, sa, 0x02)
}
pub fn sra(rd: u32, rt: u32, sa: u32) -> u32 {
    r(0, rt, rd, sa, 0x03)
}
pub fn sllv(rd: u32, rt: u32, rs: u32) -> u32 {
    r(rs, rt, rd, 0, 0x04)
}
pub fn srlv(rd: u32, rt: u32, rs: u32) -> u32 {
    r(rs, rt, rd, 0, 0x06)
}
pub fn srav(rd: u32, rt: u32, rs: u32) -> u32 {
    r(rs, rt, rd, 0, 0x07)
}
pub fn jr(rs: u32) -> u32 {
    r(rs, 0, 0, 0, 0x08)
}
pub fn jalr(rd: u32, rs: u32) -> u32 {
    r(rs, 0, rd, 0, 0x09)
}
pub fn syscall() -> u32 {
    0x0C
}
pub fn brk() -> u32 {
    0x0D
}
pub fn mfhi(rd: u32) -> u32 {
    r(0, 0, rd, 0, 0x10)
}
pub fn mthi(rs: u32) -> u32 {
    r(rs, 0, 0, 0, 0x11)
}
pub fn mflo(rd: u32) -> u32 {
    r(0, 0, rd, 0, 0x12)
}
pub fn mtlo(rs: u32) -> u32 {
    r(rs, 0, 0, 0, 0x13)
}
pub fn mult(rs: u32, rt: u32) -> u32 {
    r(rs, rt, 0, 0, 0x18)
}
pub fn multu(rs: u32, rt: u32) -> u32 {
    r(rs, rt, 0, 0, 0x19)
}
pub fn div(rs: u32, rt: u32) -> u32 {
    r(rs, rt, 0, 0, 0x1A)
}
pub fn divu(rs: u32, rt: u32) -> u32 {
    r(rs, rt, 0, 0, 0x1B)
}
pub fn add(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x20)
}
pub fn addu(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x21)
}
pub fn sub(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x22)
}
pub fn subu(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x23)
}
pub fn and(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x24)
}
pub fn or(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x25)
}
pub fn xor(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x26)
}
pub fn nor(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x27)
}
pub fn slt(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x2A)
}
pub fn sltu(rd: u32, rs: u32, rt: u32) -> u32 {
    r(rs, rt, rd, 0, 0x2B)
}
pub fn bltz(rs: u32, offset: i32) -> u32 {
    i(1, rs, 0, offset)
}
pub fn bgez(rs: u32, offset: i32) -> u32 {
    i(1, rs, 1, offset)
}
pub fn bltzal(rs: u32, offset: i32) -> u32 {
    i(1, rs, 0x10, offset)
}
pub fn bgezal(rs: u32, offset: i32) -> u32 {
    i(1, rs, 0x11, offset)
}
pub fn j(target: u32) -> u32 {
    (2 << 26) | ((target >> 2) & 0x03FF_FFFF)
}
pub fn jal(target: u32) -> u32 {
    (3 << 26) | ((target >> 2) & 0x03FF_FFFF)
}
pub fn beq(rs: u32, rt: u32, offset: i32) -> u32 {
    i(4, rs, rt, offset)
}
pub fn bne(rs: u32, rt: u32, offset: i32) -> u32 {
    i(5, rs, rt, offset)
}
pub fn blez(rs: u32, offset: i32) -> u32 {
    i(6, rs, 0, offset)
}
pub fn bgtz(rs: u32, offset: i32) -> u32 {
    i(7, rs, 0, offset)
}
pub fn addi(rt: u32, rs: u32, imm: i32) -> u32 {
    i(8, rs, rt, imm)
}
pub fn addiu(rt: u32, rs: u32, imm: i32) -> u32 {
    i(9, rs, rt, imm)
}
pub fn slti(rt: u32, rs: u32, imm: i32) -> u32 {
    i(0x0A, rs, rt, imm)
}
pub fn sltiu(rt: u32, rs: u32, imm: i32) -> u32 {
    i(0x0B, rs, rt, imm)
}
pub fn andi(rt: u32, rs: u32, imm: u32) -> u32 {
    i(0x0C, rs, rt, imm as i32)
}
pub fn ori(rt: u32, rs: u32, imm: u32) -> u32 {
    i(0x0D, rs, rt, imm as i32)
}
pub fn xori(rt: u32, rs: u32, imm: u32) -> u32 {
    i(0x0E, rs, rt, imm as i32)
}
pub fn lui(rt: u32, imm: u32) -> u32 {
    i(0x0F, 0, rt, imm as i32)
}
pub fn mfc0(rt: u32, rd: u32) -> u32 {
    (0x10 << 26) | (rt << 16) | (rd << 11)
}
pub fn mtc0(rt: u32, rd: u32) -> u32 {
    (0x10 << 26) | (4 << 21) | (rt << 16) | (rd << 11)
}
pub fn rfe() -> u32 {
    0x4200_0010
}
pub fn mfc2(rt: u32, rd: u32) -> u32 {
    (0x12 << 26) | (rt << 16) | (rd << 11)
}
pub fn mtc2(rt: u32, rd: u32) -> u32 {
    (0x12 << 26) | (4 << 21) | (rt << 16) | (rd << 11)
}
pub fn cfc2(rt: u32, rd: u32) -> u32 {
    (0x12 << 26) | (2 << 21) | (rt << 16) | (rd << 11)
}
pub fn ctc2(rt: u32, rd: u32) -> u32 {
    (0x12 << 26) | (6 << 21) | (rt << 16) | (rd << 11)
}
pub fn cop2(command: u32) -> u32 {
    (0x12 << 26) | (1 << 25) | (command & 0x01FF_FFFF)
}
pub fn lb(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x20, base, rt, offset)
}
pub fn lh(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x21, base, rt, offset)
}
pub fn lwl(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x22, base, rt, offset)
}
pub fn lw(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x23, base, rt, offset)
}
pub fn lbu(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x24, base, rt, offset)
}
pub fn lhu(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x25, base, rt, offset)
}
pub fn lwr(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x26, base, rt, offset)
}
pub fn sb(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x28, base, rt, offset)
}
pub fn sh(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x29, base, rt, offset)
}
pub fn swl(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x2A, base, rt, offset)
}
pub fn sw(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x2B, base, rt, offset)
}
pub fn swr(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x2E, base, rt, offset)
}
pub fn lwc2(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x32, base, rt, offset)
}
pub fn swc2(rt: u32, offset: i32, base: u32) -> u32 {
    i(0x3A, base, rt, offset)
}

/// Loads a 32-bit constant into `rt` with `lui` and `ori`.
pub fn li(rt: u32, value: u32) -> [u32; 2] {
    [lui(rt, value >> 16), ori(rt, rt, value & 0xFFFF)]
}

/// Where test programs start.
pub const CODE: u32 = 0x8001_0000;

/// A machine with `program` at [`CODE`], exception vectors in RAM, followed by an endless loop
/// whose address it returns; the run stops there.
pub fn machine_with(program: &[u32]) -> (Machine, u32) {
    let mut machine = Machine::new();
    let mut words = program.to_vec();
    let end = CODE + 4 * words.len() as u32;
    words.push(j(end));
    words.push(nop());
    machine.write_words(CODE, &words);
    machine.cpu_mut().set_cop0(psx_machine::cpu::cop0::SR, 0);
    machine.cpu_mut().set_pc(CODE);
    (machine, end)
}

/// Runs `program` to its end and returns the machine.
pub fn run(program: &[u32]) -> Machine {
    let (mut machine, end) = machine_with(program);
    assert_eq!(machine.run_until_pc(end, 100_000), Ok(psx_machine::Stop::Pc));
    machine
}
