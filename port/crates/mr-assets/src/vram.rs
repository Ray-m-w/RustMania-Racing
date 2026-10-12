//! A 1024x512 16-bit VRAM image, the target of texture uploads, and texel
//! fetch through a texture page + CLUT the way the PS1 GPU samples it.
//! Pixel rules follow the quarry's mr-vram (psx-spx "Texture Mapping"):
//! 15-bit 0BBBBBGGGGGRRRRR, bit 15 = semi-transparency flag; 4-bit and 8-bit
//! texels index a CLUT row; texel value 0x0000 is transparent.

pub const W: usize = 1024;
pub const H: usize = 512;

#[derive(Clone)]
pub struct Vram {
    pub px: Vec<u16>,
}

impl Default for Vram {
    fn default() -> Self {
        Vram { px: vec![0; W * H] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    B4,
    B8,
    B15,
}

impl Vram {
    pub fn get(&self, x: usize, y: usize) -> u16 {
        self.px[(y & (H - 1)) * W + (x & (W - 1))]
    }
    pub fn set(&mut self, x: usize, y: usize, v: u16) {
        self.px[(y & (H - 1)) * W + (x & (W - 1))] = v;
    }
    /// Copy a w*h block of 16-bit words (row-major) to (x, y), like LoadImage.
    pub fn load_image(&mut self, x: usize, y: usize, w: usize, h: usize, data: &[u16]) {
        for r in 0..h {
            for c in 0..w {
                self.set(x + c, y + r, data[r * w + c]);
            }
        }
    }
    /// Raw 15-bit texel at texture coordinate (u, v) of the page at
    /// (page_x, page_y) VRAM pixels, with CLUT at (clut_x, clut_y).
    pub fn texel(&self, depth: Depth, page_x: usize, page_y: usize, clut_x: usize, clut_y: usize, u: u8, v: u8) -> u16 {
        let (u, v) = (u as usize, v as usize);
        match depth {
            Depth::B4 => {
                let w = self.get(page_x + u / 4, page_y + v);
                let idx = (w >> ((u & 3) * 4)) & 0xF;
                self.get(clut_x + idx as usize, clut_y)
            }
            Depth::B8 => {
                let w = self.get(page_x + u / 2, page_y + v);
                let idx = (w >> ((u & 1) * 8)) & 0xFF;
                self.get(clut_x + idx as usize, clut_y)
            }
            Depth::B15 => self.get(page_x + u, page_y + v),
        }
    }
}

/// 15-bit PS1 colour to RGBA8 (0x0000 -> transparent).
pub fn rgba(c: u16) -> [u8; 4] {
    if c == 0 {
        return [0, 0, 0, 0];
    }
    let r = (c & 31) as u32;
    let g = ((c >> 5) & 31) as u32;
    let b = ((c >> 10) & 31) as u32;
    [(r * 255 / 31) as u8, (g * 255 / 31) as u8, (b * 255 / 31) as u8, 255]
}

/// GPU texpage attribute (as in a polygon's tpage halfword) -> (page_x, page_y, depth).
pub fn tpage(attr: u16) -> (usize, usize, Depth) {
    let px = (attr as usize & 0xF) * 64;
    let py = ((attr as usize >> 4) & 1) * 256;
    let d = match (attr >> 7) & 3 {
        0 => Depth::B4,
        1 => Depth::B8,
        _ => Depth::B15,
    };
    (px, py, d)
}

/// GPU CLUT attribute -> (clut_x, clut_y).
pub fn clut(attr: u16) -> (usize, usize) {
    ((attr as usize & 0x3F) * 16, (attr as usize >> 6) & 0x1FF)
}
