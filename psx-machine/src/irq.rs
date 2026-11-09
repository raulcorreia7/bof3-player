// SPDX-License-Identifier: MIT
//! Interrupt control: `I_STAT` and `I_MASK` at 0x1F801070 (psx-spx "Interrupts").

/// One interrupt line of `I_STAT`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Line {
    /// Vertical blank.
    VBlank = 1 << 0,
    /// GPU `GP0(1Fh)`.
    Gpu = 1 << 1,
    /// CD-ROM controller.
    Cdrom = 1 << 2,
    /// DMA controller.
    Dma = 1 << 3,
    /// Timer 0.
    Timer0 = 1 << 4,
    /// Timer 1.
    Timer1 = 1 << 5,
    /// Timer 2.
    Timer2 = 1 << 6,
    /// Controller and memory card port.
    Pad = 1 << 7,
    /// Serial port.
    Sio = 1 << 8,
    /// SPU.
    Spu = 1 << 9,
}

/// The interrupt controller.
#[derive(Clone, Debug, Default)]
pub struct Irq {
    status: u16,
    mask: u16,
}

impl Irq {
    /// Sets `line` in `I_STAT`.
    pub fn raise(&mut self, line: Line) {
        self.status |= line as u16;
    }

    /// `I_STAT`.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// `I_MASK`.
    #[must_use]
    pub fn mask(&self) -> u16 {
        self.mask
    }

    /// A write to `I_STAT`: a 0 bit acknowledges that line.
    pub fn acknowledge(&mut self, value: u16) {
        self.status &= value;
    }

    /// A write to `I_MASK`.
    pub fn set_mask(&mut self, value: u16) {
        self.mask = value & 0x07FF;
    }

    /// Whether an unmasked line is raised, which drives `CAUSE` bit 10.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.status & self.mask != 0
    }
}
