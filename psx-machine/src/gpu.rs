// SPDX-License-Identifier: MIT
//! The GPU as the code sees it: the `GP0` and `GP1` ports, `GPUSTAT`, a 1024×512 16-bit VRAM and a
//! log of every command. Transfers and fills change VRAM; draw commands are recorded, not
//! rasterized. Command formats follow psx-spx "GPU Render Polygon/Line/Rectangle Commands",
//! "GPU Memory Transfer Commands", "GPU Rendering Attributes" and "GPU Display Control Commands".

/// VRAM width in 16-bit pixels.
pub const VRAM_WIDTH: usize = 1024;
/// VRAM height in lines.
pub const VRAM_HEIGHT: usize = 512;

/// Colour depth of a texture page.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Depth {
    /// 4-bit indices into a 16-entry CLUT.
    Bits4,
    /// 8-bit indices into a 256-entry CLUT.
    Bits8,
    /// Direct 15-bit colour (the reserved value 3 acts as 15-bit too).
    Bits15,
}

/// A texture page: the 256×256 texel area a textured primitive samples, and its blend mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TexturePage {
    /// Left edge in VRAM pixels (a multiple of 64).
    pub x: u16,
    /// Top edge in VRAM lines (0 or 256).
    pub y: u16,
    /// Texel depth.
    pub depth: Depth,
    /// Semi-transparency mode, 0 to 3.
    pub blend: u8,
}

impl TexturePage {
    /// Decodes the page bits of `GP0(E1h)` or of a polygon's page attribute.
    #[must_use]
    pub fn from_bits(bits: u32) -> Self {
        Self {
            x: ((bits & 0xF) * 64) as u16,
            y: (((bits >> 4) & 1) * 256) as u16,
            depth: match (bits >> 7) & 3 {
                0 => Depth::Bits4,
                1 => Depth::Bits8,
                _ => Depth::Bits15,
            },
            blend: ((bits >> 5) & 3) as u8,
        }
    }
}

/// Where a CLUT sits in VRAM.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Clut {
    /// Left edge in VRAM pixels (a multiple of 16).
    pub x: u16,
    /// Line.
    pub y: u16,
}

impl Clut {
    /// Decodes a CLUT attribute.
    #[must_use]
    pub fn from_bits(bits: u32) -> Self {
        Self { x: ((bits & 0x3F) * 16) as u16, y: ((bits >> 6) & 0x1FF) as u16 }
    }
}

/// `GP0(E2h)`: texture coordinates become `(uv & !(mask * 8)) | ((offset & mask) * 8)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct TextureWindow {
    /// Horizontal mask in 8-texel steps.
    pub mask_x: u8,
    /// Vertical mask in 8-texel steps.
    pub mask_y: u8,
    /// Horizontal offset in 8-texel steps.
    pub offset_x: u8,
    /// Vertical offset in 8-texel steps.
    pub offset_y: u8,
}

/// Everything a textured primitive samples with.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Texture {
    /// The page.
    pub page: TexturePage,
    /// The CLUT; unused by 15-bit pages.
    pub clut: Clut,
    /// The texture window in force.
    pub window: TextureWindow,
    /// Texels are drawn as they are, without the colour modulating them.
    pub raw: bool,
    /// A rectangle is mirrored horizontally (`GP0(E1h)` bit 12).
    pub flip_x: bool,
    /// A rectangle is mirrored vertically (`GP0(E1h)` bit 13).
    pub flip_y: bool,
}

/// One vertex of a draw command, in drawing-area coordinates before the drawing offset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Vertex {
    /// X, signed 11 bits.
    pub x: i16,
    /// Y, signed 11 bits.
    pub y: i16,
    /// 24-bit colour, `0xBBGGRR`.
    pub color: u32,
    /// Texture U.
    pub u: u8,
    /// Texture V.
    pub v: u8,
}

/// The form of a draw command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Shape {
    /// Three vertices.
    Triangle,
    /// Four vertices, drawn as two triangles (1-2-3 and 2-3-4).
    Quad,
    /// A line or a polyline, one segment per pair of neighbouring vertices.
    Line,
    /// A rectangle from its top-left vertex.
    Rect {
        /// Width in pixels.
        width: u16,
        /// Height in pixels.
        height: u16,
    },
}

/// A rectangle of VRAM.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct VramRect {
    /// Left edge.
    pub x: u16,
    /// Top line.
    pub y: u16,
    /// Width in pixels.
    pub width: u16,
    /// Height in lines.
    pub height: u16,
}

/// `GP0(E3h)` and `GP0(E4h)`: the inclusive drawing area.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct DrawArea {
    /// Left edge.
    pub left: u16,
    /// Top line.
    pub top: u16,
    /// Right edge, inclusive.
    pub right: u16,
    /// Bottom line, inclusive.
    pub bottom: u16,
}

/// A polygon, line or rectangle the code asked for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Draw {
    /// The `GP0` command byte.
    pub opcode: u8,
    /// The form.
    pub shape: Shape,
    /// The vertices, in command order.
    pub vertices: Vec<Vertex>,
    /// The texture, for a textured command.
    pub texture: Option<Texture>,
    /// Semi-transparent.
    pub semi_transparent: bool,
    /// Gouraud-shaded: each vertex carries its own colour.
    pub gouraud: bool,
    /// The drawing offset `GP0(E5h)` adds to every vertex.
    pub offset: (i16, i16),
    /// The drawing area that clips the command.
    pub area: DrawArea,
}

/// One entry of the command log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// A draw command.
    Draw(Draw),
    /// `GP0(02h)`: fill a rectangle with a colour (`0xBBGGRR`).
    Fill {
        /// The rectangle, after the hardware's rounding.
        rect: VramRect,
        /// The colour.
        color: u32,
    },
    /// `GP0(A0h)`: the CPU wrote these pixels to VRAM, row by row.
    Upload {
        /// The rectangle.
        rect: VramRect,
        /// The pixels as sent, before the mask bit settings applied.
        pixels: Vec<u16>,
    },
    /// `GP0(C0h)`: the CPU read a rectangle of VRAM.
    Download {
        /// The rectangle.
        rect: VramRect,
    },
    /// `GP0(80h)`: a rectangle copied inside VRAM.
    Copy {
        /// Source left edge.
        from_x: u16,
        /// Source top line.
        from_y: u16,
        /// The destination rectangle.
        to: VramRect,
    },
    /// A `GP1` display-control command (all but the `GP1(10h)` queries).
    Display {
        /// The command byte.
        command: u8,
        /// The 24-bit parameter.
        value: u32,
    },
}

/// A log entry with the frame it happened in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Logged {
    /// Vertical blanks counted before the command.
    pub frame: u64,
    /// The command.
    pub command: Command,
}

#[derive(Clone, Debug)]
enum Transfer {
    None,
    Upload { rect: VramRect, pixels: Vec<u16> },
    Download { rect: VramRect, index: usize },
}

/// The GPU state.
pub struct Gpu {
    vram: Vec<u16>,
    log: Vec<Logged>,
    /// The frame number stamped on new log entries.
    pub frame: u64,
    buffer: Vec<u32>,
    expected: usize,
    polyline: bool,
    transfer: Transfer,
    draw_mode: u32,
    window: TextureWindow,
    area: DrawArea,
    offset: (i16, i16),
    set_mask: bool,
    check_mask: bool,
    display_disabled: bool,
    dma_direction: u32,
    display_mode: u32,
    irq: bool,
    field: bool,
    read: u32,
    /// The display start, `GP1(05h)`.
    pub display_start: (u16, u16),
    raised_irq: bool,
}

impl Default for Gpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Gpu {
    /// A GPU after `GP1(00h)`, with cleared VRAM.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vram: vec![0; VRAM_WIDTH * VRAM_HEIGHT],
            log: Vec::new(),
            frame: 0,
            buffer: Vec::with_capacity(16),
            expected: 0,
            polyline: false,
            transfer: Transfer::None,
            draw_mode: 0,
            window: TextureWindow::default(),
            area: DrawArea::default(),
            offset: (0, 0),
            set_mask: false,
            check_mask: false,
            display_disabled: true,
            dma_direction: 0,
            display_mode: 0,
            irq: false,
            field: false,
            read: 0,
            display_start: (0, 0),
            raised_irq: false,
        }
    }

    /// VRAM, row-major, 1024 pixels per line.
    #[must_use]
    pub fn vram(&self) -> &[u16] {
        &self.vram
    }

    /// One VRAM pixel; coordinates wrap.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> u16 {
        self.vram[(y % VRAM_HEIGHT) * VRAM_WIDTH + (x % VRAM_WIDTH)]
    }

    /// The command log since the last [`Gpu::take_log`].
    #[must_use]
    pub fn log(&self) -> &[Logged] {
        &self.log
    }

    /// Returns the log and starts a new one.
    pub fn take_log(&mut self) -> Vec<Logged> {
        std::mem::take(&mut self.log)
    }

    /// Whether `GP0(1Fh)` raised the interrupt since the last call.
    pub(crate) fn take_irq(&mut self) -> bool {
        std::mem::take(&mut self.raised_irq)
    }

    /// Flips the interlace field, once per vertical blank.
    pub(crate) fn vblank(&mut self) {
        self.field = !self.field;
    }

    /// `GPUSTAT`.
    #[must_use]
    pub fn status(&self) -> u32 {
        let download = matches!(self.transfer, Transfer::Download { .. });
        let mut stat = self.draw_mode & 0x7FF;
        stat |= u32::from(self.set_mask) << 11;
        stat |= u32::from(self.check_mask) << 12;
        stat |= ((self.draw_mode >> 11) & 1) << 15;
        stat |= (self.display_mode & 0x3F) << 17;
        stat |= ((self.display_mode >> 6) & 1) << 16;
        stat |= ((self.display_mode >> 7) & 1) << 14;
        stat |= u32::from(self.display_disabled) << 23;
        stat |= u32::from(self.irq) << 24;
        stat |= 1 << 26; // ready for a command word
        stat |= u32::from(download) << 27;
        stat |= 1 << 28; // ready for a DMA block
        stat |= self.dma_direction << 29;
        let request = match self.dma_direction {
            0 => 0,
            1 | 2 => 1,
            _ => u32::from(download),
        };
        stat |= request << 25;
        // Interlaced 480-line mode shows the field; otherwise the bit alternates too.
        stat |= u32::from(self.field) << 31;
        stat |= u32::from(self.field && self.display_mode & 0x24 == 0x24) << 13;
        stat
    }

    /// `GPUREAD`: the next two pixels of a download, or the last `GP1(10h)` answer.
    pub fn read(&mut self) -> u32 {
        if let Transfer::Download { rect, index } = &mut self.transfer {
            let total = usize::from(rect.width) * usize::from(rect.height);
            let mut word = 0;
            for half in 0..2 {
                if *index < total {
                    let x = usize::from(rect.x) + *index % usize::from(rect.width);
                    let y = usize::from(rect.y) + *index / usize::from(rect.width);
                    word |= u32::from(self.vram[(y % VRAM_HEIGHT) * VRAM_WIDTH + (x % VRAM_WIDTH)]) << (16 * half);
                    *index += 1;
                }
            }
            if *index >= total {
                self.transfer = Transfer::None;
            }
            self.read = word;
        }
        self.read
    }

    /// One word written to `GP0`.
    pub fn gp0(&mut self, word: u32) {
        if let Transfer::Upload { rect, pixels } = &mut self.transfer {
            let total = usize::from(rect.width) * usize::from(rect.height);
            for half in [word as u16, (word >> 16) as u16] {
                if pixels.len() < total {
                    pixels.push(half);
                }
            }
            if pixels.len() >= total
                && let Transfer::Upload { rect, pixels } = std::mem::replace(&mut self.transfer, Transfer::None)
            {
                self.finish_upload(rect, pixels);
            }
            return;
        }
        if self.polyline {
            self.buffer.push(word);
            // The terminator can stand where a vertex, or with Gouraud shading a colour, is due.
            let minimum = if self.buffer[0] & 0x1000_0000 != 0 { 4 } else { 3 };
            if self.buffer.len() > minimum && word & 0xF000_F000 == 0x5000_5000 {
                self.buffer.pop();
                self.polyline = false;
                let words = std::mem::take(&mut self.buffer);
                self.line(&words);
            }
            return;
        }
        self.buffer.push(word);
        if self.buffer.len() == 1 {
            let opcode = word >> 24;
            if (0x48..0x50).contains(&opcode) || (0x58..0x60).contains(&opcode) {
                self.polyline = true;
                return;
            }
            self.expected = command_words(opcode);
        }
        if self.buffer.len() >= self.expected {
            let words = std::mem::take(&mut self.buffer);
            self.execute(&words);
        }
    }

    /// One word written to `GP1`.
    pub fn gp1(&mut self, word: u32) {
        let command = (word >> 24) & 0x3F;
        let value = word & 0x00FF_FFFF;
        match command {
            0x00 => {
                self.buffer.clear();
                self.polyline = false;
                self.transfer = Transfer::None;
                self.draw_mode = 0;
                self.window = TextureWindow::default();
                self.area = DrawArea::default();
                self.offset = (0, 0);
                self.set_mask = false;
                self.check_mask = false;
                self.display_disabled = true;
                self.dma_direction = 0;
                self.display_mode = 0;
                self.irq = false;
                self.display_start = (0, 0);
            }
            0x01 => {
                self.buffer.clear();
                self.polyline = false;
                self.transfer = Transfer::None;
            }
            0x02 => self.irq = false,
            0x03 => self.display_disabled = value & 1 != 0,
            0x04 => self.dma_direction = value & 3,
            0x05 => self.display_start = ((value & 0x3FE) as u16, ((value >> 10) & 0x1FF) as u16),
            0x08 => self.display_mode = value & 0xFF,
            0x10..=0x1F => {
                self.read = match value & 7 {
                    2 => {
                        let w = self.window;
                        u32::from(w.mask_x)
                            | u32::from(w.mask_y) << 5
                            | u32::from(w.offset_x) << 10
                            | u32::from(w.offset_y) << 15
                    }
                    3 => u32::from(self.area.left) | u32::from(self.area.top) << 10,
                    4 => u32::from(self.area.right) | u32::from(self.area.bottom) << 10,
                    5 => (self.offset.0 as u32 & 0x7FF) | (self.offset.1 as u32 & 0x7FF) << 11,
                    7 => 2,
                    _ => self.read,
                };
                return;
            }
            _ => {}
        }
        self.record(Command::Display { command: command as u8, value });
    }

    fn record(&mut self, command: Command) {
        self.log.push(Logged { frame: self.frame, command });
    }

    fn execute(&mut self, words: &[u32]) {
        let opcode = words[0] >> 24;
        match opcode {
            0x02 => self.fill(words),
            0x1F => {
                self.irq = true;
                self.raised_irq = true;
            }
            0x20..=0x3F => self.polygon(words),
            0x40..=0x5F => self.line(words),
            0x60..=0x7F => self.rect(words),
            0x80..=0x9F => self.copy(words),
            0xA0..=0xBF => {
                let rect = transfer_rect(words[1], words[2]);
                let total = usize::from(rect.width) * usize::from(rect.height);
                self.transfer = Transfer::Upload { rect, pixels: Vec::with_capacity(total) };
            }
            0xC0..=0xDF => {
                let rect = transfer_rect(words[1], words[2]);
                self.transfer = Transfer::Download { rect, index: 0 };
                self.record(Command::Download { rect });
            }
            0xE1 => self.draw_mode = words[0] & 0x3FFF,
            0xE2 => {
                let w = words[0];
                self.window = TextureWindow {
                    mask_x: (w & 0x1F) as u8,
                    mask_y: ((w >> 5) & 0x1F) as u8,
                    offset_x: ((w >> 10) & 0x1F) as u8,
                    offset_y: ((w >> 15) & 0x1F) as u8,
                };
            }
            0xE3 => {
                self.area.left = (words[0] & 0x3FF) as u16;
                self.area.top = ((words[0] >> 10) & 0x1FF) as u16;
            }
            0xE4 => {
                self.area.right = (words[0] & 0x3FF) as u16;
                self.area.bottom = ((words[0] >> 10) & 0x1FF) as u16;
            }
            0xE5 => self.offset = (sign11(words[0]), sign11(words[0] >> 11)),
            0xE6 => {
                self.set_mask = words[0] & 1 != 0;
                self.check_mask = words[0] & 2 != 0;
            }
            _ => {}
        }
    }

    fn draw(&mut self, opcode: u32, shape: Shape, vertices: Vec<Vertex>, texture: Option<Texture>) {
        let draw = Draw {
            opcode: opcode as u8,
            shape,
            vertices,
            texture,
            semi_transparent: opcode & 0x02 != 0,
            gouraud: opcode & 0x10 != 0 && opcode < 0x60,
            offset: self.offset,
            area: self.area,
        };
        self.record(Command::Draw(draw));
    }

    fn polygon(&mut self, words: &[u32]) {
        let opcode = words[0] >> 24;
        let gouraud = opcode & 0x10 != 0;
        let count = if opcode & 0x08 != 0 { 4 } else { 3 };
        let textured = opcode & 0x04 != 0;
        let mut vertices = Vec::with_capacity(count);
        let mut cursor = 0;
        let mut clut_bits = 0;
        let mut page_bits = self.draw_mode;
        for index in 0..count {
            let color = if index == 0 || gouraud {
                let color = words[cursor] & 0x00FF_FFFF;
                cursor += 1;
                color
            } else {
                vertices.first().map_or(0, |vertex: &Vertex| vertex.color)
            };
            let position = words[cursor];
            cursor += 1;
            let mut vertex = Vertex { x: sign11(position), y: sign11(position >> 16), color, u: 0, v: 0 };
            if textured {
                let uv = words[cursor];
                cursor += 1;
                vertex.u = uv as u8;
                vertex.v = (uv >> 8) as u8;
                match index {
                    0 => clut_bits = uv >> 16,
                    1 => page_bits = uv >> 16,
                    _ => {}
                }
            }
            vertices.push(vertex);
        }
        let texture = textured.then(|| {
            // A textured polygon's page attribute replaces the page bits of the draw mode.
            self.draw_mode = (self.draw_mode & !0x09FF) | (page_bits & 0x09FF);
            Texture {
                page: TexturePage::from_bits(page_bits),
                clut: Clut::from_bits(clut_bits),
                window: self.window,
                raw: opcode & 0x01 != 0,
                flip_x: false,
                flip_y: false,
            }
        });
        let shape = if count == 4 { Shape::Quad } else { Shape::Triangle };
        self.draw(opcode, shape, vertices, texture);
    }

    fn line(&mut self, words: &[u32]) {
        let opcode = words[0] >> 24;
        let gouraud = opcode & 0x10 != 0;
        let mut vertices = Vec::new();
        let mut cursor = 0;
        let mut color = words[0] & 0x00FF_FFFF;
        while cursor < words.len() {
            if gouraud || cursor == 0 {
                color = words[cursor] & 0x00FF_FFFF;
                cursor += 1;
            }
            let Some(&position) = words.get(cursor) else { break };
            cursor += 1;
            vertices.push(Vertex { x: sign11(position), y: sign11(position >> 16), color, u: 0, v: 0 });
        }
        self.draw(opcode, Shape::Line, vertices, None);
    }

    fn rect(&mut self, words: &[u32]) {
        let opcode = words[0] >> 24;
        let textured = opcode & 0x04 != 0;
        let position = words[1];
        let mut vertex =
            Vertex { x: sign11(position), y: sign11(position >> 16), color: words[0] & 0x00FF_FFFF, u: 0, v: 0 };
        let mut cursor = 2;
        let mut clut = Clut { x: 0, y: 0 };
        if textured {
            let uv = words[cursor];
            cursor += 1;
            vertex.u = uv as u8;
            vertex.v = (uv >> 8) as u8;
            clut = Clut::from_bits(uv >> 16);
        }
        let (width, height) = match (opcode >> 3) & 3 {
            0 => {
                let size = words[cursor];
                ((size & 0x3FF) as u16, ((size >> 16) & 0x1FF) as u16)
            }
            1 => (1, 1),
            2 => (8, 8),
            _ => (16, 16),
        };
        let texture = textured.then(|| Texture {
            page: TexturePage::from_bits(self.draw_mode),
            clut,
            window: self.window,
            raw: opcode & 0x01 != 0,
            flip_x: self.draw_mode & (1 << 12) != 0,
            flip_y: self.draw_mode & (1 << 13) != 0,
        });
        self.draw(opcode, Shape::Rect { width, height }, vec![vertex], texture);
    }

    fn fill(&mut self, words: &[u32]) {
        let color = words[0] & 0x00FF_FFFF;
        let rect = VramRect {
            x: (words[1] & 0x3F0) as u16,
            y: ((words[1] >> 16) & 0x1FF) as u16,
            width: (((words[2] & 0x3FF) + 0xF) & !0xF) as u16,
            height: ((words[2] >> 16) & 0x1FF) as u16,
        };
        let pixel = to_15bit(color);
        for row in 0..usize::from(rect.height) {
            let y = (usize::from(rect.y) + row) % VRAM_HEIGHT;
            for column in 0..usize::from(rect.width) {
                let x = (usize::from(rect.x) + column) % VRAM_WIDTH;
                self.vram[y * VRAM_WIDTH + x] = pixel;
            }
        }
        self.record(Command::Fill { rect, color });
    }

    fn copy(&mut self, words: &[u32]) {
        let from_x = (words[1] & 0x3FF) as u16;
        let from_y = ((words[1] >> 16) & 0x1FF) as u16;
        let size = transfer_rect(words[2], words[3]);
        for row in 0..usize::from(size.height) {
            for column in 0..usize::from(size.width) {
                let source = self.pixel(usize::from(from_x) + column, usize::from(from_y) + row);
                self.put(usize::from(size.x) + column, usize::from(size.y) + row, source);
            }
        }
        self.record(Command::Copy { from_x, from_y, to: size });
    }

    fn finish_upload(&mut self, rect: VramRect, pixels: Vec<u16>) {
        for (index, &pixel) in pixels.iter().enumerate() {
            let x = usize::from(rect.x) + index % usize::from(rect.width);
            let y = usize::from(rect.y) + index / usize::from(rect.width);
            self.put(x, y, pixel);
        }
        self.record(Command::Upload { rect, pixels });
    }

    fn put(&mut self, x: usize, y: usize, pixel: u16) {
        let at = (y % VRAM_HEIGHT) * VRAM_WIDTH + (x % VRAM_WIDTH);
        if self.check_mask && self.vram[at] & 0x8000 != 0 {
            return;
        }
        self.vram[at] = if self.set_mask { pixel | 0x8000 } else { pixel };
    }
}

/// Words a `GP0` command takes, its parameters included; image data follows separately.
fn command_words(opcode: u32) -> usize {
    match opcode {
        0x02 => 3,
        0x20..=0x3F => {
            let vertices = if opcode & 0x08 != 0 { 4 } else { 3 };
            let per_vertex = 1 + usize::from(opcode & 0x04 != 0);
            let colors = if opcode & 0x10 != 0 { vertices } else { 1 };
            colors + vertices * per_vertex
        }
        0x40..=0x5F => {
            if opcode & 0x10 != 0 {
                4
            } else {
                3
            }
        }
        0x60..=0x7F => 2 + usize::from(opcode & 0x04 != 0) + usize::from((opcode >> 3) & 3 == 0),
        0x80..=0x9F => 4,
        0xA0..=0xDF => 3,
        _ => 1,
    }
}

fn transfer_rect(position: u32, size: u32) -> VramRect {
    VramRect {
        x: (position & 0x3FF) as u16,
        y: ((position >> 16) & 0x1FF) as u16,
        width: ((((size & 0xFFFF).wrapping_sub(1)) & 0x3FF) + 1) as u16,
        height: ((((size >> 16).wrapping_sub(1)) & 0x1FF) + 1) as u16,
    }
}

fn sign11(value: u32) -> i16 {
    (((value & 0x7FF) << 5) as u16 as i16) >> 5
}

fn to_15bit(color: u32) -> u16 {
    let r = (color >> 3) & 0x1F;
    let g = (color >> 11) & 0x1F;
    let b = (color >> 19) & 0x1F;
    (r | (g << 5) | (b << 10)) as u16
}
