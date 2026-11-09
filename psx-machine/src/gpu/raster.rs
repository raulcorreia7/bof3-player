// SPDX-License-Identifier: MIT
//! Draw commands rasterized into VRAM on the CPU, written from psx-spx "GPU Render Polygon
//! Commands", "GPU Render Line Commands", "GPU Render Rectangle Commands", "GPU Rendering
//! Attributes" and "GPU (MISC)" (dithering, semi-transparency, modulation, draw to display).
//!
//! Everything is integer arithmetic. A pixel is inside a triangle when its edge functions say
//! so, with the lower-right edges excluded. psx-spx does not give the hardware's interpolation
//! and line-stepping precision; the fixed-point forms below are the ones the `ps1-tests`
//! reference images agree with (see the crate README).

use super::{Depth, Draw, DrawArea, Shape, Texture, VRAM_HEIGHT, VRAM_WIDTH, Vertex};

/// psx-spx "GPU Rendering Attributes", Size Restriction: the largest distance between two
/// vertices of a drawn polygon or line.
const MAX_WIDTH: i32 = 1023;
/// The same restriction, vertically.
const MAX_HEIGHT: i32 = 511;

/// psx-spx "GPU (MISC)", 24bit RGB to 15bit RGB Dithering: the offset added to each 8-bit
/// channel, by `[y & 3][x & 3]`.
const DITHER: [[i32; 4]; 4] = [[-4, 0, -3, 1], [2, -2, 3, -1], [-3, 1, -4, 0], [3, -1, 2, -2]];

/// Bit 15 of a VRAM pixel or texel: the mask bit, or the texel's semi-transparency flag.
const MASK_BIT: u16 = 0x8000;

/// Fraction bits of a triangle's colour and texture coordinate planes.
const PLANE_FRACTION: u32 = 12;
/// Fraction bits of a line's colour, which starts half a step up and steps by the colour change
/// per pixel truncated toward zero. The ps1-tests `gpu/lines` reference needs at least 12.
const SHADE_FRACTION: u32 = 12;
/// Fraction bits of a line's position.
const LINE_FRACTION: u32 = 32;
/// Half a pixel, where a line's position starts within its first pixel.
const LINE_HALF: i64 = 1 << (LINE_FRACTION - 1);
/// The start of a line sits this far before the pixel centre on the x axis, and on the y axis
/// when the line goes up. Fitted to the ps1-tests `gpu/lines` reference image.
const LINE_BIAS: i64 = 1 << 10;

/// The drawing state a rasterized command uses that its log entry does not carry.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mode {
    /// `GP0(E1h)` bits 5-6, the semi-transparency mode of untextured commands.
    pub blend: u8,
    /// `GP0(E1h)` bit 9, dithering.
    pub dither: bool,
    /// `GP0(E6h)` bit 0: written pixels get the mask bit.
    pub set_mask: bool,
    /// `GP0(E6h)` bit 1: pixels with the mask bit are not overwritten.
    pub check_mask: bool,
    /// Rows of this parity are not drawn (480-line interlace with `GP0(E1h)` bit 10 clear).
    pub skip_parity: Option<usize>,
}

/// Rasterizes `draw` into `vram`; a 4- or 8-bit texture looks its texels up in `palette`, the
/// CLUT cache.
pub(super) fn draw(vram: &mut [u16], palette: &[u16; 256], draw: &Draw, mode: Mode) {
    let textured = draw.texture.is_some_and(|texture| !texture.raw);
    let mut painter = Painter {
        vram,
        area: draw.area,
        mode,
        blend: draw.semi_transparent.then(|| draw.texture.map_or(mode.blend, |texture| texture.page.blend)),
        dither: false,
        sampler: draw.texture.map(|texture| Sampler::new(texture, palette)),
        modulate: textured,
        flips: draw.texture.map_or((false, false), |texture| (texture.flip_x, texture.flip_y)),
    };
    let offset = (i32::from(draw.offset.0), i32::from(draw.offset.1));
    let v = &draw.vertices;
    match draw.shape {
        Shape::Triangle | Shape::Quad => {
            // Raw textures are drawn as they are, so neither shading nor dithering applies.
            let raw = draw.texture.is_some_and(|texture| texture.raw);
            painter.dither = mode.dither && !raw && (draw.gouraud || textured);
            let shaded = draw.gouraud && !raw;
            painter.triangle([v[0], v[1], v[2]], offset, shaded);
            if draw.shape == Shape::Quad {
                painter.triangle([v[1], v[2], v[3]], offset, shaded);
            }
        }
        Shape::Line => {
            painter.dither = mode.dither;
            for pair in v.windows(2) {
                painter.line(pair[0], pair[1], offset);
            }
        }
        Shape::Rect { width, height } => painter.rect(v[0], offset, (i32::from(width), i32::from(height))),
    }
}

/// Whether two vertices are too far apart for the hardware to draw the edge between them.
fn too_far(a: Vertex, b: Vertex) -> bool {
    (i32::from(a.x) - i32::from(b.x)).abs() > MAX_WIDTH || (i32::from(a.y) - i32::from(b.y)).abs() > MAX_HEIGHT
}

/// The red, green and blue bytes of a `0xBBGGRR` colour.
fn channels(color: u32) -> [i32; 3] {
    [(color & 0xFF) as i32, ((color >> 8) & 0xFF) as i32, ((color >> 16) & 0xFF) as i32]
}

/// Reads texels through the texture window, psx-spx "GPU Video Memory (VRAM)", Texture Bitmaps.
#[derive(Clone, Copy)]
struct Sampler<'a> {
    page_x: usize,
    page_y: usize,
    depth: Depth,
    palette: &'a [u16; 256],
    keep_u: u8,
    set_u: u8,
    keep_v: u8,
    set_v: u8,
}

impl<'a> Sampler<'a> {
    fn new(texture: Texture, palette: &'a [u16; 256]) -> Self {
        let window = texture.window;
        Self {
            page_x: usize::from(texture.page.x),
            page_y: usize::from(texture.page.y),
            depth: texture.page.depth,
            palette,
            keep_u: !(window.mask_x << 3),
            set_u: (window.offset_x & window.mask_x) << 3,
            keep_v: !(window.mask_y << 3),
            set_v: (window.offset_y & window.mask_y) << 3,
        }
    }

    fn texel(&self, vram: &[u16], u: u8, v: u8) -> u16 {
        let u = usize::from((u & self.keep_u) | self.set_u);
        let v = usize::from((v & self.keep_v) | self.set_v);
        let row = ((self.page_y + v) % VRAM_HEIGHT) * VRAM_WIDTH;
        let at = |x: usize| vram[row + x % VRAM_WIDTH];
        let lookup = |index: usize| self.palette[index];
        match self.depth {
            Depth::Bits4 => lookup(usize::from((at(self.page_x + u / 4) >> ((u % 4) * 4)) & 0xF)),
            Depth::Bits8 => lookup(usize::from((at(self.page_x + u / 2) >> ((u % 2) * 8)) & 0xFF)),
            Depth::Bits15 => at(self.page_x + u),
        }
    }
}

/// One attribute across a triangle: the plane through its three vertex values in fixed point,
/// with [`PLANE_FRACTION`] fraction bits, its gradients truncated toward zero, based at the
/// leftmost vertex and rounded to the nearest whole value there. Fitted to the ps1-tests
/// `gpu/triangle` and `gpu/uv-interpolation` reference images, which every other choice tried
/// (more fraction bits, other roundings, the top vertex as base) misses by one in places.
#[derive(Clone, Copy, Default)]
struct Plane {
    /// The value at the current pixel, in fixed point.
    value: i64,
    /// The value at the base vertex, in fixed point.
    base: i64,
    /// Change per pixel to the right.
    step_x: i64,
    /// Change per row down.
    step_y: i64,
    /// The base vertex.
    origin: (i64, i64),
}

impl Plane {
    /// The plane through `values` at the triangle `points`, whose doubled signed area is `area`
    /// (positive), based at the vertex `origin` indexes.
    fn new(points: [(i64, i64); 3], values: [i64; 3], area: i64, origin: usize) -> Self {
        let [(x0, y0), (x1, y1), (x2, y2)] = points;
        let (d1, d2) = (values[1] - values[0], values[2] - values[0]);
        let step_x = ((d1 * (y2 - y0) - d2 * (y1 - y0)) << PLANE_FRACTION) / area;
        let step_y = ((d2 * (x1 - x0) - d1 * (x2 - x0)) << PLANE_FRACTION) / area;
        Self {
            value: 0,
            base: (values[origin] << PLANE_FRACTION) + (1 << (PLANE_FRACTION - 1)),
            step_x,
            step_y,
            origin: points[origin],
        }
    }

    /// Moves to pixel `(x, y)`.
    fn start(&mut self, x: i64, y: i64) {
        self.value = self.base + self.step_x * (x - self.origin.0) + self.step_y * (y - self.origin.1);
    }

    fn value(&self) -> i64 {
        self.value >> PLANE_FRACTION
    }

    /// Moves one pixel to the right.
    fn next(&mut self) {
        self.value += self.step_x;
    }
}

/// One triangle edge as `a * x + b * y + c >= threshold`.
#[derive(Clone, Copy)]
struct Edge {
    a: i64,
    b: i64,
    c: i64,
    threshold: i64,
}

impl Edge {
    /// The edge from `from` to `to` of a triangle whose inside is on its left in VRAM order
    /// (positive area). Pixels on a left or top edge are inside; on a right or bottom edge they
    /// are not, so the lower-right edges are excluded as psx-spx describes.
    fn new(from: (i64, i64), to: (i64, i64)) -> Self {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let left_or_top = dy < 0 || (dy == 0 && dx > 0);
        Self { a: -dy, b: dx, c: dy * from.0 - dx * from.1, threshold: if left_or_top { 0 } else { 1 } }
    }

    /// Narrows `span` to the pixels of row `y` on the inside of the edge.
    fn clip(&self, y: i64, span: (i64, i64)) -> (i64, i64) {
        let rest = self.threshold - (self.b * y + self.c);
        match self.a.signum() {
            1 => (span.0.max(-((-rest).div_euclid(self.a))), span.1),
            -1 => (span.0, span.1.min((-rest).div_euclid(-self.a))),
            _ if rest <= 0 => span,
            _ => (span.0, span.0 - 1),
        }
    }
}

/// The colour and texture inputs of one pixel.
#[derive(Clone, Copy)]
struct Fragment {
    rgb: [i32; 3],
    u: u8,
    v: u8,
}

struct Painter<'a> {
    vram: &'a mut [u16],
    area: DrawArea,
    mode: Mode,
    /// The semi-transparency mode, for a semi-transparent command.
    blend: Option<u8>,
    dither: bool,
    sampler: Option<Sampler<'a>>,
    /// Texels are modulated by the colour.
    modulate: bool,
    /// A rectangle's texture runs backwards horizontally, vertically.
    flips: (bool, bool),
}

impl Painter<'_> {
    fn row_visible(&self, y: i64) -> bool {
        y >= i64::from(self.area.top)
            && y <= i64::from(self.area.bottom)
            && self.mode.skip_parity.is_none_or(|parity| y.rem_euclid(2) as usize != parity)
    }

    fn columns(&self) -> (i64, i64) {
        (i64::from(self.area.left), i64::from(self.area.right))
    }

    /// Writes one pixel inside the drawing area, psx-spx "GPU (MISC)" and "GP0(E6h)".
    fn plot(&mut self, x: i64, y: i64, fragment: Fragment) {
        let at = (y as usize % VRAM_HEIGHT) * VRAM_WIDTH + x as usize % VRAM_WIDTH;
        let back = self.vram[at];
        if self.mode.check_mask && back & MASK_BIT != 0 {
            return;
        }
        let dither = if self.dither { DITHER[(y & 3) as usize][(x & 3) as usize] } else { 0 };
        let to_5bit = |channel: i32| ((channel + dither).clamp(0, 0xFF) >> 3) as u16;
        let (mut color, mut semi) = (0, self.blend.is_some());
        let mut mask = 0;
        if let Some(sampler) = self.sampler {
            let texel = sampler.texel(self.vram, fragment.u, fragment.v);
            if texel == 0 {
                return;
            }
            semi &= texel & MASK_BIT != 0;
            mask = texel & MASK_BIT;
            color = if self.modulate {
                // psx-spx "GPU (MISC)", Modulation: texel * colour / 128, on the texel widened to 8 bits.
                (0..3).fold(0, |color, channel| {
                    let texel = i32::from((texel >> (5 * channel)) & 0x1F) << 3;
                    color | to_5bit((texel * fragment.rgb[channel]) >> 7) << (5 * channel)
                })
            } else {
                texel & !MASK_BIT
            };
        } else {
            for (channel, &value) in fragment.rgb.iter().enumerate() {
                color |= to_5bit(value) << (5 * channel);
            }
        }
        if semi && let Some(blend) = self.blend {
            color = mix(blend, back, color);
        }
        if self.mode.set_mask {
            mask = MASK_BIT;
        }
        self.vram[at] = color | mask;
    }

    fn triangle(&mut self, vertices: [Vertex; 3], offset: (i32, i32), shaded: bool) {
        if too_far(vertices[0], vertices[1]) || too_far(vertices[1], vertices[2]) || too_far(vertices[0], vertices[2]) {
            return;
        }
        let mut points = vertices
            .map(|vertex| (i64::from(i32::from(vertex.x) + offset.0), i64::from(i32::from(vertex.y) + offset.1)));
        let mut vertices = vertices;
        let cross = |p: &[(i64, i64); 3]| (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        let mut area = cross(&points);
        if area == 0 {
            return;
        }
        if area < 0 {
            points.swap(1, 2);
            vertices.swap(1, 2);
            area = -area;
        }
        let edges = [Edge::new(points[0], points[1]), Edge::new(points[1], points[2]), Edge::new(points[2], points[0])];
        let rgb = vertices.map(|vertex| channels(vertex.color));
        let origin = (0..3).min_by_key(|&index| points[index]).unwrap_or(0);
        let plane = |values: [i64; 3]| Plane::new(points, values, area, origin);
        let mut planes = [Plane::default(); 5];
        if shaded {
            for channel in 0..3 {
                planes[channel] = plane(rgb.map(|color| i64::from(color[channel])));
            }
        }
        let textured = self.sampler.is_some();
        if textured {
            planes[3] = plane(vertices.map(|vertex| i64::from(vertex.u)));
            planes[4] = plane(vertices.map(|vertex| i64::from(vertex.v)));
        }
        let mut fragment = Fragment { rgb: rgb[0], u: 0, v: 0 };
        let (top, bottom) =
            (points.iter().map(|p| p.1).min().unwrap_or(0), points.iter().map(|p| p.1).max().unwrap_or(0));
        let (left, right) =
            (points.iter().map(|p| p.0).min().unwrap_or(0), points.iter().map(|p| p.0).max().unwrap_or(0));
        let (area_left, area_right) = self.columns();
        let used = (if shaded { 0 } else { 3 })..(if textured { 5 } else { 3 });
        for y in top.max(i64::from(self.area.top))..=bottom.min(i64::from(self.area.bottom)) {
            if !self.row_visible(y) {
                continue;
            }
            let span = edges.iter().fold((left.max(area_left), right.min(area_right)), |span, edge| edge.clip(y, span));
            if span.0 > span.1 {
                continue;
            }
            for index in used.clone() {
                planes[index].start(span.0, y);
            }
            for x in span.0..=span.1 {
                if shaded {
                    for (channel, plane) in fragment.rgb.iter_mut().zip(&planes) {
                        *channel = plane.value() as i32;
                    }
                }
                if textured {
                    fragment.u = planes[3].value() as u8;
                    fragment.v = planes[4].value() as u8;
                }
                self.plot(x, y, fragment);
                for index in used.clone() {
                    planes[index].next();
                }
            }
        }
    }

    /// A line with both end points drawn, psx-spx "GPU Render Line Commands". The position steps
    /// along the longer axis in 32-bit fixed point (see [`line_step`]), which decides where a
    /// pixel falls when the line passes exactly between two.
    fn line(&mut self, from: Vertex, to: Vertex, offset: (i32, i32)) {
        if too_far(from, to) {
            return;
        }
        let (dx, dy) = (i64::from(to.x) - i64::from(from.x), i64::from(to.y) - i64::from(from.y));
        let steps = dx.abs().max(dy.abs());
        let (step_x, step_y) = (line_step(dx, steps), line_step(dy, steps));
        let mut x = (i64::from(i32::from(from.x) + offset.0) << LINE_FRACTION) + LINE_HALF - LINE_BIAS;
        let mut y = (i64::from(i32::from(from.y) + offset.1) << LINE_FRACTION) + LINE_HALF;
        if step_y < 0 {
            y -= LINE_BIAS;
        }
        let (first, last) = (channels(from.color), channels(to.color));
        let mut shade: [i64; 3] =
            std::array::from_fn(|channel| (i64::from(first[channel]) << SHADE_FRACTION) + (1 << (SHADE_FRACTION - 1)));
        let shade_step: [i64; 3] = std::array::from_fn(|channel| {
            if steps == 0 { 0 } else { (i64::from(last[channel] - first[channel]) << SHADE_FRACTION) / steps }
        });
        let (area_left, area_right) = self.columns();
        for _ in 0..=steps {
            let (column, row) = (x >> LINE_FRACTION, y >> LINE_FRACTION);
            let rgb = shade.map(|value| (value >> SHADE_FRACTION) as i32);
            x += step_x;
            y += step_y;
            for (value, step) in shade.iter_mut().zip(shade_step) {
                *value += step;
            }
            if column < area_left || column > area_right || !self.row_visible(row) {
                continue;
            }
            self.plot(column, row, Fragment { rgb, u: 0, v: 0 });
        }
    }

    /// A rectangle from its top-left corner, psx-spx "GPU Render Rectangle Commands": texture
    /// coordinates step by one texel per pixel, backwards along a flipped axis.
    fn rect(&mut self, corner: Vertex, offset: (i32, i32), size: (i32, i32)) {
        let left = i64::from(i32::from(corner.x) + offset.0);
        let top = i64::from(i32::from(corner.y) + offset.1);
        let rgb = channels(corner.color);
        let (area_left, area_right) = self.columns();
        let columns = (area_left - left).max(0)..i64::from(size.0).min(area_right - left + 1);
        let rows =
            (i64::from(self.area.top) - top).max(0)..i64::from(size.1).min(i64::from(self.area.bottom) - top + 1);
        let flips = self.flips;
        for row in rows {
            let y = top + row;
            if !self.row_visible(y) {
                continue;
            }
            let v = if flips.1 { corner.v.wrapping_sub(row as u8) } else { corner.v.wrapping_add(row as u8) };
            for column in columns.clone() {
                let x = left + column;
                let u = if flips.0 {
                    // One texel to the right of the corner's U, as the ps1-tests `gpu/texture-flip`
                    // reference image shows (measured with an even U only).
                    corner.u.wrapping_add(1).wrapping_sub(column as u8)
                } else {
                    corner.u.wrapping_add(column as u8)
                };
                self.plot(x, y, Fragment { rgb, u, v });
            }
        }
    }
}

/// psx-spx "GPU (MISC)", Semi-transparency: the back pixel `back` and the front colour `front`,
/// per 5-bit channel, saturated.
fn mix(mode: u8, back: u16, front: u16) -> u16 {
    (0..3).fold(0, |color, channel| {
        let shift = 5 * channel;
        let b = i32::from((back >> shift) & 0x1F);
        let f = i32::from((front >> shift) & 0x1F);
        let mixed = match mode {
            0 => (b + f) >> 1,
            1 => b + f,
            2 => b - f,
            _ => b + (f >> 2),
        };
        color | (mixed.clamp(0, 0x1F) as u16) << shift
    })
}

/// The per-step advance of a line's position along one axis: `delta / steps` in fixed point,
/// rounded away from zero.
fn line_step(delta: i64, steps: i64) -> i64 {
    if steps == 0 {
        return 0;
    }
    let scaled = delta << LINE_FRACTION;
    let away = if scaled < 0 { -(steps - 1) } else { steps - 1 };
    (scaled + away) / steps
}
