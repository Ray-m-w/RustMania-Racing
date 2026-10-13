//! VAB sample bank: SFX/SFX_6.VH (header) + SFX/SFX_6.VB (ADPCM body), the
//! only sample bank on the disc (the game loads it once, FUN_8005EA94 from
//! FUN_8004321C: "sfx\sfx_6" + ".VH"/".VB"; it opens with SsVabOpenHead and
//! transfers the body with SsVabTransBody).
//!
//! VH layout (PSYQ libsnd, `VabHdr` / `ProgAtr` / `VagAtr`):
//! - 0x00 header, 0x20 bytes: magic "pBAV" (0x56414270), version (6 here),
//!   vab id, total size, reserved, ps (programs), ts (tones), vs (VAGs),
//!   mvol, pan, attr1, attr2, reserved.
//! - 0x20: 128 x ProgAtr (16 bytes): tones, mvol, prior, mode, mpan, ...
//! - 0x820: ps x 16 x VagAtr (32 bytes): prior, mode, vol, pan, center,
//!   shift, min, max, vibW, vibT, porW, porT, pbmin, pbmax, -, -, adsr1 (u16
//!   at 0x10), adsr2 (0x12), prog (0x14), vag (0x16).
//! - then 256 x u16 VAG sizes (in 8-byte units from version 5 on; entry 0
//!   is the empty VAG 0).
//!
//! libsnd (`_SsVabOpenHeadWithMode` 0x8009E110) looks tones up as VagAtr
//! number `tone + 16 * index_of_program`, where the program index counts
//! only programs with tones (written into ProgAtr byte 8). The game always
//! uses program 0, so its tone numbers (sound id - 1, up to 87) run through
//! the VagAtr blocks of all programs in order. VAG `v` (1-based) starts at
//! `base + sum(size[0..v])` in SPU RAM, where `base` is the SpuMalloc'ed
//! address of the body.

use crate::adpcm;

pub const MAGIC: u32 = 0x5641_4270;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub tones: u8,
    pub mvol: u8,
    pub prior: u8,
    pub mode: u8,
    pub mpan: u8,
    /// libsnd's index among the programs that have tones (ProgAtr byte 8
    /// after SsVabOpenHead): the program's first VagAtr block.
    pub tone_base: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tone {
    pub prior: u8,
    pub mode: u8,
    pub vol: u8,
    pub pan: u8,
    pub center: u8,
    pub shift: u8,
    pub min: u8,
    pub max: u8,
    pub adsr1: u16,
    pub adsr2: u16,
    pub prog: i16,
    /// VAG number (1-based; 0 = none).
    pub vag: i16,
}

#[derive(Clone, Debug, Default)]
pub struct Vab {
    pub version: u32,
    pub mvol: u8,
    pub pan: u8,
    pub programs: Vec<Program>,
    /// Every VagAtr in file order (ps x 16).
    pub tones: Vec<Tone>,
    /// VAG sizes in bytes, entries 0..=vs (entry 0 = 0).
    pub vag_sizes: Vec<u32>,
    /// The VB body (ADPCM blocks of all VAGs back to back).
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum VabError {
    Magic(u32),
    Short,
}

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

impl Vab {
    /// Parse a VH header and attach its VB body.
    pub fn parse(vh: &[u8], vb: Vec<u8>) -> Result<Vab, VabError> {
        if vh.len() < 0x20 {
            return Err(VabError::Short);
        }
        let magic = u32le(vh, 0);
        if magic != MAGIC {
            return Err(VabError::Magic(magic));
        }
        let version = u32le(vh, 4);
        let ps = u16le(vh, 0x12) as usize;
        let vs = u16le(vh, 0x16) as usize;
        let mvol = vh[0x18];
        let pan = vh[0x19];
        let tones_off = 0x20 + 0x800;
        let sizes_off = tones_off + ps * 16 * 32;
        if vh.len() < sizes_off + 512 {
            return Err(VabError::Short);
        }
        let mut programs = Vec::with_capacity(128);
        let mut idx = 0u16;
        for p in 0..128 {
            let o = 0x20 + p * 16;
            let pr = Program { tones: vh[o], mvol: vh[o + 1], prior: vh[o + 2], mode: vh[o + 3], mpan: vh[o + 4], tone_base: idx };
            if pr.tones != 0 {
                idx += 1;
            }
            programs.push(pr);
        }
        let tones = (0..ps * 16)
            .map(|t| {
                let o = tones_off + t * 32;
                Tone {
                    prior: vh[o],
                    mode: vh[o + 1],
                    vol: vh[o + 2],
                    pan: vh[o + 3],
                    center: vh[o + 4],
                    shift: vh[o + 5],
                    min: vh[o + 6],
                    max: vh[o + 7],
                    adsr1: u16le(vh, o + 0x10),
                    adsr2: u16le(vh, o + 0x12),
                    prog: u16le(vh, o + 0x14) as i16,
                    vag: u16le(vh, o + 0x16) as i16,
                }
            })
            .collect();
        let unit = if version > 4 { 8 } else { 4 };
        let vag_sizes = (0..=vs).map(|i| u16le(vh, sizes_off + 2 * i) as u32 * unit).collect();
        Ok(Vab { version, mvol, pan, programs, tones, vag_sizes, body: vb })
    }

    /// The VagAtr libsnd uses for (program, tone).
    pub fn tone(&self, prog: usize, tone: usize) -> Option<&Tone> {
        let p = self.programs.get(prog)?;
        self.tones.get(tone + 16 * p.tone_base as usize)
    }

    /// Byte offset of VAG `v` (1-based) inside the body, and its size.
    pub fn vag_range(&self, v: usize) -> Option<(usize, usize)> {
        if v == 0 || v >= self.vag_sizes.len() {
            return None;
        }
        let off: u32 = self.vag_sizes[..v].iter().sum();
        Some((off as usize, self.vag_sizes[v] as usize))
    }

    /// SPU RAM address of VAG `v` when the body sits at `base`, in 8-byte
    /// units as libsnd stores it (`(base + sum) >> 3`).
    pub fn vag_spu_addr(&self, base: u32, v: usize) -> Option<u16> {
        self.vag_range(v).map(|(o, _)| ((base + o as u32) >> 3) as u16)
    }

    /// Bytes SpuMalloc is asked for: the body size rounded up to 64.
    pub fn alloc_size(&self) -> u32 {
        (self.vag_sizes.iter().sum::<u32>() + 0x3F) & !0x3F
    }

    /// Decode VAG `v` to PCM.
    pub fn decode_vag(&self, v: usize) -> Option<adpcm::Decoded> {
        let (o, n) = self.vag_range(v)?;
        let end = (o + n).min(self.body.len());
        Some(adpcm::decode_sample(&self.body[o.min(end)..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth() -> Vec<u8> {
        // 2 programs (0 empty, 1 with 2 tones), 2 VAGs of 32 and 16 bytes.
        let ps = 2usize;
        let mut vh = vec![0u8; 0x20 + 0x800 + ps * 0x200 + 0x200];
        vh[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        vh[4] = 7;
        vh[0x12] = ps as u8;
        vh[0x14] = 2;
        vh[0x16] = 2;
        vh[0x18] = 0x7F;
        vh[0x20 + 16] = 2; // program 1 has tones
        vh[0x20 + 16 + 1] = 0x70;
        let t0 = 0x820; // VagAtr 0 (program index 0 = program 1)
        vh[t0 + 2] = 0x60;
        vh[t0 + 4] = 60;
        vh[t0 + 0x16] = 2;
        let s = 0x820 + ps * 0x200;
        vh[s + 2] = 4; // VAG 1: 32 bytes
        vh[s + 4] = 2; // VAG 2: 16 bytes
        vh
    }

    #[test]
    fn parse_layout() {
        let v = Vab::parse(&synth(), vec![0; 48]).unwrap();
        assert_eq!(v.programs[1].tones, 2);
        assert_eq!(v.programs[1].tone_base, 0);
        assert_eq!(v.programs[2].tone_base, 1);
        let t = v.tone(1, 0).unwrap();
        assert_eq!((t.vol, t.center, t.vag), (0x60, 60, 2));
        assert_eq!(v.vag_range(1), Some((0, 32)));
        assert_eq!(v.vag_range(2), Some((32, 16)));
        assert_eq!(v.vag_spu_addr(0x1010, 2), Some(((0x1010 + 32) >> 3) as u16));
        assert_eq!(v.alloc_size(), 64);
        assert!(Vab::parse(&[0u8; 0x40], vec![]).is_err());
    }
}
