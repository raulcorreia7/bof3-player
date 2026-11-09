// SPDX-License-Identifier: MIT
//! The R3000A core: the MIPS I instruction set with its branch and load delay slots, coprocessor 0
//! (status, cause, exception PC) and the exceptions the PlayStation uses. Behaviour follows psx-spx
//! "CPU Specifications" and the MIPS I architecture; see `docs/PROVENANCE.md`.

use crate::Halt;
use crate::bus::Bus;
use crate::gte::Gte;

/// The cause of an exception, as `CAUSE.ExcCode` stores it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Exception {
    /// An enabled interrupt line is raised.
    Interrupt = 0,
    /// A load or an instruction fetch from a misaligned address.
    AddressLoad = 4,
    /// A store to a misaligned address.
    AddressStore = 5,
    /// `SYSCALL`.
    Syscall = 8,
    /// `BREAK`.
    Break = 9,
    /// An opcode the R3000A does not define.
    Reserved = 10,
    /// A coprocessor instruction while that coprocessor is disabled or absent.
    CoprocessorUnusable = 11,
    /// A signed `ADD`, `ADDI` or `SUB` overflowed.
    Overflow = 12,
}

/// Coprocessor 0 register numbers the machine reads.
pub mod cop0 {
    /// Bad virtual address of the last address error.
    pub const BADVADDR: usize = 8;
    /// Status register: interrupt enables, mode stack, cache isolation, boot vectors.
    pub const SR: usize = 12;
    /// Cause register: exception code, pending interrupts, branch-delay flag.
    pub const CAUSE: usize = 13;
    /// Exception return address.
    pub const EPC: usize = 14;
    /// Processor ID.
    pub const PRID: usize = 15;
}

const SR_ISOLATE_CACHE: u32 = 1 << 16;
const SR_BOOT_VECTORS: u32 = 1 << 22;
const CAUSE_BRANCH_DELAY: u32 = 1 << 31;
const CAUSE_HARDWARE_IRQ: u32 = 1 << 10;

/// The CPU state: general registers, `HI`/`LO`, the program counters, the pending load and
/// coprocessors 0 and 2.
#[derive(Clone)]
pub struct Cpu {
    regs: [u32; 32],
    hi: u32,
    lo: u32,
    pc: u32,
    next_pc: u32,
    current_pc: u32,
    /// A load whose value lands after the next instruction: (register, value); register 0 is none.
    load: (usize, u32),
    /// The register the current instruction wrote, which cancels a pending load to it.
    written: usize,
    /// The last instruction was a branch or jump, so the next one sits in its delay slot.
    branch: bool,
    delay_slot: bool,
    cop0: [u32; 16],
    /// The geometry transformation engine (coprocessor 2).
    pub gte: Gte,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// A CPU at the reset vector, interrupts disabled and boot vectors selected.
    #[must_use]
    pub fn new() -> Self {
        let mut cop0 = [0; 16];
        cop0[cop0::SR] = SR_BOOT_VECTORS;
        cop0[cop0::PRID] = 0x0000_0002;
        Self {
            regs: [0; 32],
            hi: 0,
            lo: 0,
            pc: 0xBFC0_0000,
            next_pc: 0xBFC0_0004,
            current_pc: 0xBFC0_0000,
            load: (0, 0),
            written: 0,
            branch: false,
            delay_slot: false,
            cop0,
            gte: Gte::default(),
        }
    }

    /// General register `index` (0 to 31).
    #[must_use]
    pub fn reg(&self, index: usize) -> u32 {
        self.regs[index]
    }

    /// Sets general register `index`; writes to register 0 are dropped.
    pub fn set_reg(&mut self, index: usize, value: u32) {
        if index != 0 {
            self.regs[index] = value;
        }
    }

    /// The `HI` register.
    #[must_use]
    pub fn hi(&self) -> u32 {
        self.hi
    }

    /// The `LO` register.
    #[must_use]
    pub fn lo(&self) -> u32 {
        self.lo
    }

    /// Sets `HI` and `LO`.
    pub fn set_hi_lo(&mut self, hi: u32, lo: u32) {
        self.hi = hi;
        self.lo = lo;
    }

    /// The address of the next instruction to run.
    #[must_use]
    pub fn pc(&self) -> u32 {
        self.pc
    }

    /// The address of the instruction that ran last.
    #[must_use]
    pub fn current_pc(&self) -> u32 {
        self.current_pc
    }

    /// Jumps to `pc` outside any delay slot.
    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
        self.next_pc = pc.wrapping_add(4);
        self.branch = false;
    }

    /// Coprocessor 0 register `index` (see [`cop0`]).
    #[must_use]
    pub fn cop0(&self, index: usize) -> u32 {
        self.cop0[index]
    }

    /// Sets coprocessor 0 register `index`.
    pub fn set_cop0(&mut self, index: usize, value: u32) {
        self.cop0[index] = value;
    }

    /// Writes the pending load now, as when the pipeline drains before an exception or a BIOS call.
    pub fn flush_load(&mut self) {
        let (reg, value) = std::mem::take(&mut self.load);
        self.set_reg(reg, value);
    }

    /// Pops the interrupt-enable and mode stack of `SR`, as `RFE` does.
    pub fn return_from_exception(&mut self) {
        let sr = self.cop0[cop0::SR];
        self.cop0[cop0::SR] = (sr & !0xF) | ((sr >> 2) & 0xF);
    }

    /// Runs one instruction, or enters the interrupt handler when an enabled interrupt is pending.
    ///
    /// # Errors
    ///
    /// [`Halt::Gte`] when the instruction is a GTE command the machine does not implement.
    pub fn step(&mut self, bus: &mut Bus) -> Result<(), Halt> {
        if self.take_interrupt(bus) {
            return Ok(());
        }
        let pc = self.pc;
        self.current_pc = pc;
        self.delay_slot = self.branch;
        self.branch = false;
        if pc & 3 != 0 {
            self.flush_load();
            self.cop0[cop0::BADVADDR] = pc;
            self.exception(Exception::AddressLoad);
            return Ok(());
        }
        let instruction = bus.read32(pc);
        self.pc = self.next_pc;
        self.next_pc = self.pc.wrapping_add(4);

        let pending = std::mem::take(&mut self.load);
        self.written = 0;
        let result = self.execute(instruction, pending, bus);
        if pending.0 != 0 && pending.0 != self.written && pending.0 != self.load.0 {
            self.regs[pending.0] = pending.1;
        }
        self.regs[0] = 0;
        result
    }

    /// Enters the interrupt handler when an enabled interrupt is pending; returns whether it did.
    pub fn take_interrupt(&mut self, bus: &mut Bus) -> bool {
        if bus.irq.pending() {
            self.cop0[cop0::CAUSE] |= CAUSE_HARDWARE_IRQ;
        } else {
            self.cop0[cop0::CAUSE] &= !CAUSE_HARDWARE_IRQ;
        }
        let sr = self.cop0[cop0::SR];
        if sr & 1 == 0 || (sr & self.cop0[cop0::CAUSE] & 0xFF00) == 0 || self.next_is_gte_command(bus) {
            return false;
        }
        self.flush_load();
        self.current_pc = self.pc;
        self.delay_slot = self.branch;
        self.exception(Exception::Interrupt);
        true
    }

    /// Whether the instruction at `pc` is a GTE command. Hardware runs a GTE command before it
    /// takes an interrupt there, so the interrupt waits one instruction.
    fn next_is_gte_command(&self, bus: &mut Bus) -> bool {
        self.pc & 3 == 0 && bus.peek32(self.pc).is_some_and(|word| word >> 25 == 0x25)
    }

    fn exception(&mut self, cause: Exception) {
        let sr = self.cop0[cop0::SR];
        self.cop0[cop0::SR] = (sr & !0x3F) | ((sr << 2) & 0x3F);
        let mut value = (self.cop0[cop0::CAUSE] & !(CAUSE_BRANCH_DELAY | 0x7C)) | ((cause as u32) << 2);
        let epc = if self.delay_slot {
            value |= CAUSE_BRANCH_DELAY;
            self.current_pc.wrapping_sub(4)
        } else {
            self.current_pc
        };
        self.cop0[cop0::CAUSE] = value;
        self.cop0[cop0::EPC] = epc;
        let vector = if sr & SR_BOOT_VECTORS != 0 { 0xBFC0_0180 } else { 0x8000_0080 };
        self.set_pc(vector);
    }

    fn coprocessor_unusable(&mut self, number: u32) {
        self.cop0[cop0::CAUSE] = (self.cop0[cop0::CAUSE] & !(3 << 28)) | (number << 28);
        self.exception(Exception::CoprocessorUnusable);
    }

    fn set(&mut self, reg: usize, value: u32) {
        if reg != 0 {
            self.regs[reg] = value;
            self.written = reg;
        }
    }

    fn delayed(&mut self, reg: usize, value: u32) {
        if reg != 0 {
            self.load = (reg, value);
        }
    }

    fn branch_to(&mut self, target: u32) {
        self.next_pc = target;
        self.branch = true;
    }

    fn relative_branch(&mut self, taken: bool, offset: u32) {
        self.branch = true;
        if taken {
            self.next_pc = self.pc.wrapping_add(offset << 2);
        }
    }

    fn load_address(&mut self, address: u32, align: u32) -> Option<u32> {
        if address & (align - 1) != 0 {
            self.cop0[cop0::BADVADDR] = address;
            self.exception(Exception::AddressLoad);
            return None;
        }
        Some(address)
    }

    fn store_address(&mut self, address: u32, align: u32) -> Option<u32> {
        if address & (align - 1) != 0 {
            self.cop0[cop0::BADVADDR] = address;
            self.exception(Exception::AddressStore);
            return None;
        }
        if self.cop0[cop0::SR] & SR_ISOLATE_CACHE != 0 {
            return None;
        }
        Some(address)
    }

    fn cop_enabled(&self, number: u32) -> bool {
        let sr = self.cop0[cop0::SR];
        if number == 0 {
            // Kernel mode always reaches coprocessor 0.
            sr & 2 == 0 || sr & (1 << 28) != 0
        } else {
            sr & (1 << (28 + number)) != 0
        }
    }

    fn execute(&mut self, word: u32, pending: (usize, u32), bus: &mut Bus) -> Result<(), Halt> {
        let op = word >> 26;
        let rs = ((word >> 21) & 0x1F) as usize;
        let rt = ((word >> 16) & 0x1F) as usize;
        let rd = ((word >> 11) & 0x1F) as usize;
        let shamt = (word >> 6) & 0x1F;
        let imm = word & 0xFFFF;
        let simm = (imm as i16) as i32 as u32;
        let s = self.regs[rs];
        let t = self.regs[rt];
        let address = s.wrapping_add(simm);
        // The value LWL and LWR merge into: the pending load when it targets the same register.
        let merge_base = if pending.0 == rt && rt != 0 { pending.1 } else { t };

        match op {
            0x00 => return self.special(word, rs, rt, rd, shamt),
            0x01 => {
                let link = rt & 0x1E == 0x10;
                let taken = if rt & 1 != 0 { (s as i32) >= 0 } else { (s as i32) < 0 };
                if link {
                    self.set(31, self.next_pc);
                }
                self.relative_branch(taken, simm);
            }
            0x02 => self.branch_to((self.pc & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2)),
            0x03 => {
                self.set(31, self.next_pc);
                self.branch_to((self.pc & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2));
            }
            0x04 => self.relative_branch(s == t, simm),
            0x05 => self.relative_branch(s != t, simm),
            0x06 => self.relative_branch((s as i32) <= 0, simm),
            0x07 => self.relative_branch((s as i32) > 0, simm),
            0x08 => match (s as i32).checked_add(simm as i32) {
                Some(value) => self.set(rt, value as u32),
                None => self.exception(Exception::Overflow),
            },
            0x09 => self.set(rt, s.wrapping_add(simm)),
            0x0A => self.set(rt, u32::from((s as i32) < (simm as i32))),
            0x0B => self.set(rt, u32::from(s < simm)),
            0x0C => self.set(rt, s & imm),
            0x0D => self.set(rt, s | imm),
            0x0E => self.set(rt, s ^ imm),
            0x0F => self.set(rt, imm << 16),
            0x10 => self.cop0_op(word, rt, rd),
            0x11 | 0x13 => self.coprocessor_unusable(op & 3),
            0x12 => return self.cop2_op(word, rt, rd),
            0x20 => {
                if let Some(a) = self.load_address(address, 1) {
                    self.delayed(rt, bus.read8(a) as i8 as i32 as u32);
                }
            }
            0x21 => {
                if let Some(a) = self.load_address(address, 2) {
                    self.delayed(rt, bus.read16(a) as i16 as i32 as u32);
                }
            }
            0x22 => {
                let word = bus.read32(address & !3);
                let value = match address & 3 {
                    0 => (merge_base & 0x00FF_FFFF) | (word << 24),
                    1 => (merge_base & 0x0000_FFFF) | (word << 16),
                    2 => (merge_base & 0x0000_00FF) | (word << 8),
                    _ => word,
                };
                self.delayed(rt, value);
            }
            0x23 => {
                if let Some(a) = self.load_address(address, 4) {
                    self.delayed(rt, bus.read32(a));
                }
            }
            0x24 => {
                if let Some(a) = self.load_address(address, 1) {
                    self.delayed(rt, u32::from(bus.read8(a)));
                }
            }
            0x25 => {
                if let Some(a) = self.load_address(address, 2) {
                    self.delayed(rt, u32::from(bus.read16(a)));
                }
            }
            0x26 => {
                let word = bus.read32(address & !3);
                let value = match address & 3 {
                    0 => word,
                    1 => (merge_base & 0xFF00_0000) | (word >> 8),
                    2 => (merge_base & 0xFFFF_0000) | (word >> 16),
                    _ => (merge_base & 0xFFFF_FF00) | (word >> 24),
                };
                self.delayed(rt, value);
            }
            0x28 => {
                if let Some(a) = self.store_address(address, 1) {
                    bus.write8(a, t as u8);
                }
            }
            0x29 => {
                if let Some(a) = self.store_address(address, 2) {
                    bus.write16(a, t as u16);
                }
            }
            0x2A => {
                if let Some(a) = self.store_address(address & !3, 1) {
                    let memory = bus.read32(a);
                    let value = match address & 3 {
                        0 => (memory & 0xFFFF_FF00) | (t >> 24),
                        1 => (memory & 0xFFFF_0000) | (t >> 16),
                        2 => (memory & 0xFF00_0000) | (t >> 8),
                        _ => t,
                    };
                    bus.write32(a, value);
                }
            }
            0x2B => {
                if let Some(a) = self.store_address(address, 4) {
                    bus.write32(a, t);
                }
            }
            0x2E => {
                if let Some(a) = self.store_address(address & !3, 1) {
                    let memory = bus.read32(a);
                    let value = match address & 3 {
                        0 => t,
                        1 => (memory & 0x0000_00FF) | (t << 8),
                        2 => (memory & 0x0000_FFFF) | (t << 16),
                        _ => (memory & 0x00FF_FFFF) | (t << 24),
                    };
                    bus.write32(a, value);
                }
            }
            0x30 | 0x31 | 0x33 | 0x38 | 0x39 | 0x3B => self.coprocessor_unusable(op & 3),
            0x32 => {
                if !self.cop_enabled(2) {
                    self.coprocessor_unusable(2);
                } else if let Some(a) = self.load_address(address, 4) {
                    let value = bus.read32(a);
                    self.gte.write_data(rt, value);
                }
            }
            0x3A => {
                if !self.cop_enabled(2) {
                    self.coprocessor_unusable(2);
                } else if let Some(a) = self.store_address(address, 4) {
                    let value = self.gte.read_data(rt);
                    bus.write32(a, value);
                }
            }
            _ => self.exception(Exception::Reserved),
        }
        Ok(())
    }

    fn special(&mut self, word: u32, rs: usize, rt: usize, rd: usize, shamt: u32) -> Result<(), Halt> {
        let s = self.regs[rs];
        let t = self.regs[rt];
        match word & 0x3F {
            0x00 => self.set(rd, t << shamt),
            0x02 => self.set(rd, t >> shamt),
            0x03 => self.set(rd, ((t as i32) >> shamt) as u32),
            0x04 => self.set(rd, t << (s & 0x1F)),
            0x06 => self.set(rd, t >> (s & 0x1F)),
            0x07 => self.set(rd, ((t as i32) >> (s & 0x1F)) as u32),
            0x08 => self.branch_to(s),
            0x09 => {
                self.set(rd, self.next_pc);
                self.branch_to(s);
            }
            0x0C => self.exception(Exception::Syscall),
            0x0D => self.exception(Exception::Break),
            0x10 => self.set(rd, self.hi),
            0x11 => self.hi = s,
            0x12 => self.set(rd, self.lo),
            0x13 => self.lo = s,
            0x18 => {
                let product = i64::from(s as i32) * i64::from(t as i32);
                self.hi = (product >> 32) as u32;
                self.lo = product as u32;
            }
            0x19 => {
                let product = u64::from(s) * u64::from(t);
                self.hi = (product >> 32) as u32;
                self.lo = product as u32;
            }
            0x1A => {
                let (n, d) = (s as i32, t as i32);
                if d == 0 {
                    self.hi = s;
                    self.lo = if n >= 0 { 0xFFFF_FFFF } else { 1 };
                } else if n == i32::MIN && d == -1 {
                    self.hi = 0;
                    self.lo = 0x8000_0000;
                } else {
                    self.hi = (n % d) as u32;
                    self.lo = (n / d) as u32;
                }
            }
            0x1B => {
                if t == 0 {
                    self.hi = s;
                    self.lo = 0xFFFF_FFFF;
                } else {
                    self.hi = s % t;
                    self.lo = s / t;
                }
            }
            0x20 => match (s as i32).checked_add(t as i32) {
                Some(value) => self.set(rd, value as u32),
                None => self.exception(Exception::Overflow),
            },
            0x21 => self.set(rd, s.wrapping_add(t)),
            0x22 => match (s as i32).checked_sub(t as i32) {
                Some(value) => self.set(rd, value as u32),
                None => self.exception(Exception::Overflow),
            },
            0x23 => self.set(rd, s.wrapping_sub(t)),
            0x24 => self.set(rd, s & t),
            0x25 => self.set(rd, s | t),
            0x26 => self.set(rd, s ^ t),
            0x27 => self.set(rd, !(s | t)),
            0x2A => self.set(rd, u32::from((s as i32) < (t as i32))),
            0x2B => self.set(rd, u32::from(s < t)),
            _ => self.exception(Exception::Reserved),
        }
        Ok(())
    }

    fn cop0_op(&mut self, word: u32, rt: usize, rd: usize) {
        if !self.cop_enabled(0) {
            self.coprocessor_unusable(0);
            return;
        }
        match (word >> 21) & 0x1F {
            0x00 => self.delayed(rt, self.cop0[rd & 0xF]),
            0x04 => {
                let value = self.regs[rt];
                match rd {
                    cop0::CAUSE => {
                        // Only the two software interrupt bits are writable.
                        self.cop0[rd] = (self.cop0[rd] & !0x300) | (value & 0x300);
                    }
                    cop0::PRID | cop0::BADVADDR | cop0::EPC => {}
                    _ => self.cop0[rd & 0xF] = value,
                }
            }
            0x10 if word & 0x3F == 0x10 => self.return_from_exception(),
            _ => self.exception(Exception::Reserved),
        }
    }

    fn cop2_op(&mut self, word: u32, rt: usize, rd: usize) -> Result<(), Halt> {
        if !self.cop_enabled(2) {
            self.coprocessor_unusable(2);
            return Ok(());
        }
        if word & (1 << 25) != 0 {
            return self.gte.command(word & 0x01FF_FFFF);
        }
        match (word >> 21) & 0x1F {
            0x00 => {
                let value = self.gte.read_data(rd);
                self.delayed(rt, value);
            }
            0x02 => {
                let value = self.gte.read_control(rd);
                self.delayed(rt, value);
            }
            0x04 => self.gte.write_data(rd, self.regs[rt]),
            0x06 => self.gte.write_control(rd, self.regs[rt]),
            _ => self.exception(Exception::Reserved),
        }
        Ok(())
    }
}
