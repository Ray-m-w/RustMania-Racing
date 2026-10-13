//! A 24-voice model of the PS1 SPU (psx-spx "Sound Processing Unit"), enough
//! for the game's sound effects: 512 KB sound RAM, per-voice ADPCM decoding
//! with loop flags, pitch counter with 4-point interpolation, ADSR envelope,
//! fixed left/right volumes, key on / key off. 44100 Hz stereo out.
//!
//! What the game does not use is not modelled (docs/spec/audio.md, ledger):
//! reverb (SPUCNT 0xC001 in a race: reverb master off), noise and pitch
//! modulation (NON = PMON = 0), volume sweeps (libsnd writes fixed volumes),
//! the SPU IRQ, capture buffers. CD audio is mixed by the music side.
//!
//! Interpolation: the hardware uses a 512-entry Gaussian table (SPU ROM;
//! not in the executable or on the disc). `gauss()` builds a Gaussian kernel
//! with the same 4-tap, 256-phase structure (sigma fitted to the table's
//! centre tap, ~0.70 of the sum); a deliberate, ledgered approximation.

use crate::adpcm::{self, BLOCK_BYTES, BLOCK_SAMPLES};

pub const RAM_SIZE: usize = 512 * 1024;
pub const VOICES: usize = 24;
pub const RATE: u32 = 44_100;

/// Envelope phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Register-level state of one voice plus its decoder.
#[derive(Clone, Debug, Default)]
pub struct Voice {
    /// 0x1F801C00 + 16v: VOL_L, VOL_R (fixed mode: bits 0-14 = volume / 2).
    pub vol_l: u16,
    pub vol_r: u16,
    /// +4: pitch (0x1000 = 44100 Hz; capped at 0x4000 when stepping).
    pub pitch: u16,
    /// +6: start address / 8.
    pub start: u16,
    /// +8 / +A: ADSR.
    pub adsr1: u16,
    pub adsr2: u16,
    /// +E: repeat address / 8 (latched from a loop-start block).
    pub repeat: u16,
    /// Envelope level 0..0x7FFF (ENVX).
    pub env: i32,
    pub phase: Phase,
    /// ADSR wait counter (cycles until the next step).
    env_wait: i32,
    /// Current block address (bytes).
    cur: u32,
    /// Pitch counter: bits 12+ = samples consumed in the block; bits 4-11 =
    /// interpolation phase.
    counter: u32,
    /// Decoded samples of the current block, preceded by the last 3 of the
    /// previous block (interpolation history).
    buf: [i16; BLOCK_SAMPLES + 3],
    hist: adpcm::History,
    /// Flags of the current block.
    flags: u8,
    /// Set when the voice reached an END block (ENDX bit).
    pub endx: bool,
}

/// One sample-accurate SPU.
#[derive(Clone)]
pub struct Spu {
    pub ram: Vec<u8>,
    pub voices: Vec<Voice>,
    /// Main volume (fixed mode), both sides.
    pub main_l: u16,
    pub main_r: u16,
    /// Apply the main volume in `render` (false: the voice sum only, for a
    /// mixer that applies the main volume to voices + CD audio itself).
    pub apply_main: bool,
    gauss: Vec<[i32; 4]>,
}

impl Default for Spu {
    fn default() -> Self {
        Self::new()
    }
}

/// Interpolation weights per phase (256 phases x 4 taps, oldest first),
/// summing to 0x7F80 (the hardware table's sums are ~0x7F7F..0x8000).
pub fn gauss() -> Vec<[i32; 4]> {
    let sigma = 0.57f64;
    (0..256)
        .map(|p| {
            let f = p as f64 / 256.0;
            // Samples at positions 0..3 (oldest first); the output point
            // lies between samples 1 and 2 at 1 + f.
            let d = [1.0 + f, f, 1.0 - f, 2.0 - f];
            let w: Vec<f64> = d.iter().map(|x| (-(x * x) / (2.0 * sigma * sigma)).exp()).collect();
            let s: f64 = w.iter().sum();
            let mut out = [0i32; 4];
            for i in 0..4 {
                out[i] = (w[i] / s * 32640.0).round() as i32;
            }
            out
        })
        .collect()
}

impl Spu {
    pub fn new() -> Spu {
        Spu { ram: vec![0; RAM_SIZE], voices: vec![Voice::default(); VOICES], main_l: 0x3FFF, main_r: 0x3FFF, apply_main: true, gauss: gauss() }
    }

    /// DMA write into sound RAM (`addr` in bytes).
    pub fn write_ram(&mut self, addr: usize, data: &[u8]) {
        let end = (addr + data.len()).min(RAM_SIZE);
        if addr < end {
            self.ram[addr..end].copy_from_slice(&data[..end - addr]);
        }
    }

    /// Key on the voices in `mask` (KON): envelope from 0 in attack, decoding
    /// from the start address, ENDX cleared.
    pub fn key_on(&mut self, mask: u32) {
        for v in 0..VOICES {
            if mask & (1 << v) != 0 {
                let vo = &mut self.voices[v];
                vo.env = 0;
                vo.env_wait = 0;
                vo.phase = Phase::Attack;
                vo.cur = vo.start as u32 * 8;
                vo.counter = 0;
                vo.hist = adpcm::History::default();
                vo.buf = [0; BLOCK_SAMPLES + 3];
                vo.endx = false;
                Self::load_block(&self.ram, vo, true);
            }
        }
    }

    /// Key off (KOFF): release.
    pub fn key_off(&mut self, mask: u32) {
        for v in 0..VOICES {
            if mask & (1 << v) != 0 && self.voices[v].phase != Phase::Off {
                self.voices[v].phase = Phase::Release;
                self.voices[v].env_wait = 0;
            }
        }
    }

    fn load_block(ram: &[u8], vo: &mut Voice, first: bool) {
        let a = (vo.cur as usize) & (RAM_SIZE - 1) & !(BLOCK_BYTES - 1);
        let blk = &ram[a..a + BLOCK_BYTES];
        vo.flags = adpcm::flags(blk);
        if vo.flags & adpcm::FLAG_START != 0 {
            vo.repeat = (a / 8) as u16;
        }
        if !first {
            let n = BLOCK_SAMPLES;
            vo.buf.copy_within(n..n + 3, 0);
        }
        let mut out = [0i16; BLOCK_SAMPLES];
        adpcm::decode_block(blk, &mut vo.hist, &mut out);
        vo.buf[3..].copy_from_slice(&out);
    }

    /// One ADSR step for a voice (psx-spx "SPU ADSR": cycles = 1 << max(0,
    /// shift - 11), step << max(0, 11 - shift), exponential increase above
    /// 0x6000 four times slower, exponential decrease scaled by the level).
    fn adsr_tick(vo: &mut Voice) {
        let (exp, dec, shift, step) = match vo.phase {
            Phase::Off => return,
            Phase::Attack => (vo.adsr1 & 0x8000 != 0, false, ((vo.adsr1 >> 10) & 0x1F) as i32, 7 - ((vo.adsr1 >> 8) & 3) as i32),
            Phase::Decay => (true, true, ((vo.adsr1 >> 4) & 0xF) as i32, -8),
            Phase::Sustain => {
                let dec = vo.adsr2 & 0x4000 != 0;
                let st = ((vo.adsr2 >> 6) & 3) as i32;
                (vo.adsr2 & 0x8000 != 0, dec, ((vo.adsr2 >> 8) & 0x1F) as i32, if dec { -8 + st } else { 7 - st })
            }
            Phase::Release => (vo.adsr2 & 0x20 != 0, true, (vo.adsr2 & 0x1F) as i32, -8),
        };
        if vo.env_wait > 0 {
            vo.env_wait -= 1;
            return;
        }
        let mut cycles = 1i32 << (shift - 11).max(0);
        let mut st = step << (11 - shift).max(0);
        if exp && !dec && vo.env > 0x6000 {
            cycles *= 4;
        }
        if exp && dec {
            st = st * vo.env >> 15;
        }
        vo.env_wait = cycles - 1;
        vo.env = (vo.env + st).clamp(0, 0x7FFF);
        match vo.phase {
            Phase::Attack if vo.env >= 0x7FFF => vo.phase = Phase::Decay,
            Phase::Decay => {
                let sl = (((vo.adsr1 & 0xF) as i32) + 1) * 0x800;
                if vo.env <= sl {
                    vo.phase = Phase::Sustain;
                }
            }
            Phase::Release if vo.env == 0 => vo.phase = Phase::Off,
            _ => {}
        }
    }

    fn vol(reg: u16) -> i32 {
        if reg & 0x8000 != 0 {
            // Sweep mode is not modelled; libsnd writes fixed volumes.
            0x7FFF
        } else {
            ((reg << 1) as i16) as i32
        }
    }

    /// One output sample of one voice (before volume), advancing it.
    fn voice_sample(ram: &[u8], g: &[[i32; 4]], vo: &mut Voice) -> i32 {
        if vo.phase == Phase::Off {
            return 0;
        }
        let i = (vo.counter >> 12) as usize;
        let ph = ((vo.counter >> 4) & 0xFF) as usize;
        let w = &g[ph];
        let s = (w[0] * vo.buf[i] as i32 + w[1] * vo.buf[i + 1] as i32 + w[2] * vo.buf[i + 2] as i32 + w[3] * vo.buf[i + 3] as i32) >> 15;
        let out = s * vo.env >> 15;
        Self::adsr_tick(vo);
        vo.counter += (vo.pitch as u32).min(0x4000);
        while (vo.counter >> 12) as usize >= BLOCK_SAMPLES {
            vo.counter -= (BLOCK_SAMPLES as u32) << 12;
            if vo.flags & adpcm::FLAG_END != 0 {
                vo.endx = true;
                vo.cur = vo.repeat as u32 * 8;
                if vo.flags & adpcm::FLAG_REPEAT == 0 {
                    vo.phase = Phase::Off;
                    vo.env = 0;
                }
            } else {
                vo.cur = vo.cur.wrapping_add(BLOCK_BYTES as u32);
            }
            Self::load_block(ram, vo, false);
        }
        out
    }

    /// Render `n` stereo frames, interleaved L R, appended to `out`.
    pub fn render(&mut self, n: usize, out: &mut Vec<i16>) {
        let ml = Self::vol(self.main_l);
        let mr = Self::vol(self.main_r);
        for _ in 0..n {
            let (mut l, mut r) = (0i32, 0i32);
            for vo in self.voices.iter_mut() {
                let s = Self::voice_sample(&self.ram, &self.gauss, vo);
                if s != 0 {
                    l += s * Self::vol(vo.vol_l) >> 15;
                    r += s * Self::vol(vo.vol_r) >> 15;
                }
            }
            l = l.clamp(-0x8000, 0x7FFF);
            r = r.clamp(-0x8000, 0x7FFF);
            if self.apply_main {
                l = (l * ml) >> 15;
                r = (r * mr) >> 15;
            }
            out.push(l as i16);
            out.push(r as i16);
        }
    }

    /// ENVX of a voice (what libsnd's SpuGetVoiceEnvelope reads).
    pub fn envelope(&self, v: usize) -> i16 {
        self.voices[v].env as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_sums() {
        for w in gauss() {
            let s: i32 = w.iter().sum();
            assert!((s - 32640).abs() <= 2, "{s}");
        }
        // Phase 0: the centre tap (sample 1) dominates.
        let g = gauss();
        assert!(g[0][1] > 20000 && g[0][1] < 25000);
    }

    fn square_bank(spu: &mut Spu, at: usize, blocks: usize, looped: bool) {
        for b in 0..blocks {
            let mut blk = [0u8; 16];
            blk[0] = 0; // shift 0, filter 0: nibble 1 -> 0x1000
            for i in 2..16 {
                blk[i] = if b % 2 == 0 { 0x11 } else { 0xFF };
            }
            if b == 0 {
                blk[1] |= adpcm::FLAG_START;
            }
            if b == blocks - 1 {
                blk[1] |= adpcm::FLAG_END | if looped { adpcm::FLAG_REPEAT } else { 0 };
            }
            spu.write_ram(at + 16 * b, &blk);
        }
    }

    fn voice(spu: &mut Spu, v: usize, at: usize) {
        let vo = &mut spu.voices[v];
        vo.start = (at / 8) as u16;
        vo.pitch = 0x1000;
        vo.vol_l = 0x3FFF;
        vo.vol_r = 0x2000;
        vo.adsr1 = 0x00FF; // fastest linear attack, sustain level max
        vo.adsr2 = 0x0000;
    }

    #[test]
    fn one_shot_ends() {
        let mut spu = Spu::new();
        square_bank(&mut spu, 0x1000, 2, false);
        voice(&mut spu, 3, 0x1000);
        spu.key_on(1 << 3);
        let mut out = Vec::new();
        spu.render(56 + 10, &mut out);
        assert!(spu.voices[3].endx);
        assert_eq!(spu.voices[3].phase, Phase::Off);
        assert!(out.iter().any(|&s| s != 0));
        // After the end: silence.
        out.clear();
        spu.render(10, &mut out);
        assert!(out.iter().all(|&s| s == 0));
    }

    #[test]
    fn loop_keeps_playing_and_pans() {
        let mut spu = Spu::new();
        square_bank(&mut spu, 0x2000, 2, true);
        voice(&mut spu, 0, 0x2000);
        spu.key_on(1);
        let mut out = Vec::new();
        spu.render(2000, &mut out);
        assert!(spu.voices[0].endx);
        assert_ne!(spu.voices[0].phase, Phase::Off);
        assert_eq!(spu.voices[0].repeat, 0x2000 / 8);
        let tail = &out[out.len() - 400..];
        let l: i64 = tail.iter().step_by(2).map(|&s| (s as i64).abs()).sum();
        let r: i64 = tail.iter().skip(1).step_by(2).map(|&s| (s as i64).abs()).sum();
        assert!(l > 0 && r > 0 && l > r, "{l} {r}");
        // Key off: release to silence.
        spu.voices[0].adsr2 = 0x0000; // release shift 0: fastest
        spu.key_off(1);
        out.clear();
        spu.render(4000, &mut out);
        assert_eq!(spu.voices[0].phase, Phase::Off);
    }

    #[test]
    fn pitch_steps_samples() {
        // Pitch 0x800: half speed -> 28 samples take 56 output frames.
        let mut spu = Spu::new();
        square_bank(&mut spu, 0x3000, 4, false);
        voice(&mut spu, 1, 0x3000);
        spu.voices[1].pitch = 0x800;
        spu.key_on(2);
        let mut out = Vec::new();
        spu.render(56 * 4 - 1, &mut out);
        assert!(!spu.voices[1].endx);
        spu.render(1, &mut out);
        assert!(spu.voices[1].endx);
    }

    #[test]
    fn adsr_attack_reaches_decay() {
        let mut vo = Voice { adsr1: 0x7F00 | 0x0F, phase: Phase::Attack, ..Default::default() };
        // attack shift 0x1F (slowest) vs shift 0: shift 0 step = 7 << 11.
        vo.adsr1 = 0x000F;
        for _ in 0..3 {
            Spu::adsr_tick(&mut vo);
        }
        assert_eq!(vo.env, 0x7FFF);
        assert_eq!(vo.phase, Phase::Decay);
    }
}
