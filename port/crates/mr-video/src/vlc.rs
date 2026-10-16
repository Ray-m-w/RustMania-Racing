//! The STR bitstream (version 2, the only one on this disc) -> MDEC
//! run-level halfwords: what libpress's DecDCTvlc does on the CPU.
//!
//! Frame header (8 bytes, psx-spx "CDROM File Video Bitstream"): u16 MDEC
//! word count (32-bit words, rounded up), u16 0x3800, u16 quantiser scale,
//! u16 version (2). Then the bitstream, read as little-endian 16-bit words,
//! most significant bit first. Per block (Cr, Cb, Y0..Y3 per macroblock):
//! - DC: 10 bits, two's complement; halfword `qscale << 10 | dc` (v2 has no
//!   DC prediction; v3 would, and is not used by these files);
//! - AC: MPEG-1 "dct_coeff_next" codes (ISO 11172-2 table B.5c..g; the
//!   table below) + a sign bit (1 = negative), halfword `run << 10 |
//!   level & 0x3FF`; escape `000001` + 6-bit run + 10-bit signed level;
//!   `10` = end of block (halfword 0xFE00).

use std::sync::OnceLock;

/// Codes without the sign bit: (bits, run, level).
const AC: &[(&str, u8, u16)] = &[
    ("11", 0, 1),
    ("011", 1, 1),
    ("0100", 0, 2),
    ("0101", 2, 1),
    ("00101", 0, 3),
    ("00111", 3, 1),
    ("00110", 4, 1),
    ("000110", 1, 2),
    ("000111", 5, 1),
    ("000101", 6, 1),
    ("000100", 7, 1),
    ("0000110", 0, 4),
    ("0000100", 2, 2),
    ("0000111", 8, 1),
    ("0000101", 9, 1),
    ("00100110", 0, 5),
    ("00100001", 0, 6),
    ("00100101", 1, 3),
    ("00100100", 3, 2),
    ("00100111", 10, 1),
    ("00100011", 11, 1),
    ("00100010", 12, 1),
    ("00100000", 13, 1),
    ("0000001010", 0, 7),
    ("0000001100", 1, 4),
    ("0000001011", 2, 3),
    ("0000001111", 4, 2),
    ("0000001001", 5, 2),
    ("0000001110", 14, 1),
    ("0000001101", 15, 1),
    ("0000001000", 16, 1),
    ("000000011101", 0, 8),
    ("000000011000", 0, 9),
    ("000000010011", 0, 10),
    ("000000010000", 0, 11),
    ("000000011011", 1, 5),
    ("000000010100", 2, 4),
    ("000000011100", 3, 3),
    ("000000010010", 4, 3),
    ("000000011110", 6, 2),
    ("000000010101", 7, 2),
    ("000000010001", 8, 2),
    ("000000011111", 17, 1),
    ("000000011010", 18, 1),
    ("000000011001", 19, 1),
    ("000000010111", 20, 1),
    ("000000010110", 21, 1),
    ("0000000011010", 0, 12),
    ("0000000011001", 0, 13),
    ("0000000011000", 0, 14),
    ("0000000010111", 0, 15),
    ("0000000010110", 1, 6),
    ("0000000010101", 1, 7),
    ("0000000010100", 2, 5),
    ("0000000010011", 3, 4),
    ("0000000010010", 5, 3),
    ("0000000010001", 9, 2),
    ("0000000010000", 10, 2),
    ("0000000011111", 22, 1),
    ("0000000011110", 23, 1),
    ("0000000011101", 24, 1),
    ("0000000011100", 25, 1),
    ("0000000011011", 26, 1),
    ("00000000011111", 0, 16),
    ("00000000011110", 0, 17),
    ("00000000011101", 0, 18),
    ("00000000011100", 0, 19),
    ("00000000011011", 0, 20),
    ("00000000011010", 0, 21),
    ("00000000011001", 0, 22),
    ("00000000011000", 0, 23),
    ("00000000010111", 0, 24),
    ("00000000010110", 0, 25),
    ("00000000010101", 0, 26),
    ("00000000010100", 0, 27),
    ("00000000010011", 0, 28),
    ("00000000010010", 0, 29),
    ("00000000010001", 0, 30),
    ("00000000010000", 0, 31),
    ("000000000011000", 0, 32),
    ("000000000010111", 0, 33),
    ("000000000010110", 0, 34),
    ("000000000010101", 0, 35),
    ("000000000010100", 0, 36),
    ("000000000010011", 0, 37),
    ("000000000010010", 0, 38),
    ("000000000010001", 0, 39),
    ("000000000010000", 0, 40),
    ("000000000011111", 1, 8),
    ("000000000011110", 1, 9),
    ("000000000011101", 1, 10),
    ("000000000011100", 1, 11),
    ("000000000011011", 1, 12),
    ("000000000011010", 1, 13),
    ("000000000011001", 1, 14),
    ("0000000000010011", 1, 15),
    ("0000000000010010", 1, 16),
    ("0000000000010001", 1, 17),
    ("0000000000010000", 1, 18),
    ("0000000000010100", 6, 3),
    ("0000000000011010", 11, 2),
    ("0000000000011001", 12, 2),
    ("0000000000011000", 13, 2),
    ("0000000000010111", 14, 2),
    ("0000000000010110", 15, 2),
    ("0000000000010101", 16, 2),
    ("0000000000011111", 27, 1),
    ("0000000000011110", 28, 1),
    ("0000000000011101", 29, 1),
    ("0000000000011100", 30, 1),
    ("0000000000011011", 31, 1),
];

const K_EOB: u32 = 1 << 30;
const K_ESC: u32 = 1 << 29;

/// 16-bit prefix -> len | run << 8 | level << 16 (0 = invalid).
fn table() -> &'static [u32] {
    static T: OnceLock<Vec<u32>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = vec![0u32; 1 << 16];
        let mut put = |bits: &str, v: u32| {
            let len = bits.len();
            let code = u32::from_str_radix(bits, 2).unwrap();
            let base = (code << (16 - len)) as usize;
            for e in &mut t[base..base + (1 << (16 - len))] {
                assert_eq!(*e, 0, "overlapping code {bits}");
                *e = v | len as u32;
            }
        };
        put("10", K_EOB);
        put("000001", K_ESC);
        for &(bits, run, level) in AC {
            put(bits, (run as u32) << 8 | (level as u32) << 16);
        }
        t
    })
}

struct Bits<'a> {
    d: &'a [u8],
    /// Bit position (in the 16-bit-word MSB-first order).
    pos: usize,
}

impl Bits<'_> {
    #[inline]
    fn bit(&self, p: usize) -> u32 {
        let w = p / 16;
        let i = w * 2;
        if i + 1 >= self.d.len() {
            return 0;
        }
        let v = u16::from_le_bytes([self.d[i], self.d[i + 1]]);
        ((v >> (15 - (p % 16))) & 1) as u32
    }
    #[inline]
    fn peek(&self, n: usize) -> u32 {
        (0..n).fold(0, |a, k| a << 1 | self.bit(self.pos + k))
    }
    #[inline]
    fn take(&mut self, n: usize) -> u32 {
        let v = self.peek(n);
        self.pos += n;
        v
    }
}

/// A frame's bitstream header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub mdec_words: u16,
    pub qscale: u16,
    pub version: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Short,
    Version(u16),
    BadCode { block: usize, bit: usize },
}

pub fn header(frame: &[u8]) -> Result<Header, Error> {
    if frame.len() < 8 {
        return Err(Error::Short);
    }
    let h = |o: usize| u16::from_le_bytes([frame[o], frame[o + 1]]);
    let hd = Header { mdec_words: h(0), qscale: h(4), version: h(6) };
    if h(2) != 0x3800 || hd.version != 2 {
        return Err(Error::Version(hd.version));
    }
    Ok(hd)
}

/// Decode `blocks` blocks (6 per macroblock) of a version-2 frame into MDEC
/// halfwords (appended to `out`). Returns the header.
pub fn decode(frame: &[u8], blocks: usize, out: &mut Vec<u16>) -> Result<Header, Error> {
    let hd = header(frame)?;
    let t = table();
    let mut b = Bits { d: &frame[8..], pos: 0 };
    let q = (hd.qscale & 0x3F) << 10;
    for blk in 0..blocks {
        if b.pos / 8 >= b.d.len() {
            return Err(Error::Short);
        }
        out.push(q | (b.take(10) as u16 & 0x3FF));
        loop {
            let e = t[b.peek(16) as usize];
            if e == 0 {
                return Err(Error::BadCode { block: blk, bit: b.pos });
            }
            b.pos += (e & 0xFF) as usize;
            if e & K_EOB != 0 {
                out.push(crate::mdec::EOB);
                break;
            }
            if e & K_ESC != 0 {
                let run = b.take(6) as u16;
                let level = b.take(10) as u16;
                out.push(run << 10 | level);
                continue;
            }
            let run = ((e >> 8) & 0x3F) as u16;
            let mut level = ((e >> 16) & 0x3FF) as i32;
            if b.take(1) == 1 {
                level = -level;
            }
            out.push(run << 10 | (level as u16 & 0x3FF));
        }
    }
    Ok(hd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_prefix_free_and_complete_enough() {
        let t = table();
        // every 16-bit prefix not starting with the 16 zero bits decodes
        let unused = t.iter().filter(|&&e| e == 0).count();
        // only prefixes 0000 0000 0000 xxxx (below 0000 0000 0001 0000) are free
        assert_eq!(unused, 16);
    }

    #[test]
    fn small_stream() {
        // header: 1 word, 0x3800, qscale 1, version 2; one block:
        // DC 5 (0000000101), "11" + "0" (run 0, +1), "011" + "1" (run 1, -1),
        // escape 000001 + run 3 (000011) + level -2 (1111111110), EOB "10".
        let bits = "0000000101" .to_string() + "110" + "0111" + "000001" + "000011" + "1111111110" + "10";
        let mut words = vec![0u16; bits.len().div_ceil(16)];
        for (i, c) in bits.chars().enumerate() {
            if c == '1' {
                words[i / 16] |= 1 << (15 - i % 16);
            }
        }
        let mut f = vec![1, 0, 0, 0x38, 1, 0, 2, 0];
        for w in words {
            f.extend(w.to_le_bytes());
        }
        let mut out = Vec::new();
        decode(&f, 1, &mut out).unwrap();
        assert_eq!(out, vec![1 << 10 | 5, 1, 1 << 10 | 0x3FF, 3 << 10 | 0x3FE, 0xFE00]);
    }
}
