//! The PS1 SPU ADPCM block (psx-spx "SPU ADPCM Samples"): 16 bytes ->
//! 28 signed 16-bit samples. Byte 0: shift (bits 0-3) and filter (bits 4-6);
//! byte 1: loop flags; bytes 2..16: 28 nibbles, low nibble first.
//! Shared by the VB sample bank (vab.rs) and the SPU voice model (spu.rs).
//!
//! Decoding (psx-spx): for each nibble t (sign-extended 4 bits),
//! `s = (t << 12) >> shift` (shift 13..15 behave as 9, the hardware's
//! "reserved" values), then `s += (old*f0 + older*f1 + 32) >> 6` with the
//! filter pair `f0, f1`, clamped to i16.

/// Bytes per block.
pub const BLOCK_BYTES: usize = 16;
/// Samples per block.
pub const BLOCK_SAMPLES: usize = 28;

/// Flag bit 0: end of the sample (jump to the repeat address).
pub const FLAG_END: u8 = 1;
/// Flag bit 1: with END, keep playing from the repeat address (loop);
/// END without it releases the voice (ADSR release, envelope to 0).
pub const FLAG_REPEAT: u8 = 2;
/// Flag bit 2: loop start (the voice latches this block as its repeat address).
pub const FLAG_START: u8 = 4;

/// The SPU's filter coefficients (positive and negative halves; psx-spx).
const POS: [i32; 5] = [0, 60, 115, 98, 122];
const NEG: [i32; 5] = [0, 0, -52, -55, -60];

/// Decoder history: the last two output samples (old, older).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct History {
    pub old: i16,
    pub older: i16,
}

/// Decode one 16-byte block into 28 samples, updating the history.
pub fn decode_block(block: &[u8], h: &mut History, out: &mut [i16; BLOCK_SAMPLES]) {
    let mut shift = (block[0] & 0x0F) as u32;
    if shift > 12 {
        shift = 9;
    }
    let filter = (((block[0] >> 4) & 7) as usize).min(4);
    let (f0, f1) = (POS[filter], NEG[filter]);
    let mut old = h.old as i32;
    let mut older = h.older as i32;
    for (i, o) in out.iter_mut().enumerate() {
        let byte = block[2 + i / 2];
        let nib = if i & 1 == 0 { byte & 0x0F } else { byte >> 4 };
        // Sign-extend the nibble into the top of a 16-bit word, then shift.
        let t = ((nib as i16) << 12) as i32 >> shift;
        let s = (t + ((old * f0 + older * f1 + 32) >> 6)).clamp(-0x8000, 0x7FFF);
        *o = s as i16;
        older = old;
        old = s;
    }
    h.old = old as i16;
    h.older = older as i16;
}

/// The loop flags of a block.
pub fn flags(block: &[u8]) -> u8 {
    block[1]
}

/// A decoded sample: PCM plus where the loop starts (in samples), if it loops.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decoded {
    pub pcm: Vec<i16>,
    /// Sample index of the block flagged loop start that the END+REPEAT
    /// block jumps back to; None for a one-shot.
    pub loop_start: Option<usize>,
}

/// Decode a sample from its first block to the first block with the END
/// flag (or the end of `data`). Loop start = the last START-flagged block
/// seen before the END block, as the voice would latch it.
pub fn decode_sample(data: &[u8]) -> Decoded {
    let mut h = History::default();
    let mut pcm = Vec::new();
    let mut loop_blk = None;
    let mut buf = [0i16; BLOCK_SAMPLES];
    for (bi, b) in data.chunks_exact(BLOCK_BYTES).enumerate() {
        let f = flags(b);
        if f & FLAG_START != 0 {
            loop_blk = Some(bi);
        }
        decode_block(b, &mut h, &mut buf);
        pcm.extend_from_slice(&buf);
        if f & FLAG_END != 0 {
            let loops = f & FLAG_REPEAT != 0;
            return Decoded { pcm, loop_start: if loops { loop_blk.map(|b| b * BLOCK_SAMPLES) } else { None } };
        }
    }
    Decoded { pcm, loop_start: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_block_is_silence() {
        let mut h = History::default();
        let mut out = [1i16; BLOCK_SAMPLES];
        decode_block(&[0u8; 16], &mut h, &mut out);
        assert!(out.iter().all(|&s| s == 0));
    }

    #[test]
    fn shift_and_sign() {
        // shift 12, filter 0: nibble 1 -> 1, nibble 0xF -> -1, nibble 8 -> -8.
        let mut b = [0u8; 16];
        b[0] = 12;
        b[2] = 0xF1;
        b[3] = 0x08;
        let mut h = History::default();
        let mut out = [0i16; BLOCK_SAMPLES];
        decode_block(&b, &mut h, &mut out);
        assert_eq!(&out[..4], &[1, -1, -8, 0]);
        // shift 0: nibble 7 -> 0x7000.
        b[0] = 0;
        b[2] = 0x07;
        decode_block(&b, &mut History::default(), &mut out);
        assert_eq!(out[0], 0x7000);
    }

    #[test]
    fn filter_uses_history() {
        // filter 1 (60/64 of the previous sample), all-zero nibbles.
        let mut b = [0u8; 16];
        b[0] = 0x10;
        let mut h = History { old: 6400, older: 0 };
        let mut out = [0i16; BLOCK_SAMPLES];
        decode_block(&b, &mut h, &mut out);
        assert_eq!(out[0], ((6400 * 60 + 32) >> 6) as i16);
        assert_eq!(out[1], ((out[0] as i32 * 60 + 32) >> 6) as i16);
        // filter 2 with both taps.
        b[0] = 0x20;
        let mut h = History { old: 1000, older: 500 };
        decode_block(&b, &mut h, &mut out);
        assert_eq!(out[0] as i32, (1000 * 115 + 500 * -52 + 32) >> 6);
    }

    #[test]
    fn clamps() {
        let mut b = [0u8; 16];
        b[0] = 0x40; // filter 4, shift 0
        b[2] = 0x77;
        let mut h = History { old: 32000, older: -32000 };
        let mut out = [0i16; BLOCK_SAMPLES];
        decode_block(&b, &mut h, &mut out);
        assert_eq!(out[0], 0x7FFF);
    }

    #[test]
    fn loop_flags() {
        let mut d = vec![0u8; 48];
        d[1] = FLAG_START;
        d[16 + 1] = 0;
        d[32 + 1] = FLAG_END | FLAG_REPEAT;
        let s = decode_sample(&d);
        assert_eq!(s.pcm.len(), 84);
        assert_eq!(s.loop_start, Some(0));
        d[32 + 1] = FLAG_END;
        assert_eq!(decode_sample(&d).loop_start, None);
        // Stops at the first END.
        d[16 + 1] = FLAG_END;
        assert_eq!(decode_sample(&d).pcm.len(), 56);
    }
}
