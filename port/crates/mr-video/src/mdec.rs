//! The PS1 MDEC (motion decoder) as the game drives it, in integers.
//!
//! Input: MDEC run-level halfwords (what libpress's DecDCTvlc hands the
//! MDEC by DMA; `vlc.rs` makes them from the STR bitstream). Output: 24-bit
//! RGB (the game's movie mode: DecDCTin mode 3 -> MDEC command bits 27-28 =
//! 3 (24 bpp), unsigned, written to VRAM by LoadImage) [CODE FMV.BIN overlay
//! 0x800E4EB0: 0x80160084 = 3; DISC FMV.BIN tables below].
//!
//! The arithmetic follows the hardware as documented by psx-spx ("MDEC")
//! and modelled bit-exactly by Mednafen/Beetle's MDEC (which the oracle's
//! runtime, psxrecomp runtime/src/mdec.c, reproduces). Each step:
//!
//! 1. Run-level decode (psx-spx "rl_decode_block"): a block starts with a
//!    halfword `qscale << 10 | dc` (0xFE00 padding words skipped), then
//!    `run << 10 | level` halfwords until 0xFE00 (end of block). `k`
//!    (zig-zag position) advances by run + 1; k > 63 ends the block.
//! 2. Dequantisation in Beetle's x16 fixed point, clamped to
//!    [-0x4000, 0x3FFF]:
//!    DC: `(level * qt[0]) << 4`, AC: `((level * qscale * qt[k]) >> 3) << 4`,
//!    each non-zero coefficient plus a rounding bias of -8 (positive) /
//!    +8 (negative); a zero quantiser gives `(level * 2) << 4`.
//!    The coefficient goes to `ZIGZAG[k]` (column-major: the transpose of
//!    the MPEG zig-zag, which pairs with the transposed IDCT below).
//! 3. IDCT: two separable 8-point passes with the scale matrix the game
//!    uploads (MDEC command 3, libpress's table; entries stored transposed
//!    and pre-shifted >> 3): `sum = Σ src[u] * scale[x][u]`,
//!    `(sum + 0x4000) >> 15`; pass 1 keeps an i16 and transposes, pass 2
//!    wraps to 9 bits and clamps to [-128, 127] ("Mask9ClampS8").
//! 4. YCbCr -> RGB per pixel (psx-spx: R = Y + 1.402 Cr, G = Y - 0.3437 Cb
//!    - 0.7143 Cr, B = Y + 1.772 Cb) in the hardware's /256 integers:
//!    `R = Y + ((359 Cr + 0x80) >> 8)`,
//!    `G = Y + (((-88 Cb) & !0x1F) + ((-183 Cr) & !7) + 0x80) >> 8)` (the
//!    reduced-precision green terms), `B = Y + ((454 Cb + 0x80) >> 8)`,
//!    each Mask9ClampS8, then `^ 0x80` to unsigned bytes. Chroma is one
//!    sample per 2x2 pixels (Cr / Cb blocks cover the 16x16 macroblock).
//! 5. Macroblock order: Cr, Cb, Y0 (top left), Y1 (top right), Y2 (bottom
//!    left), Y3 (bottom right); macroblocks column by column (top to bottom,
//!    then left to right), as the game's LoadImage slices place them.

/// libpress's IQ table (zig-zag order; the MPEG-1 default intra matrix with
/// DC 2), uploaded by DecDCTPutEnv for luma and chroma alike [DISC FMV.BIN
/// 0x3E0C and 0x3E4C; test `tables_match_the_disc`].
pub const QUANT: [u8; 64] = [
    2, 16, 16, 19, 16, 19, 22, 22, 22, 22, 22, 22, 26, 24, 26, 27, //
    27, 27, 26, 26, 26, 26, 27, 27, 27, 29, 29, 29, 34, 34, 34, 29, //
    29, 29, 27, 27, 29, 29, 32, 32, 34, 34, 37, 38, 37, 35, 35, 34, //
    35, 38, 38, 40, 40, 40, 48, 48, 46, 46, 56, 56, 58, 69, 69, 83,
];

/// libpress's IDCT scale matrix as uploaded (row f = frequency, column n =
/// sample; c(f) cos((2n + 1) f pi / 16) in 1.15; the negative entries are
/// the one's complement of the positive ones, e.g. 0xE707 = !0x18F8, one
/// below psx-spx's listing) [DISC FMV.BIN 0x3E90].
pub const SCALE: [i16; 64] = {
    const R: [u16; 64] = [
        0x5A82, 0x5A82, 0x5A82, 0x5A82, 0x5A82, 0x5A82, 0x5A82, 0x5A82, //
        0x7D8A, 0x6A6D, 0x471C, 0x18F8, 0xE707, 0xB8E3, 0x9592, 0x8275, //
        0x7641, 0x30FB, 0xCF04, 0x89BE, 0x89BE, 0xCF04, 0x30FB, 0x7641, //
        0x6A6D, 0xE707, 0x8275, 0xB8E3, 0x471C, 0x7D8A, 0x18F8, 0x9592, //
        0x5A82, 0xA57D, 0xA57D, 0x5A82, 0x5A82, 0xA57D, 0xA57D, 0x5A82, //
        0x471C, 0x8275, 0x18F8, 0x6A6D, 0x9592, 0xE707, 0x7D8A, 0xB8E3, //
        0x30FB, 0x89BE, 0x7641, 0xCF04, 0xCF04, 0x7641, 0x89BE, 0x30FB, //
        0x18F8, 0xB8E3, 0x6A6D, 0x8275, 0x7D8A, 0x9592, 0x471C, 0xE707,
    ];
    let mut o = [0i16; 64];
    let mut i = 0;
    while i < 64 {
        o[i] = R[i] as i16;
        i += 1;
    }
    o
};

/// MPEG zig-zag (scan position -> row-major index).
const MPEG_ZIGZAG: [u8; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, //
    12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, //
    35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, //
    58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// The MDEC's scan order: the transpose of the MPEG table (column-major).
pub const ZIGZAG: [u8; 64] = {
    let mut o = [0u8; 64];
    let mut k = 0;
    while k < 64 {
        let v = MPEG_ZIGZAG[k];
        o[k] = (v % 8) * 8 + v / 8;
        k += 1;
    }
    o
};

/// End-of-block / padding halfword.
pub const EOB: u16 = 0xFE00;

#[inline]
fn sext10(v: u16) -> i32 {
    ((v as i32) << 22) >> 22
}

/// 9-bit wrap, then clamp to i8 (Beetle Mask9ClampS8).
#[inline]
fn m9(v: i32) -> i32 {
    (((v as u32) << 23) as i32 >> 23).clamp(-128, 127)
}

/// The decoder's tables (what the game uploads: quant for luma / chroma and
/// the IDCT matrix, transposed and >> 3).
#[derive(Clone)]
pub struct Mdec {
    y_quant: [u8; 64],
    uv_quant: [u8; 64],
    scale: [i16; 64],
}

impl Default for Mdec {
    fn default() -> Self {
        Mdec::new(&QUANT, &QUANT, &SCALE)
    }
}

impl Mdec {
    pub fn new(y_quant: &[u8; 64], uv_quant: &[u8; 64], scale: &[i16; 64]) -> Mdec {
        let mut s = [0i16; 64];
        for (i, &v) in scale.iter().enumerate() {
            s[((i & 7) << 3) | ((i >> 3) & 7)] = v >> 3;
        }
        Mdec { y_quant: *y_quant, uv_quant: *uv_quant, scale: s }
    }

    /// One block: run-level decode from `src[*pos..]`, dequantise, IDCT.
    /// Returns false when the input ran out before a block started.
    pub fn block(&self, src: &[u16], pos: &mut usize, chroma: bool, out: &mut [i16; 64]) -> bool {
        let quant = if chroma { &self.uv_quant } else { &self.y_quant };
        *out = [0; 64];
        let mut w = loop {
            let Some(&w) = src.get(*pos) else { return false };
            *pos += 1;
            if w != EOB {
                break w;
            }
        };
        let deq = |ci: i32, q: i32, ac: bool| -> i16 {
            let t = if q != 0 {
                let p = if ac { ((ci * q) >> 3) << 4 } else { (ci * q) << 4 };
                p + if ci == 0 { 0 } else if ci < 0 { 8 } else { -8 }
            } else {
                (ci * 2) << 4
            };
            t.clamp(-0x4000, 0x3FFF) as i16
        };
        let qscale = ((w >> 10) & 0x3F) as i32;
        out[0] = deq(sext10(w & 0x3FF), quant[0] as i32, false);
        let mut k = 0usize;
        while k < 63 {
            let Some(&n) = src.get(*pos) else { break };
            *pos += 1;
            w = n;
            if w == EOB {
                break;
            }
            k += ((w >> 10) & 0x3F) as usize + 1;
            if k >= 64 {
                break;
            }
            out[ZIGZAG[k] as usize] = deq(sext10(w & 0x3FF), qscale * quant[k] as i32, true);
        }
        self.idct(out);
        true
    }

    fn idct(&self, b: &mut [i16; 64]) {
        let mut tmp = [0i16; 64];
        for col in 0..8 {
            let src = &b[col * 8..col * 8 + 8];
            for x in 0..8 {
                let sc = &self.scale[x * 8..x * 8 + 8];
                let sum: i32 = (0..8).map(|u| src[u] as i32 * sc[u] as i32).sum();
                tmp[x * 8 + col] = ((sum + 0x4000) >> 15) as i16;
            }
        }
        for col in 0..8 {
            let src = &tmp[col * 8..col * 8 + 8];
            for x in 0..8 {
                let sc = &self.scale[x * 8..x * 8 + 8];
                let sum: i32 = (0..8).map(|u| src[u] as i32 * sc[u] as i32).sum();
                b[col * 8 + x] = m9((sum + 0x4000) >> 15) as i16;
            }
        }
    }

    /// Decode a whole picture of `w` x `h` pixels (multiples of 16 after
    /// rounding up) into `rgb` (3 bytes per pixel, row-major, `w * h * 3`).
    /// Returns the macroblocks decoded (fewer = the input ran out).
    pub fn picture(&self, src: &[u16], w: usize, h: usize, rgb: &mut Vec<u8>) -> usize {
        rgb.clear();
        rgb.resize(w * h * 3, 0);
        let (mbw, mbh) = (w.div_ceil(16), h.div_ceil(16));
        let mut pos = 0usize;
        let (mut cr, mut cb) = ([0i16; 64], [0i16; 64]);
        let mut y = [[0i16; 64]; 4];
        for n in 0..mbw * mbh {
            let ok = self.block(src, &mut pos, true, &mut cr)
                && self.block(src, &mut pos, true, &mut cb)
                && self.block(src, &mut pos, false, &mut y[0])
                && self.block(src, &mut pos, false, &mut y[1])
                && self.block(src, &mut pos, false, &mut y[2])
                && self.block(src, &mut pos, false, &mut y[3]);
            if !ok {
                return n;
            }
            let (mx, my) = (n / mbh * 16, n % mbh * 16);
            for py in 0..16 {
                let yy = my + py;
                if yy >= h {
                    break;
                }
                for px in 0..16 {
                    let xx = mx + px;
                    if xx >= w {
                        break;
                    }
                    let yb = &y[(py >> 3) * 2 + (px >> 3)];
                    let l = yb[(px & 7) + (py & 7) * 8] as i32;
                    let c = (px >> 1) + (py >> 1) * 8;
                    let p = ycbcr_to_rgb(l, cr[c] as i32, cb[c] as i32);
                    let o = (yy * w + xx) * 3;
                    rgb[o..o + 3].copy_from_slice(&p);
                }
            }
        }
        mbw * mbh
    }
}

/// One pixel, signed Y / Cr / Cb (-128..127) -> unsigned 24-bit RGB.
#[inline]
pub fn ycbcr_to_rgb(y: i32, cr: i32, cb: i32) -> [u8; 3] {
    let r = m9(y + ((359 * cr + 0x80) >> 8));
    let g = m9(y + ((((-88 * cb) & !0x1F) + ((-183 * cr) & !0x07) + 0x80) >> 8));
    let b = m9(y + ((454 * cb + 0x80) >> 8));
    [(r as u8) ^ 0x80, (g as u8) ^ 0x80, (b as u8) ^ 0x80]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_is_column_major() {
        assert_eq!(&ZIGZAG[..10], &[0x00, 0x08, 0x01, 0x02, 0x09, 0x10, 0x18, 0x11, 0x0A, 0x03]);
        let mut seen = [false; 64];
        for &z in &ZIGZAG {
            seen[z as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn flat_block_is_flat() {
        // DC only: every sample equal; DC 0 -> 0 -> grey 0x80 after ^0x80.
        let m = Mdec::default();
        let mut b = [0i16; 64];
        let mut pos = 0;
        assert!(m.block(&[0x0000 | 40, EOB], &mut pos, false, &mut b));
        assert_eq!(pos, 2);
        assert!(b.iter().all(|&v| v == b[0]), "{b:?}");
        // DC 40 * qt 2 = 80, x16 - 8 = 1272; the two passes (scale 0x5A82
        // >> 3 = 2896 each): 1272 * 2896 / 32768 = 112 (rounded), then
        // 112 * 2896 / 32768 = 10 (rounded)
        assert_eq!(b[0], 10);
        let mut pos = 0;
        assert!(m.block(&[0, EOB], &mut pos, false, &mut b));
        assert!(b.iter().all(|&v| v == 0));
        assert_eq!(ycbcr_to_rgb(0, 0, 0), [0x80; 3]);
        assert_eq!(ycbcr_to_rgb(127, 0, 0), [0xFF; 3]);
        assert_eq!(ycbcr_to_rgb(-128, 0, 0), [0; 3]);
    }

    #[test]
    fn mask9_wraps_before_clamping() {
        assert_eq!(m9(300), -128); // 300 = 0x12C -> 9 bits 0x12C = -212 -> -128
        assert_eq!(m9(200), 127);
        assert_eq!(m9(-200), -128);
        assert_eq!(m9(-300), 127); // wraps to 212
    }
}
