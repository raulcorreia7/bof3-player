// SPDX-License-Identifier: MIT
//! The machine: the CPU, the bus, the stand-in kernel and the run loop.

use std::path::Path;

use crate::bios::{KERNEL_END, Kernel};
use crate::bus::{Bus, physical};
use crate::cpu::{Cpu, cop0};
use crate::disc::{Disc, ImageDisc};
use crate::exe::{Executable, SystemConfig};
use crate::{Error, Halt};

/// Cycles one instruction costs. The R3000A runs most instructions in one cycle from the
/// instruction cache, but loads and uncached fetches stall; two is a common average.
const CYCLES_PER_INSTRUCTION: u64 = 2;
/// Cycles between device updates.
const SYNC_INTERVAL: u64 = 64;
/// The stack when neither the executable nor `SYSTEM.CNF` names one.
const DEFAULT_STACK: u32 = 0x801F_FF00;

/// Where a run stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    /// The program counter reached the requested address.
    Pc,
    /// The requested number of frames ran.
    Frames,
    /// The condition held.
    Condition,
    /// The instruction budget ran out first.
    Budget,
}

/// A PlayStation: CPU, memory, devices and, once booted, the stand-in kernel.
pub struct Machine {
    pub(crate) cpu: Cpu,
    pub(crate) bus: Bus,
    pub(crate) kernel: Kernel,
    kernel_enabled: bool,
    next_sync: u64,
    instructions: u64,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    /// A machine at power-on with cleared memory and no kernel: the CPU starts at the empty BIOS.
    #[must_use]
    pub fn new() -> Self {
        Self { cpu: Cpu::new(), bus: Bus::new(), kernel: Kernel::default(), kernel_enabled: false, next_sync: SYNC_INTERVAL, instructions: 0 }
    }

    /// A machine with the stand-in kernel installed, ready to load an executable.
    #[must_use]
    pub fn with_kernel() -> Self {
        let mut machine = Self::new();
        machine.kernel_enabled = true;
        machine.install_kernel();
        // The BIOS hands over with its own exception vector in RAM, not the boot vector.
        machine.cpu.set_cop0(cop0::SR, 0);
        machine
    }

    /// Opens the disc image at `path` (CUE sheet, raw `.bin` or `.iso`), puts it in the drive,
    /// and loads the executable `SYSTEM.CNF` names, ready to run from its entry point.
    ///
    /// # Errors
    ///
    /// [`Error::Disc`] when the image cannot be read, [`Error::Boot`] when `SYSTEM.CNF` or the
    /// executable is missing or malformed.
    pub fn boot_image(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut disc = ImageDisc::open(path)?;
        let image = disc.image_mut();
        let entries = image.entries()?;
        let find = |name: &str| entries.iter().find(|entry| entry.path.to_string_lossy().eq_ignore_ascii_case(name)).cloned();
        let config = match find("SYSTEM.CNF") {
            Some(entry) => SystemConfig::parse(&String::from_utf8_lossy(&image.read_file(&entry)?)),
            None => return Err(Error::Boot("the disc has no SYSTEM.CNF".into())),
        };
        let entry = find(&config.boot).ok_or_else(|| Error::Boot(format!("boot file {} is not on the disc", config.boot)))?;
        let exe = image.read_file(&entry)?;
        let mut machine = Self::with_kernel();
        machine.load_executable(&exe, config.stack)?;
        machine.bus.cdrom.insert(Box::new(disc));
        Ok(machine)
    }

    /// Puts `disc` in the CD-ROM drive.
    pub fn insert_disc(&mut self, disc: Box<dyn Disc>) {
        self.bus.cdrom.insert(disc);
    }

    /// Loads a PS-X EXE and points the CPU at its entry, as the BIOS does after boot: the stack
    /// from the header, else `stack`, else 0x801FFF00.
    ///
    /// # Errors
    ///
    /// [`Error::Boot`] when `bytes` is not a PS-X EXE.
    pub fn load_executable(&mut self, bytes: &[u8], stack: Option<u32>) -> Result<(), Error> {
        let exe = Executable::parse(bytes).map_err(|message| Error::Boot(message.into()))?;
        self.write_ram(exe.load_address, exe.body);
        for offset in (0..exe.bss_size).step_by(4) {
            self.bus.write32(exe.bss.wrapping_add(offset), 0);
        }
        let sp = if exe.stack != 0 { exe.stack.wrapping_add(exe.stack_size) } else { stack.unwrap_or(DEFAULT_STACK) };
        self.cpu.set_reg(29, sp);
        self.cpu.set_reg(30, sp);
        self.cpu.set_reg(28, exe.gp);
        self.cpu.set_reg(31, 0x8000_0F10);
        self.cpu.set_cop0(cop0::SR, 0x0000_0401);
        self.cpu.set_pc(exe.pc);
        Ok(())
    }

    /// The CPU.
    #[must_use]
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// The CPU, writable.
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// The bus and the devices on it.
    #[must_use]
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// The bus, writable.
    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.bus
    }

    /// The stand-in kernel.
    #[must_use]
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    /// The stand-in kernel, writable: controller buttons live here.
    pub fn kernel_mut(&mut self) -> &mut Kernel {
        &mut self.kernel
    }

    /// Main RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        self.bus.ram()
    }

    /// VRAM, 1024×512 16-bit pixels, row-major.
    #[must_use]
    pub fn vram(&self) -> &[u16] {
        self.bus.gpu.vram()
    }

    /// The picture on the screen: the display area of VRAM as RGBA.
    #[must_use]
    pub fn display(&self) -> crate::gpu::Frame {
        self.bus.gpu.display()
    }

    /// Turns rasterizing draw commands into VRAM on (the default) or off; the GPU log records
    /// every command either way.
    pub fn set_rasterize(&mut self, on: bool) {
        self.bus.gpu.set_rasterize(on);
    }

    /// The GPU command log.
    #[must_use]
    pub fn gpu_log(&self) -> &[crate::gpu::Logged] {
        self.bus.gpu.log()
    }

    /// The SPU key-on log.
    #[must_use]
    pub fn key_ons(&self) -> &[crate::spu::KeyOn] {
        self.bus.spu.key_ons()
    }

    /// Vertical blanks since power-on.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.bus.timers.frame
    }

    /// Instructions run since power-on, kernel calls counted as one.
    #[must_use]
    pub fn instructions(&self) -> u64 {
        self.instructions
    }

    /// Copies `bytes` into RAM at `address` (any segment), wrapping at the 2 MB mirror.
    pub fn write_ram(&mut self, address: u32, bytes: &[u8]) {
        let ram = self.bus.ram_mut();
        let base = physical(address) as usize;
        for (offset, byte) in bytes.iter().enumerate() {
            ram[(base + offset) & (ram.len() - 1)] = *byte;
        }
    }

    /// Copies `words` into RAM at `address`, little-endian.
    pub fn write_words(&mut self, address: u32, words: &[u32]) {
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        self.write_ram(address, &bytes);
    }

    /// Runs one instruction, or one kernel call.
    ///
    /// # Errors
    ///
    /// The [`Halt`] that stopped the machine.
    pub fn step(&mut self) -> Result<(), Halt> {
        self.tick()
    }

    pub(crate) fn tick(&mut self) -> Result<(), Halt> {
        let pc = physical(self.cpu.pc());
        if self.kernel_enabled && pc < KERNEL_END {
            self.kernel_trap(pc)?;
        } else {
            self.cpu.step(&mut self.bus)?;
        }
        self.instructions += 1;
        self.bus.cycles += CYCLES_PER_INSTRUCTION;
        if self.bus.cycles >= self.next_sync {
            self.next_sync = self.bus.cycles + SYNC_INTERVAL;
            self.bus.sync();
        }
        Ok(())
    }

    /// Runs until the next instruction is at `pc`, or `budget` instructions have run.
    ///
    /// # Errors
    ///
    /// The [`Halt`] that stopped the machine.
    pub fn run_until_pc(&mut self, pc: u32, budget: u64) -> Result<Stop, Halt> {
        for _ in 0..budget {
            if self.cpu.pc() == pc {
                return Ok(Stop::Pc);
            }
            self.tick()?;
        }
        Ok(if self.cpu.pc() == pc { Stop::Pc } else { Stop::Budget })
    }

    /// Runs `frames` vertical blanks, or until `budget` instructions have run.
    ///
    /// # Errors
    ///
    /// The [`Halt`] that stopped the machine.
    pub fn run_frames(&mut self, frames: u64, budget: u64) -> Result<Stop, Halt> {
        let target = self.frame() + frames;
        self.run_until(|machine| machine.frame() >= target, budget).map(|stop| match stop {
            Stop::Condition => Stop::Frames,
            other => other,
        })
    }

    /// Runs until `condition` holds before an instruction, or `budget` instructions have run.
    ///
    /// # Errors
    ///
    /// The [`Halt`] that stopped the machine.
    pub fn run_until(&mut self, mut condition: impl FnMut(&Machine) -> bool, budget: u64) -> Result<Stop, Halt> {
        for _ in 0..budget {
            if condition(self) {
                return Ok(Stop::Condition);
            }
            self.tick()?;
        }
        Ok(if condition(self) { Stop::Condition } else { Stop::Budget })
    }
}
