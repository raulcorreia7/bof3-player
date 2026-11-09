// SPDX-License-Identifier: MIT
//! The kernel the machine stands in for the BIOS: the `A0`, `B0` and `C0` function tables, the
//! exception and interrupt dispatch, events, threads, the controller buffers and `Exec`. Each
//! function behaves as psx-spx "BIOS Function Summary", "BIOS Interrupt/Exception Handling",
//! "BIOS Event Functions", "BIOS Thread Functions", "BIOS Joypad Functions" and "BIOS Memory Map"
//! describe it. A function the machine lacks stops the run with [`Halt::Bios`].
//!
//! The kernel data the code may read lives in RAM where the BIOS keeps it: the function tables at
//! 0x200, 0x674 and 0x874, the table of tables at 0x100, the process control block and the thread
//! control blocks. Code addresses inside the kernel are traps the machine handles.

use crate::Halt;
use crate::bus::physical;
use crate::irq::Line;
use crate::machine::Machine;

/// The end of the kernel's 64 KB of RAM; the machine handles every instruction fetch below it.
pub(crate) const KERNEL_END: u32 = 0x1_0000;
const EXCEPTION_VECTOR: u32 = 0x80;
const RETURN_TRAP: u32 = 0x0F00;
const EXEC_RETURN_TRAP: u32 = 0x0F10;
/// The `A0`, `B0` and `C0` table entries point into these stub areas, one word per function.
const A0_STUBS: u32 = 0x2000;
const B0_STUBS: u32 = 0x3000;
const C0_STUBS: u32 = 0x4000;
/// `_patch_pad` (PsyQ) derives the pad enable and disable routines from the `ChangeClearPAD`
/// entry: entry + 0x884 and + 0x894 (psx-spx "BIOS Patches").
const PAD_ENABLE_TRAP: u32 = B0_STUBS + 4 * 0x5B + 0x884;
const PAD_DISABLE_TRAP: u32 = B0_STUBS + 4 * 0x5B + 0x894;
const TABLE_OF_TABLES: u32 = 0x100;
const PCB: u32 = 0x7000;
const TCBS: u32 = 0x7100;
const TCB_SIZE: u32 = 0xC0;
const TCB_COUNT: u32 = 8;
const KERNEL_STACK: u32 = 0x8000_E000;
/// Instructions a call into the code from the kernel may run before the machine gives up.
const GUEST_CALL_BUDGET: u64 = 50_000_000;

const TCB_REGS: u32 = 0x08;
const TCB_EPC: u32 = 0x88;
const TCB_HI: u32 = 0x8C;
const TCB_LO: u32 = 0x90;
const TCB_SR: u32 = 0x94;
const TCB_CAUSE: u32 = 0x98;

const EVENT_ENABLED: u32 = 0x2000;
const EVENT_READY: u32 = 0x4000;
const EVENT_DISABLED: u32 = 0x1000;
const EVENT_MODE_CALL: u32 = 0x1000;

/// One kernel event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    /// Event class, such as `0xF2000003` for the vertical blank counter.
    pub class: u32,
    /// Specifier bits.
    pub spec: u32,
    /// `0x1000` calls `handler`, `0x2000` only marks the event ready.
    pub mode: u32,
    /// Handler address for mode `0x1000`.
    pub handler: u32,
    /// `0x1000` disabled, `0x2000` enabled, `0x4000` ready.
    pub status: u32,
    open: bool,
}

/// State of the stand-in kernel.
#[derive(Clone, Debug)]
pub struct Kernel {
    events: Vec<Event>,
    hook: Option<u32>,
    chains: [u32; 4],
    clear_pad: bool,
    clear_counter: [bool; 4],
    pad_buffers: Option<(u32, u32)>,
    pad_word: Option<u32>,
    pad_started: bool,
    pad_enabled: bool,
    /// Buttons held on port 1, in the controller's bit order (bit 3 Start, bit 14 Cross), set bits
    /// pressed.
    pub buttons: u16,
    rand: u32,
    heap: (u32, u32),
    exec: Vec<ExecFrame>,
    /// Text the code printed through `printf`, `puts` and `putchar`.
    pub tty: String,
    /// Calls per function: (table, function) to count.
    pub calls: std::collections::BTreeMap<(u8, u8), u64>,
}

#[derive(Clone, Copy, Debug)]
struct ExecFrame {
    ra: u32,
    sp: u32,
    fp: u32,
    gp: u32,
}

impl Default for Kernel {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            hook: None,
            chains: [0; 4],
            clear_pad: true,
            clear_counter: [true; 4],
            pad_buffers: None,
            pad_word: None,
            pad_started: false,
            pad_enabled: true,
            buttons: 0,
            rand: 0,
            heap: (0, 0),
            exec: Vec::new(),
            tty: String::new(),
            calls: std::collections::BTreeMap::new(),
        }
    }
}

impl Kernel {
    /// The events the code opened, in handle order.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

/// The result of one kernel trap.
enum Flow {
    /// Return to the caller with `v0`.
    Return(u32),
    /// The registers and the program counter are already set.
    Done,
    /// Try the same call again later, as a blocking call does.
    Retry,
}

impl Machine {
    /// Writes the kernel's tables into RAM.
    pub(crate) fn install_kernel(&mut self) {
        for (table, stubs, count) in [(0x200, A0_STUBS, 0xC0), (0x874, B0_STUBS, 0x80), (0x674, C0_STUBS, 0x20)] {
            for index in 0..count {
                self.poke(table + 4 * index, 0x8000_0000 | (stubs + 4 * index));
            }
        }
        // Table of tables: exception chains, PCB, TCBs, EvCBs (address and size).
        self.poke(TABLE_OF_TABLES, 0x8000_0000 | 0x6000);
        self.poke(TABLE_OF_TABLES + 4, 0x20);
        self.poke(TABLE_OF_TABLES + 8, 0x8000_0000 | PCB);
        self.poke(TABLE_OF_TABLES + 12, 4);
        self.poke(TABLE_OF_TABLES + 16, 0x8000_0000 | TCBS);
        self.poke(TABLE_OF_TABLES + 20, TCB_COUNT * TCB_SIZE);
        self.poke(PCB, 0x8000_0000 | TCBS);
        for index in 0..TCB_COUNT {
            self.poke(TCBS + index * TCB_SIZE, if index == 0 { 0x4000 } else { 0x1000 });
        }
    }

    fn poke(&mut self, address: u32, value: u32) {
        let at = (physical(address) as usize) & 0x1F_FFFF;
        self.bus.ram[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn peek(&self, address: u32) -> u32 {
        let at = (physical(address) as usize) & 0x1F_FFFC;
        u32::from_le_bytes([self.bus.ram[at], self.bus.ram[at + 1], self.bus.ram[at + 2], self.bus.ram[at + 3]])
    }

    fn arg(&mut self, index: usize) -> u32 {
        if index < 4 {
            self.cpu.reg(4 + index)
        } else {
            let sp = self.cpu.reg(29);
            self.bus.read32(sp.wrapping_add(4 * index as u32))
        }
    }

    fn string(&mut self, address: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut at = address;
        while bytes.len() < 4096 {
            let byte = self.bus.read8(at);
            if byte == 0 {
                break;
            }
            bytes.push(byte);
            at = at.wrapping_add(1);
        }
        bytes
    }

    /// Handles an instruction fetch from kernel RAM at physical address `pc`.
    pub(crate) fn kernel_trap(&mut self, pc: u32) -> Result<(), Halt> {
        self.cpu.flush_load();
        let flow = match pc {
            EXCEPTION_VECTOR => return self.exception(),
            0xA0 | 0xB0 | 0xC0 => {
                let table = pc as u8;
                let function = self.cpu.reg(9) as u8;
                *self.kernel.calls.entry((table, function)).or_default() += 1;
                match table {
                    0xA0 => self.a0(function)?,
                    0xB0 => self.b0(function)?,
                    _ => self.c0(function)?,
                }
            }
            PAD_ENABLE_TRAP => {
                self.kernel.pad_enabled = true;
                Flow::Return(1)
            }
            PAD_DISABLE_TRAP => {
                self.kernel.pad_enabled = false;
                Flow::Return(1)
            }
            EXEC_RETURN_TRAP => {
                let Some(frame) = self.kernel.exec.pop() else {
                    return Err(Halt::Returned);
                };
                self.cpu.set_reg(29, frame.sp);
                self.cpu.set_reg(30, frame.fp);
                self.cpu.set_reg(28, frame.gp);
                self.cpu.set_reg(31, frame.ra);
                Flow::Return(1)
            }
            _ => {
                return Err(Halt::KernelPc { pc: self.cpu.pc(), caller: self.cpu.reg(31) });
            }
        };
        match flow {
            Flow::Return(value) => {
                self.cpu.set_reg(2, value);
                let ra = self.cpu.reg(31);
                self.cpu.set_pc(ra);
            }
            Flow::Done => {}
            Flow::Retry => {
                self.cpu.take_interrupt(&mut self.bus);
            }
        }
        Ok(())
    }

    fn unknown(&self, table: u8, function: u8) -> Halt {
        Halt::Bios { table, function, caller: self.cpu.reg(31) }
    }

    fn a0(&mut self, function: u8) -> Result<Flow, Halt> {
        let (a0, a1, a2) = (self.arg(0), self.arg(1), self.arg(2));
        Ok(match function {
            0x13 => {
                for (slot, reg) in [31, 29, 30, 16, 17, 18, 19, 20, 21, 22, 23, 28].into_iter().enumerate() {
                    let value = self.cpu.reg(reg);
                    self.bus.write32(a0.wrapping_add(4 * slot as u32), value);
                }
                Flow::Return(0)
            }
            0x14 => {
                self.long_jump(a0, a1);
                Flow::Done
            }
            0x15 => {
                let end = a0.wrapping_add(self.string(a0).len() as u32);
                let source = self.string(a1);
                self.store_string(end, &source);
                Flow::Return(a0)
            }
            0x17 | 0x18 => {
                let (left, right) = (self.string(a0), self.string(a1));
                let limit = if function == 0x18 { a2 as usize } else { usize::MAX };
                let left = &left[..left.len().min(limit)];
                let right = &right[..right.len().min(limit)];
                Flow::Return(match left.cmp(right) {
                    std::cmp::Ordering::Less => u32::MAX,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                })
            }
            0x19 => {
                let source = self.string(a1);
                self.store_string(a0, &source);
                Flow::Return(a0)
            }
            0x1A => {
                let mut source = self.string(a1);
                source.resize(a2 as usize, 0);
                for (offset, byte) in source.iter().enumerate() {
                    self.bus.write8(a0.wrapping_add(offset as u32), *byte);
                }
                Flow::Return(a0)
            }
            0x1B => Flow::Return(self.string(a0).len() as u32),
            0x2A => {
                for offset in 0..a2 {
                    let byte = self.bus.read8(a1.wrapping_add(offset));
                    self.bus.write8(a0.wrapping_add(offset), byte);
                }
                Flow::Return(a0)
            }
            0x2B => {
                for offset in 0..a2 {
                    self.bus.write8(a0.wrapping_add(offset), a1 as u8);
                }
                Flow::Return(a0)
            }
            0x2E => {
                let found = (0..a2).find(|&offset| self.bus.read8(a0.wrapping_add(offset)) == a1 as u8);
                Flow::Return(found.map_or(0, |offset| a0.wrapping_add(offset)))
            }
            0x2F => {
                self.kernel.rand = self.kernel.rand.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
                Flow::Return((self.kernel.rand >> 16) & 0x7FFF)
            }
            0x30 => {
                self.kernel.rand = a0;
                Flow::Return(0)
            }
            0x39 => {
                self.kernel.heap = (a0, a1);
                Flow::Return(0)
            }
            0x3C => {
                self.kernel.tty.push(char::from(a0 as u8));
                Flow::Return(a0)
            }
            0x3F => {
                let text = self.format(a0, 1);
                self.kernel.tty.push_str(&text);
                Flow::Return(text.len() as u32)
            }
            0x43 => self.exec(a0, a1, a2),
            0x44 | 0x70 | 0x71 | 0x72 => Flow::Return(1),
            0x49 => {
                self.bus.gpu.gp0(a0);
                Flow::Return(0)
            }
            0xAB | 0xAC => {
                // No memory card: the slot answers with a timeout.
                self.deliver(0xF400_0001, 0x0100)?;
                Flow::Return(1)
            }
            _ => return Err(self.unknown(0xA0, function)),
        })
    }

    fn b0(&mut self, function: u8) -> Result<Flow, Halt> {
        let (a0, a1, a2, a3) = (self.arg(0), self.arg(1), self.arg(2), self.arg(3));
        Ok(match function {
            0x07 => {
                self.deliver(a0, a1)?;
                Flow::Return(1)
            }
            0x08 => {
                let event = Event { class: a0, spec: a1, mode: a2, handler: a3, status: EVENT_DISABLED, open: true };
                let index = match self.kernel.events.iter().position(|event| !event.open) {
                    Some(index) => {
                        self.kernel.events[index] = event;
                        index
                    }
                    None => {
                        self.kernel.events.push(event);
                        self.kernel.events.len() - 1
                    }
                };
                Flow::Return(0xF100_0000 | index as u32)
            }
            0x09..=0x0D => {
                let Some(event) = self.kernel.events.get_mut((a0 & 0xFFFF) as usize) else {
                    return Ok(Flow::Return(0));
                };
                match function {
                    0x09 => event.open = false,
                    0x0A => {
                        if event.status == EVENT_READY {
                            event.status = EVENT_ENABLED;
                        } else if event.status == EVENT_ENABLED {
                            return Ok(Flow::Retry);
                        } else {
                            return Ok(Flow::Return(0));
                        }
                    }
                    0x0B => {
                        let ready = event.status == EVENT_READY;
                        if ready {
                            event.status = EVENT_ENABLED;
                        }
                        return Ok(Flow::Return(u32::from(ready)));
                    }
                    0x0C => event.status = EVENT_ENABLED,
                    _ => event.status = EVENT_DISABLED,
                }
                Flow::Return(1)
            }
            0x0E => {
                let free = (1..TCB_COUNT).find(|&index| self.peek(TCBS + index * TCB_SIZE) != 0x4000);
                match free {
                    Some(index) => {
                        let tcb = TCBS + index * TCB_SIZE;
                        self.poke(tcb, 0x4000);
                        self.poke(tcb + TCB_EPC, a0);
                        self.poke(tcb + TCB_REGS + 4 * 29, a1);
                        self.poke(tcb + TCB_REGS + 4 * 30, a1);
                        self.poke(tcb + TCB_REGS + 4 * 28, a2);
                        Flow::Return(0xFF00_0000 | index)
                    }
                    None => Flow::Return(u32::MAX),
                }
            }
            0x0F => {
                self.poke(TCBS + (a0 & 0xFFFF) * TCB_SIZE, 0x1000);
                Flow::Return(1)
            }
            0x10 => {
                self.change_thread(a0);
                Flow::Done
            }
            0x12 => {
                self.kernel.pad_buffers = Some((a0, a2));
                for (buffer, size) in [(a0, a1), (a2, a3)] {
                    for offset in 0..size.min(0x22) {
                        self.bus.write8(buffer.wrapping_add(offset), 0xFF);
                    }
                }
                Flow::Return(2)
            }
            0x13 => {
                self.kernel.pad_started = true;
                Flow::Return(1)
            }
            0x14 => {
                self.kernel.pad_started = false;
                Flow::Return(1)
            }
            0x15 => {
                self.kernel.pad_word = Some(a1);
                self.kernel.pad_started = true;
                Flow::Return(2)
            }
            0x16 => {
                let word = self.pad_word();
                if let Some(address) = self.kernel.pad_word {
                    self.bus.write32(address, word);
                }
                Flow::Return(word)
            }
            0x17 => {
                self.return_from_exception();
                Flow::Done
            }
            0x18 => {
                self.kernel.hook = None;
                Flow::Return(0)
            }
            0x19 => {
                self.kernel.hook = Some(a0);
                Flow::Return(0)
            }
            0x32..=0x36 => Flow::Return(u32::MAX),
            0x3D => {
                self.kernel.tty.push(char::from(a0 as u8));
                Flow::Return(a0)
            }
            0x3F => {
                let mut text = String::from_utf8_lossy(&self.string(a0)).into_owned();
                text.push('\n');
                self.kernel.tty.push_str(&text);
                Flow::Return(1)
            }
            0x41 => Flow::Return(0),
            0x42 | 0x43 => Flow::Return(0),
            0x4A..=0x4C => Flow::Return(1),
            0x4E | 0x50 => {
                self.deliver(0xF400_0001, 0x0100)?;
                Flow::Return(1)
            }
            0x56 => Flow::Return(0x8000_0674),
            0x57 => Flow::Return(0x8000_0874),
            0x5B => {
                self.kernel.clear_pad = a0 != 0;
                Flow::Return(1)
            }
            _ => return Err(self.unknown(0xB0, function)),
        })
    }

    fn c0(&mut self, function: u8) -> Result<Flow, Halt> {
        let (a0, a1) = (self.arg(0), self.arg(1));
        Ok(match function {
            0x02 => {
                let priority = (a0 & 3) as usize;
                let head = self.kernel.chains[priority];
                self.bus.write32(a1, head);
                self.kernel.chains[priority] = a1;
                Flow::Return(1)
            }
            0x03 => {
                let priority = (a0 & 3) as usize;
                let next = self.bus.read32(a1);
                if self.kernel.chains[priority] == a1 {
                    self.kernel.chains[priority] = next;
                } else {
                    let mut node = self.kernel.chains[priority];
                    for _ in 0..64 {
                        if node == 0 {
                            break;
                        }
                        let following = self.bus.read32(node);
                        if following == a1 {
                            self.bus.write32(node, next);
                            break;
                        }
                        node = following;
                    }
                }
                Flow::Return(1)
            }
            0x0A => {
                let index = (a0 & 3) as usize;
                let old = self.kernel.clear_counter[index];
                self.kernel.clear_counter[index] = a1 != 0;
                Flow::Return(u32::from(old))
            }
            _ => return Err(self.unknown(0xC0, function)),
        })
    }

    fn store_string(&mut self, address: u32, bytes: &[u8]) {
        for (offset, byte) in bytes.iter().chain(std::iter::once(&0)).enumerate() {
            self.bus.write8(address.wrapping_add(offset as u32), *byte);
        }
    }

    fn long_jump(&mut self, buffer: u32, value: u32) {
        for (slot, reg) in [31, 29, 30, 16, 17, 18, 19, 20, 21, 22, 23, 28].into_iter().enumerate() {
            let saved = self.bus.read32(buffer.wrapping_add(4 * slot as u32));
            self.cpu.set_reg(reg, saved);
        }
        self.cpu.set_reg(2, value);
        let ra = self.cpu.reg(31);
        self.cpu.set_pc(ra);
    }

    /// The PAD_dr word: both ports, the controller's two button bytes swapped, set bits released.
    fn pad_word(&self) -> u32 {
        let buttons = self.kernel.buttons;
        let swapped = buttons.rotate_left(8);
        0xFFFF_0000 | u32::from(!swapped)
    }

    fn read_pads(&mut self) {
        if !self.kernel.pad_started || !self.kernel.pad_enabled {
            return;
        }
        let released = !self.kernel.buttons;
        if let Some((first, second)) = self.kernel.pad_buffers {
            for (offset, byte) in [0x00, 0x41, released as u8, (released >> 8) as u8].into_iter().enumerate() {
                self.bus.write8(first.wrapping_add(offset as u32), byte);
            }
            self.bus.write8(second, 0xFF);
        }
        if let Some(address) = self.kernel.pad_word {
            let word = self.pad_word();
            self.bus.write32(address, word);
        }
    }

    fn deliver(&mut self, class: u32, spec: u32) -> Result<(), Halt> {
        for index in 0..self.kernel.events.len() {
            let event = self.kernel.events[index];
            if !event.open || event.class != class || event.spec & spec == 0 || event.status != EVENT_ENABLED {
                continue;
            }
            if event.mode == EVENT_MODE_CALL {
                if event.handler != 0 {
                    self.call_guest(event.handler, &[])?;
                }
            } else {
                self.kernel.events[index].status = EVENT_READY;
            }
        }
        Ok(())
    }

    fn exec(&mut self, header: u32, argc: u32, argv: u32) -> Flow {
        let word = |machine: &mut Self, index: u32| machine.bus.read32(header.wrapping_add(4 * index));
        let (pc, gp) = (word(self, 0), word(self, 1));
        let (bss, bss_size) = (word(self, 6), word(self, 7));
        let (stack, stack_size) = (word(self, 8), word(self, 9));
        for offset in (0..bss_size).step_by(4) {
            self.bus.write32(bss.wrapping_add(offset), 0);
        }
        self.kernel.exec.push(ExecFrame { ra: self.cpu.reg(31), sp: self.cpu.reg(29), fp: self.cpu.reg(30), gp: self.cpu.reg(28) });
        if stack != 0 {
            let top = stack.wrapping_add(stack_size);
            self.cpu.set_reg(29, top);
            self.cpu.set_reg(30, top);
        }
        self.cpu.set_reg(28, gp);
        self.cpu.set_reg(4, argc);
        self.cpu.set_reg(5, argv);
        self.cpu.set_reg(31, 0x8000_0000 | EXEC_RETURN_TRAP);
        self.cpu.set_pc(pc);
        Flow::Done
    }

    fn current_tcb(&self) -> u32 {
        self.peek(PCB) & 0x1F_FFFF
    }

    /// Saves the registers into the current thread control block, as the exception entry does.
    fn save_context(&mut self) {
        let tcb = self.current_tcb();
        for reg in 1..32 {
            let value = self.cpu.reg(reg);
            self.poke(tcb + TCB_REGS + 4 * reg as u32, value);
        }
        self.poke(tcb + TCB_EPC, self.cpu.cop0(crate::cpu::cop0::EPC));
        self.poke(tcb + TCB_HI, self.cpu.hi());
        self.poke(tcb + TCB_LO, self.cpu.lo());
        self.poke(tcb + TCB_SR, self.cpu.cop0(crate::cpu::cop0::SR));
        self.poke(tcb + TCB_CAUSE, self.cpu.cop0(crate::cpu::cop0::CAUSE));
    }

    /// `ReturnFromException`: the registers of the current thread, then `RFE` to its EPC.
    fn return_from_exception(&mut self) {
        let tcb = self.current_tcb();
        for reg in 1..32 {
            let value = self.peek(tcb + TCB_REGS + 4 * reg as u32);
            self.cpu.set_reg(reg, value);
        }
        self.cpu.set_hi_lo(self.peek(tcb + TCB_HI), self.peek(tcb + TCB_LO));
        self.cpu.set_cop0(crate::cpu::cop0::SR, self.peek(tcb + TCB_SR));
        self.cpu.return_from_exception();
        let epc = self.peek(tcb + TCB_EPC);
        self.cpu.set_pc(epc);
    }

    fn change_thread(&mut self, handle: u32) {
        let target = TCBS + (handle & 0xFFFF) * TCB_SIZE;
        if (handle & 0xFFFF) >= TCB_COUNT || self.peek(target) != 0x4000 {
            self.cpu.set_reg(2, 0);
            let ra = self.cpu.reg(31);
            self.cpu.set_pc(ra);
            return;
        }
        // The caller resumes after its call with v0 = 1, interrupts as they are now.
        let ra = self.cpu.reg(31);
        self.cpu.set_reg(2, 1);
        let sr = self.cpu.cop0(crate::cpu::cop0::SR);
        self.cpu.set_cop0(crate::cpu::cop0::SR, (sr & !0x3F) | ((sr << 2) & 0x3F));
        self.cpu.set_cop0(crate::cpu::cop0::EPC, ra);
        self.save_context();
        self.cpu.set_cop0(crate::cpu::cop0::SR, sr);
        self.poke(PCB, 0x8000_0000 | target);
        self.return_from_exception();
    }

    /// Calls the code at `function` with `args` and returns its `v0`, preserving the registers.
    pub(crate) fn call_guest(&mut self, function: u32, args: &[u32]) -> Result<u32, Halt> {
        let saved = self.cpu.clone();
        for (index, &value) in args.iter().enumerate().take(4) {
            self.cpu.set_reg(4 + index, value);
        }
        let sp = self.cpu.reg(29);
        let in_kernel_stack = physical(sp) < KERNEL_END;
        self.cpu.set_reg(29, if in_kernel_stack { sp.wrapping_sub(0x400) } else { KERNEL_STACK });
        self.cpu.set_reg(31, 0x8000_0000 | RETURN_TRAP);
        self.cpu.set_pc(function);
        let mut budget = GUEST_CALL_BUDGET;
        while physical(self.cpu.pc()) != RETURN_TRAP {
            if budget == 0 {
                return Err(Halt::GuestCall { function });
            }
            budget -= 1;
            self.tick()?;
        }
        let result = self.cpu.reg(2);
        self.cpu = saved;
        Ok(result)
    }

    fn exception(&mut self) -> Result<(), Halt> {
        let cause = self.cpu.cop0(crate::cpu::cop0::CAUSE);
        let epc = self.cpu.cop0(crate::cpu::cop0::EPC);
        match (cause >> 2) & 0x1F {
            8 => {
                let sr = self.cpu.cop0(crate::cpu::cop0::SR);
                let value = match self.cpu.reg(4) {
                    1 => {
                        self.cpu.set_cop0(crate::cpu::cop0::SR, sr & !0x404);
                        u32::from(sr & 0x404 == 0x404)
                    }
                    2 => {
                        self.cpu.set_cop0(crate::cpu::cop0::SR, sr | 0x404);
                        self.cpu.reg(2)
                    }
                    _ => self.cpu.reg(2),
                };
                self.cpu.set_reg(2, value);
                self.cpu.return_from_exception();
                self.cpu.set_pc(epc.wrapping_add(4));
                Ok(())
            }
            0 => self.interrupt(),
            code => Err(Halt::Exception { code: code as u8, epc, address: self.cpu.cop0(crate::cpu::cop0::BADVADDR) }),
        }
    }

    fn interrupt(&mut self) -> Result<(), Halt> {
        self.save_context();
        let status = self.bus.irq.status();
        if status & Line::VBlank as u16 != 0 {
            self.read_pads();
            self.deliver(0xF200_0003, 0x0002)?;
            if self.kernel.clear_pad {
                self.bus.irq.acknowledge(!(Line::VBlank as u16));
            }
        }
        for counter in 0..3 {
            let line = (Line::Timer0 as u16) << counter;
            if status & line != 0 {
                self.deliver(0xF200_0000 + counter, 0x0002)?;
                if self.kernel.clear_counter[counter as usize] {
                    self.bus.irq.acknowledge(!line);
                }
            }
        }
        for priority in 0..4 {
            let mut node = self.kernel.chains[priority];
            for _ in 0..64 {
                if node == 0 {
                    break;
                }
                let next = self.bus.read32(node);
                let second = self.bus.read32(node.wrapping_add(4));
                let first = self.bus.read32(node.wrapping_add(8));
                if first != 0 {
                    let result = self.call_guest(first, &[])?;
                    if result != 0 && second != 0 {
                        self.call_guest(second, &[result])?;
                    }
                }
                node = next;
            }
        }
        match self.kernel.hook {
            Some(buffer) => self.long_jump(buffer, 1),
            None => self.return_from_exception(),
        }
        Ok(())
    }

    /// Formats a `printf` string whose first argument is argument `first`.
    fn format(&mut self, format: u32, first: usize) -> String {
        let pattern = self.string(format);
        let mut out = String::new();
        let mut next = first;
        let mut index = 0;
        while index < pattern.len() {
            let byte = pattern[index];
            index += 1;
            if byte != b'%' {
                out.push(char::from(byte));
                continue;
            }
            let mut spec = String::from("%");
            while index < pattern.len() && b"-+ #0123456789.lh".contains(&pattern[index]) {
                spec.push(char::from(pattern[index]));
                index += 1;
            }
            let Some(&kind) = pattern.get(index) else { break };
            index += 1;
            let zero = spec.contains('0') && !spec.contains('-');
            let width: usize = spec.trim_start_matches(['%', '-', '+', ' ', '#', '0']).split(['.', 'l', 'h']).next().and_then(|w| w.parse().ok()).unwrap_or(0);
            let text = match kind {
                b'%' => "%".to_string(),
                b'd' | b'i' => {
                    let value = self.arg(next) as i32;
                    next += 1;
                    value.to_string()
                }
                b'u' => {
                    let value = self.arg(next);
                    next += 1;
                    value.to_string()
                }
                b'x' | b'p' => {
                    let value = self.arg(next);
                    next += 1;
                    format!("{value:x}")
                }
                b'X' => {
                    let value = self.arg(next);
                    next += 1;
                    format!("{value:X}")
                }
                b'c' => {
                    let value = self.arg(next);
                    next += 1;
                    char::from(value as u8).to_string()
                }
                b's' => {
                    let address = self.arg(next);
                    next += 1;
                    String::from_utf8_lossy(&self.string(address)).into_owned()
                }
                other => format!("{spec}{}", char::from(other)),
            };
            let pad = width.saturating_sub(text.len());
            if spec.contains('-') {
                out.push_str(&text);
                out.extend(std::iter::repeat_n(' ', pad));
            } else {
                out.extend(std::iter::repeat_n(if zero { '0' } else { ' ' }, pad));
                out.push_str(&text);
            }
        }
        out
    }
}
