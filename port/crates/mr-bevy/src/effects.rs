//! Effects: the game's billboards and streaks in 3D (docs/spec/effects.md).
//!
//! What the race render pass FUN_80029F40 draws besides scenery and karts:
//! - sprite particles 0x800E0C20 (FUN_80010E90): smoke, flames, stars,
//!   explosion puffs, splashes; camera-facing quads (flag 0x10: flat on the
//!   ground) from the sprite definitions *0x800D1B40 (exe 0x800ACF40),
//!   animated by age, rotated, faded (mr_sim::particles);
//! - spark streaks 0x800B4540 (FUN_80032928 -> FUN_80032120): additive
//!   gouraud lines from black at the spark to its colour at pos + vel * len;
//! - pickups (FUN_8007E21C): turbo tokens 0x800E2370 (type table 0x800AC1B8,
//!   spun per group by DAT_800B0B90 + the camera roll) and item boxes
//!   0x800E1B70 (the star sprite, slot 0x15 clut 0xB, size 2 * scale);
//! - projectiles 0x800DDC50 (FUN_80030D78): the weapon sprites (flags 0x40
//!   wobbling star, 1, default; 0x400 = a 3D model, not drawn here).
//! The billboards FUN_80030474 queues into the 0x800DF688 batch and
//! FUN_80011E3C draws are camera-facing quads of half size (w, h) view units
//! (= world >> 5) around a world point, rotated in the screen plane.
//!
//! Sources: `--replay-capture DIR` of a tools/oracle/fx_capture.py run (the
//! lists read from the captured RAM at the replayed VBlank), or a live
//! mr-sim session once it carries the item and particle state (track A).
//! Drawing: one dynamic mesh per material, rebuilt every frame, facing the
//! ChaseCamera.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use bevy::prelude::*;
use mr_assets::raw::{self, TexSlot};
use mr_sim::mem::Mem;
use mr_sim::particles::{Particles, Sparks};

use crate::vramtex::VramTextures;

/// One camera-facing (or ground-flat) textured quad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Billboard {
    /// World position (kart units).
    pub pos: [i32; 3],
    /// Half size, view units (world >> 5).
    pub hw: i32,
    pub hh: i32,
    /// 12-bit rotation in the screen plane (about y when `flat`).
    pub angle: i32,
    /// Anchor bits of FUN_80011E3C (4 = standing on the point).
    pub anchor: u8,
    pub flat: bool,
    pub u: u8,
    pub v: u8,
    pub uw: i32,
    pub vh: i32,
    pub tpage: u16,
    pub clut: u16,
    pub rgb: [u8; 3],
    pub semi: bool,
    pub abr: u8,
}

/// A 3D line, colour a at `a` to colour b at `b` (additive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Streak {
    pub a: [i32; 3],
    pub b: [i32; 3],
    pub rgb_a: [u8; 3],
    pub rgb_b: [u8; 3],
}

/// What is drawn this frame. Sources fill it; set `manual` to fill it yourself.
#[derive(Resource, Default, Clone, Debug)]
pub struct FxFrame {
    pub billboards: Vec<Billboard>,
    pub streaks: Vec<Streak>,
    pub manual: bool,
}

/// A particle sprite definition (exe 0x800ACF40 -> 12+ bytes each).
#[derive(Clone, Debug)]
pub struct SpriteDef {
    pub slot: usize,
    pub w: u8,
    pub h: u8,
    pub frames: Vec<(u8, u8)>,
}

/// Tables the conversions need.
#[derive(Resource, Clone)]
pub struct FxTables {
    pub defs: Vec<Option<SpriteDef>>,
    pub slots: [Option<TexSlot>; 32],
    pub tokens: Vec<[u8; 8]>,
    pub sin: Vec<i16>,
    pub types: Vec<[u8; 4]>,
    /// 0x800A8848: 16 view frames (u, v, uw) of the type-2 weapon.
    pub anim: Vec<[i16; 3]>,
}

fn exe_off(a: u32) -> usize {
    (a - 0x8001_0000) as usize + 0x800
}

impl FxTables {
    pub fn load(exe: &[u8], slots: [Option<TexSlot>; 32]) -> FxTables {
        let u32at = |a: u32| {
            let o = exe_off(a);
            u32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]])
        };
        let mut defs = Vec::new();
        for i in 0..48u32 {
            let p = u32at(0x800A_CF40 + 4 * i);
            if !(0x8001_0000..0x800B_1000).contains(&p) {
                defs.push(None);
                continue;
            }
            let o = exe_off(p);
            let slot = u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
            // frames run to the next definition (at most 16)
            let next = (0..48u32).map(|k| u32at(0x800A_CF40 + 4 * k)).filter(|&q| q > p).min().unwrap_or(p + 12);
            let n = (((next - p) as usize).saturating_sub(8) / 2).clamp(1, 16);
            let frames = (0..n).map(|k| (exe[o + 8 + 2 * k], exe[o + 9 + 2 * k])).collect();
            defs.push(Some(SpriteDef { slot, w: exe[o + 2], h: exe[o + 3], frames }));
        }
        let t = exe_off(0x800A_C1B8);
        let tokens = (0..8).map(|k| exe[t + 8 * k..t + 8 * k + 8].try_into().unwrap()).collect();
        let s = exe_off(0x800A_88C0);
        let sin = (0..4096 + 1024).map(|k| i16::from_le_bytes([exe[s + 2 * k], exe[s + 2 * k + 1]])).collect();
        let types = Particles::from_exe(exe).types;
        let a = exe_off(0x800A_8848);
        let anim = (0..16).map(|k| std::array::from_fn(|j| i16::from_le_bytes([exe[a + 6 * k + 2 * j], exe[a + 6 * k + 2 * j + 1]]))).collect();
        FxTables { defs, slots, tokens, sin, types, anim }
    }

    fn slot_attr(&self, slot: usize, clut_off: u16, abr: u16) -> (u16, u16) {
        match self.slots.get(slot).and_then(|s| *s) {
            Some(s) => ((s.tpage.unwrap_or(0) & !0x60) | (abr & 0x60), s.clut.wrapping_add(clut_off)),
            None => (0, 0),
        }
    }

    /// FUN_80010E90 for one particle: None when not drawn.
    pub fn particle(&self, p: &mr_sim::particles::Particle) -> Option<Billboard> {
        if p.sprite == 0 {
            return None;
        }
        let d = self.defs.get(p.sprite as usize)?.as_ref()?;
        let (u, v) = *d.frames.get(p.frame as usize).or(d.frames.first())?;
        let s = self.slots.get(d.slot).and_then(|s| *s)?;
        // FUN_8005CC94: 4-bit pages take the CLUT from frame 0's (u, v)
        let clut = if s.mode.unwrap_or(0) == 0 {
            let (u0, v0) = d.frames[0];
            s.clut + ((v0 as u16 & 0xC0) >> 4) + (u0 as u16 >> 6)
        } else {
            s.clut
        };
        let abr = p.flags & 0x60;
        Some(Billboard {
            pos: p.pos,
            hw: p.w as i32,
            hh: p.h as i32,
            angle: p.angle as i32,
            anchor: 0,
            flat: p.flags & 0x10 != 0,
            u,
            v,
            uw: d.w as i32,
            vh: d.h as i32,
            tpage: (s.tpage.unwrap_or(0) & !0x60) | abr,
            clut,
            rgb: p.rgb,
            semi: abr != 0x60,
            abr: (abr >> 5) as u8,
        })
    }

    /// FUN_80032928 for one spark: None when dead (life >= 0x80).
    pub fn spark(&self, s: &mr_sim::particles::Spark) -> Option<Streak> {
        if s.life == 0xFF {
            return None;
        }
        let l = s.len as i32;
        Some(Streak { a: s.pos, b: [s.pos[0] + s.vel[0] * l, s.pos[1] + s.vel[1] * l, s.pos[2] + s.vel[2] * l], rgb_a: [0; 3], rgb_b: s.rgb })
    }
}

/// The camera struct fields the pickups and projectiles read
/// (DAT_800B0E60 -> the player's camera: +0x54 screen roll).
#[derive(Clone, Copy, Debug, Default)]
pub struct FxCamera {
    pub roll: i32,
    /// +0x52: yaw (the type-2 projectile picks its view frame with it).
    pub yaw: i32,
}

/// Pickups and projectiles from game memory (the oracle capture source;
/// the live session will map track A's ItemState to the same billboards).
pub fn items_from_mem(t: &FxTables, m: &Mem, cam: FxCamera, frame: i32, out: &mut Vec<Billboard>) {
    // turbo tokens: DAT_800B0A52 of them at 0x800E2370, 4 groups
    let n = m.u8(0x800B_0A52) as usize;
    for g in 0..4usize {
        let spin = m.i16(0x800B_0B90 + 2 * g as u32) as i32;
        let mut i = g;
        while i < n {
            let a = 0x800E_2370 + 0x20 * i as u32;
            let ty = m.u8(a + 0x1F) as usize;
            let tb = t.tokens.get(ty).copied().unwrap_or([0; 8]);
            let sc = m.i16(a + 0x16) as i32;
            let div = tb[6].max(1) as i32;
            let (tp, cl) = t.slot_attr(tb[4] as usize, tb[5] as u16, 0x60);
            out.push(Billboard {
                pos: [m.i32(a) << 5, m.i32(a + 4), m.i32(a + 8) << 5],
                hw: sc * tb[2] as i32 / div,
                hh: sc * tb[3] as i32 / div,
                angle: (cam.roll + spin) & 0xFFF,
                anchor: 0,
                flat: false,
                u: tb[0],
                v: tb[1],
                uw: tb[2] as i32 - 1,
                vh: tb[3] as i32 - 1,
                tpage: tp,
                clut: cl,
                rgb: [0x80; 3],
                semi: false,
                abr: 3,
            });
            i += 4;
        }
    }
    // item boxes: the star sprite, size 2 * scale
    if m.u8(0x800B_0B60) != 0 {
        // the count is *DAT_800D1C94; fx captures do not hold that byte, so
        // fall back to DAT_800B0B60 (the same number on DOCK: 44)
        let p = m.u32(0x800D_1C94);
        let n = match (0x8000_0000..0x8020_0000).contains(&p).then(|| m.u8(p)) {
            Some(c) if c != 0 => c as usize,
            _ => m.u8(0x800B_0B60) as usize,
        };
        for i in 0..n.min(0x40) {
            let a = 0x800E_1B70 + 0x20 * i as u32;
            let s = m.i16(a + 0x16) as i32 * 2;
            let (tp, cl) = t.slot_attr(0x15, 0xB, 0x60);
            out.push(Billboard { pos: [m.i32(a) << 5, m.i32(a + 4), m.i32(a + 8) << 5], hw: s, hh: s, angle: cam.roll & 0xFFF, anchor: 0, flat: false, u: 0xC0, v: 0x80, uw: 0x37, vh: 0x37, tpage: tp, clut: cl, rgb: [0x80; 3], semi: false, abr: 3 });
        }
    }
    // projectiles (FUN_80030D78)
    for i in 0..4u32 {
        let a = 0x800D_DC50 + 0x48 * i;
        if m.u8(a + 0x2E) == 0 {
            continue;
        }
        let flags = m.u16(a + 0x34);
        let ang = m.u16(a + 0x3A) as i32;
        let pos = [m.i32(a), m.i32(a + 4), m.i32(a + 8)];
        let b = |hw, hh, angle, anchor, u, v, uw, vh, slot: usize, clut| {
            let (tp, cl) = t.slot_attr(slot, clut, 0x60);
            Billboard { pos, hw, hh, angle, anchor, flat: false, u, v, uw, vh, tpage: tp, clut: cl, rgb: [0x80; 3], semi: false, abr: 3 }
        };
        if flags & 0x40 != 0 {
            let w = t.sin[(((frame & 0xF) * 0x100 + ((i as i32) << 9)) & 0xFFF) as usize] as i32;
            let w = if w < 0 { w + 0x7F } else { w } >> 7;
            out.push(b(0x90, 0xC0, (cam.roll + w) & 0xFFF, 4, 0x40, 0x82, 0x27, 0x3D, 0x14, 9));
        } else if flags & 1 != 0 {
            out.push(b(0xC0, 0xC0, ang, 0, 0x40, 0x56, 0x29, 0x29, 0x15, 5));
        } else if flags & 0x400 == 0 {
            out.push(b(0x64, 0x118, ang, 0, 0xC0, 0xC0, 0x13, 0x37, 0x15, 0xF));
        } else {
            // FUN_80030AD8(x, y, z, vx, vz, slot): one of the 16 view frames
            // of 0x800A8848 by the flight direction ratan2(vx, vz) seen from
            // the camera yaw, wobbling with the slot's phase
            let (vx, vz) = (m.i16(a + 0xC) as f64, m.i16(a + 0x14) as f64);
            let head = (vx.atan2(vz) * 2048.0 / std::f64::consts::PI).round() as i32 & 0xFFF;
            let mut d = (head - cam.yaw) as i16 as i32;
            if d > 0x800 {
                d -= 0x1000;
            } else if d < -0x800 {
                d += 0x1000;
            }
            let k = (((d + 0x92) as i16 as i32 & 0xFFF) / 0x125) as usize;
            let w = t.sin[(((frame & 0xF) * 0x100 + (i as i32) * 0x200) & 0xFFF) as usize] as i32;
            let w = if w < 0 { w + 0x7F } else { w } >> 7;
            if let Some(f) = t.anim.get(k) {
                let mut bb = b(0xC0, 0xC0, (cam.roll + w) & 0xFFF, 0, f[0] as u8, f[1] as u8, f[2] as i32, 0x1F, 0x14, 7);
                bb.anchor = 0;
                out.push(bb);
            }
        }
    }
}

/// Per-kart effects from a kart record (0x504 bytes), as FUN_8007E21C's
/// kart loop and FUN_80037B80 draw them:
/// - +0x4D6 (special move): two additive sparkles spinning both ways at the
///   kart, size (0x1000 - +0x442) * DAT_800B07BE / 07C6 >> 12, colours
///   0x800B07B8.. (the turbo meter's);
/// - +0x4F1 status effect (FUN_80037B80 inline cases 4, 0xF, 0x10, 0x13,
///   0x14, 0x17: billboards above / around the kart from record fields
///   +0x300.. that the effect owns). The function-call cases (0, 0xB, 0xC,
///   0xE, 0x15: FUN_80036A34 ...) are not drawn yet.
pub fn kart_effects(t: &FxTables, rec: &[u8], cam: FxCamera, frame: i32, out: &mut Vec<Billboard>) {
    let i32at = |o: usize| i32::from_le_bytes([rec[o], rec[o + 1], rec[o + 2], rec[o + 3]]);
    let i16at = |o: usize| i16::from_le_bytes([rec[o], rec[o + 1]]) as i32;
    let pos = [i32at(0), i32at(4), i32at(8)];
    let sp = [0i32, 32, 255, 384, 0, 192, 255, 240]; // 0x800B07B8.. (exe constants)
    let bb = |pos, hw, hh, angle, anchor, u, v, uw, vh, slot: usize, clut: u16, rgb: [u8; 3], abr: u16| {
        let (tp, cl) = t.slot_attr(slot, clut, abr);
        Billboard { pos, hw, hh, angle, anchor, flat: false, u, v, uw, vh, tpage: tp, clut: cl, rgb, semi: abr != 0x60, abr: (abr >> 5) as u8 }
    };
    if rec[0x4D6] != 0 {
        let f = 0x1000 - i16at(0x442);
        let s1 = { let v = f * sp[3]; (if v < 0 { v + 0xFFF } else { v }) >> 12 };
        let s2 = { let v = f * sp[7]; (if v < 0 { v + 0xFFF } else { v }) >> 12 };
        out.push(bb(pos, s1, s1, (frame & 0x3F) << 6, 0, 0xA0, 0xC0, 0x1F, 0x1F, 0x15, 0xE, [sp[0] as u8, sp[1] as u8, sp[2] as u8], 0x20));
        out.push(bb(pos, s2, s2, ((frame & 0x3F) * -0x40 + 0xFFF) & 0xFFF, 0, 0xA0, 0xC0, 0x1F, 0x1F, 0x15, 0xE, [sp[4] as u8, sp[5] as u8, sp[6] as u8], 0x20));
    }
    let effect = rec[0x4F1];
    // phase per kart: +0x4D7 (the kart's index byte)
    let kk = rec[0x4D7] as i8 as i32;
    let wob = |_: i32| {
        let v = t.sin[((((frame & 0x3F) * 0x40) + kk * 0x100) & 0xFFF) as usize] as i32;
        (if v < 0 { v + 0x3F } else { v }) >> 6
    };
    let k300 = i32at(0x300);
    match effect {
        4 => {
            let s = (k300 * 0x60000) >> 16;
            out.push(bb(pos, s, s, cam.roll & 0xFFF, 4, 0xA0, 0x10, 0x17, 0x17, 0x15, 0xB, [0x80; 3], 0x60));
        }
        0xF => {
            let w = i32at(0x30C);
            let h = { let v = w * 0xA0; (if v < 0 { v + 0x7F } else { v }) >> 7 };
            let g = (w & 0xFF) as u8;
            out.push(bb([i32at(0x300), i32at(0x304), i32at(0x308)], w as i16 as i32, h as i16 as i32, i32at(0x310) & 0xFFF, 4, 0x80, 0x80, 0x1F, 0x27, 0x14, 10, [g; 3], 0x60));
        }
        0x10 => {
            let s = i32at(0x304) as i16 as i32;
            out.push(bb([i32at(0x330), i32at(0x338), i32at(0x340)], s, s, 0, 0, 0x00, 0x40, 0x1F, 0x1F, 0x15, 4, [0, 0x60, 0xC0], 0x20));
        }
        0x13 => {
            let s = ((k300 << 18) >> 16) as i16 as i32;
            out.push(bb(pos, s, s, (cam.roll + wob(0x13)) & 0xFFF, 4, 0xA0, 0x80, 0x1F, 0x27, 0x14, 10, [0x80; 3], 0x60));
        }
        0x14 => {
            let w = { let v = k300 * 0x1B; (if v < 0 { v + 7 } else { v }) >> 3 };
            out.push(bb(pos, w as i16 as i32, (k300 * 7) as i16 as i32, (cam.roll + wob(0x14)) & 0xFFF, 4, 0xC0, 0x40, 0x1A, 0x37, 0xF, 7, [0x80; 3], 0x60));
        }
        0x17 => {
            out.push(bb(pos, (k300 & 0x1FFF) * 8, (k300 * 11) as i16 as i32, (cam.roll + wob(0x17)) & 0xFFF, 4, 0x20, 0xC0, 0x1F, 0x2B, 0x15, 0xB, [0x80; 3], 0x60));
        }
        _ => {}
    }
}

/// `--replay-capture DIR`: the fx lists at the replayed VBlank (the HUD
/// reads the same rows for its item state).
#[derive(Resource)]
pub(crate) struct CapRows {
    file: std::fs::File,
    regions: Vec<(u32, usize)>,
    vbs: Vec<i64>,
    size: usize,
}

fn json_numbers(s: &str, key: &str) -> Vec<i64> {
    let Some(i) = s.find(&format!("\"{key}\"")) else { return vec![] };
    let rest = &s[i..];
    let Some(a) = rest.find('[') else { return vec![] };
    let mut depth = 0;
    let mut end = a;
    for (j, ch) in rest[a..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = a + j;
                    break;
                }
            }
            _ => {}
        }
    }
    rest[a..end].split(|c: char| !(c.is_ascii_digit() || c == '-')).filter_map(|t| t.parse().ok()).collect()
}

impl CapRows {
    fn open(dir: &str) -> Option<CapRows> {
        let meta = std::fs::read_to_string(format!("{dir}/meta.json")).ok()?;
        let r = json_numbers(&meta, "regions");
        let regions: Vec<(u32, usize)> = r.chunks_exact(2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        if !regions.iter().any(|r| r.0 == 0x800E_0C20) {
            return None;
        }
        let vbs = json_numbers(&meta, "vb");
        let size = regions.iter().map(|r| r.1).sum();
        Some(CapRows { file: std::fs::File::open(format!("{dir}/frames.bin")).ok()?, regions, vbs, size })
    }
    /// One 32-bit word of the row at `vb`.
    pub(crate) fn u32_at(&mut self, vb: i64, addr: u32) -> Option<u32> {
        let i = self.vbs.iter().position(|&v| v == vb)?;
        let mut o = 0;
        for &(a, n) in &self.regions {
            if addr >= a && addr + 4 <= a + n as u32 {
                let mut b = [0u8; 4];
                self.file.seek(SeekFrom::Start((i * self.size + o + (addr - a) as usize) as u64)).ok()?;
                self.file.read_exact(&mut b).ok()?;
                return Some(u32::from_le_bytes(b));
            }
            o += n;
        }
        None
    }

    pub(crate) fn mem(&mut self, vb: i64) -> Option<Mem> {
        let i = self.vbs.iter().position(|&v| v == vb)?;
        let mut b = vec![0u8; self.size];
        self.file.seek(SeekFrom::Start((i * self.size) as u64)).ok()?;
        self.file.read_exact(&mut b).ok()?;
        let mut m = Mem::new(vec![0; 0x20_0000], vec![0; 0x400]);
        let mut o = 0;
        for &(a, n) in &self.regions {
            m.write(a, &b[o..o + n]);
            o += n;
        }
        Some(m)
    }
}

type MatKey = (u16, u16, bool, u8, bool);

#[derive(Resource, Default)]
struct FxDraw {
    mats: HashMap<MatKey, Handle<StandardMaterial>>,
    batches: HashMap<MatKey, (Entity, Handle<Mesh>)>,
    inverted: HashMap<(u16, u16), Handle<Image>>,
}

/// The page through the CLUT as a darkening mask: black, alpha = the
/// texel's brightest channel (0 for transparent texels).
fn inverted(cache: &mut HashMap<(u16, u16), Handle<Image>>, tex: &VramTextures, images: &mut Assets<Image>, tp: u16, cl: u16) -> Handle<Image> {
    if let Some(h) = cache.get(&(tp, cl)) {
        return h.clone();
    }
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let (px, py, depth) = mr_assets::vram::tpage(tp);
    let (cx, cy) = mr_assets::vram::clut(cl);
    let mut data = Vec::with_capacity(256 * 256 * 4);
    for v in 0..256u32 {
        for u in 0..256u32 {
            let t = tex.vram.texel(depth, px, py, cx, cy, u as u8, v as u8);
            let c = mr_assets::vram::rgba(t);
            let a = if c[3] == 0 { 0 } else { c[0].max(c[1]).max(c[2]) };
            data.extend_from_slice(&[0, 0, 0, a]);
        }
    }
    let mut img = Image::new(Extent3d { width: 256, height: 256, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, bevy::asset::RenderAssetUsages::RENDER_WORLD);
    img.sampler = bevy::image::ImageSampler::nearest();
    let h = images.add(img);
    cache.insert((tp, cl), h.clone());
    h
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FxFrame>()
            .init_resource::<FxDraw>()
            .add_systems(Startup, setup.after(crate::scene::setup))
            .add_systems(Update, (capture_source, session_source, draw.after(capture_source).after(session_source)));
    }
}

fn setup(mut commands: Commands) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let path = raw::course_raw_path(&exe, crate::scene::DOCK).expect("course");
    let file = disc.read(&path).expect("read RAW");
    let env = raw::LoadEnv::from_exe(&exe, crate::scene::DOCK).expect("env");
    let dec = raw::decode(&file, &env).expect("decode RAW");
    commands.insert_resource(FxTables::load(&exe, dec.slots));
    let a: Vec<String> = std::env::args().collect();
    if let Some(dir) = a.iter().position(|x| x == "--replay-capture").and_then(|i| a.get(i + 1)) {
        if let Some(c) = CapRows::open(dir) {
            commands.insert_resource(c);
        }
    }
}

fn capture_source(cap: Option<Res<crate::capture::CaptureReplay>>, rows: Option<ResMut<CapRows>>, t: Option<Res<FxTables>>, mut fx: ResMut<FxFrame>, mut last: Local<i64>) {
    let (Some(cap), Some(mut rows), Some(t)) = (cap, rows, t) else { return };
    if fx.manual || cap.vb == *last {
        return;
    }
    *last = cap.vb;
    let Some(m) = rows.mem(cap.vb) else { return };
    let mut out = FxFrame::default();
    let parts = Particles::read(&m, t.types.clone());
    out.billboards.extend(parts.list.iter().filter_map(|p| t.particle(p)));
    let sparks = Sparks::read(&m);
    out.streaks.extend(sparks.list.iter().filter_map(|s| t.spark(s)));
    let cam = FxCamera { roll: m.i16(0x800C_4F40 + 0x54) as i32, yaw: m.i16(0x800C_4F40 + 0x52) as i32 };
    items_from_mem(&t, &m, cam, m.i32(0x800B_0D90), &mut out.billboards);
    for k in 0..6u32 {
        kart_effects(&t, m.slice(0x800C_30A8 + 0x504 * k, 0x504), cam, m.i32(0x800B_0D90), &mut out.billboards);
    }
    *fx = out;
}

/// A live mr-sim session: its item state, kart records, frame counter and
/// camera roll written into a scratch game-layout memory, then the same
/// conversions as for captures; particles and sparks straight from
/// `Session::particles()` / `sparks()` (track A owns them in the race state).
fn session_source(p: Res<crate::Player>, cap: Option<Res<crate::capture::CaptureReplay>>, t: Option<Res<FxTables>>, mut fx: ResMut<FxFrame>, mut st: Local<(u64, Option<Mem>)>) {
    let Some(t) = t else { return };
    if fx.manual || cap.is_some() {
        return;
    }
    let crate::Driver::Sim(run) = &p.driver else { return };
    let s = &run.session;
    if st.1.is_some() && st.0 == s.ticks {
        return;
    }
    st.0 = s.ticks;
    let m = st.1.get_or_insert_with(|| Mem::new(vec![0; 0x20_0000], vec![0; 0x400]));
    s.items().write(m);
    let mut recs = Vec::new();
    for (slot, _, _, rec) in s.kart_records() {
        m.write(0x800C_30A8 + 0x504 * slot as u32, rec);
        recs.push(rec.to_vec());
    }
    let frame = s.globals.frame;
    let cam = FxCamera { roll: s.camera.roll as i32, yaw: s.camera.yaw as i32 };
    let mut out = FxFrame::default();
    if let Some(parts) = s.particles() {
        out.billboards.extend(parts.list.iter().filter_map(|p| t.particle(p)));
    }
    if let Some(sparks) = s.sparks() {
        out.streaks.extend(sparks.list.iter().filter_map(|k| t.spark(k)));
    }
    items_from_mem(&t, m, cam, frame, &mut out.billboards);
    for rec in &recs {
        kart_effects(&t, rec, cam, frame, &mut out.billboards);
    }
    *fx = out;
}

const S: f32 = 1.0 / crate::UNITS;
/// Gain of additive sprites (gamma vs linear blending, render ledger).
const ADD_GAIN: f32 = 2.2;

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    fx: Res<FxFrame>,
    mut st: ResMut<FxDraw>,
    tex: Option<ResMut<VramTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    cam: Query<&Transform, With<crate::ChaseCamera>>,
    mut vis: Query<&mut Visibility>,
) {
    let (Some(mut tex), Ok(ct)) = (tex, cam.single()) else { return };
    let right = ct.rotation * Vec3::X;
    let up = ct.rotation * Vec3::Y;
    // geometry per material
    let mut groups: HashMap<MatKey, (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>)> = HashMap::new();
    let k = |c: u8, q: f32| (c as f32 / 128.0).powf(2.2) * q;
    for b in &fx.billboards {
        let key: MatKey = (b.tpage & !0x60, b.clut, b.semi, b.abr, true);
        let g = groups.entry(key).or_default();
        let (hw, hh) = (b.hw as f32 * 32.0 * S, b.hh as f32 * 32.0 * S);
        // The game sorts each sprite by its centre's depth in the ordering
        // table, so the ground and walls around it rarely cut it; with a
        // depth buffer, pull camera-facing sprites toward the eye by their
        // radius (at most half the distance) instead (render ledger).
        let mut c = crate::to_bevy(b.pos[0], b.pos[1], b.pos[2]);
        // (moving along the eye ray keeps the projected centre; the size is
        // scaled by the same ratio so the sprite keeps its on-screen size)
        let (mut hw, mut hh) = (hw, hh);
        if !b.flat {
            let to_eye = ct.translation - c;
            let d = to_eye.length();
            if d > 1e-3 {
                let pull = hw.abs().max(hh.abs()).min(d * 0.5);
                c += to_eye / d * pull;
                let k = (d - pull) / d;
                hw *= k;
                hh *= k;
            }
        }
        let (s, co) = ((b.angle as f32) * std::f32::consts::TAU / 4096.0).sin_cos();
        // GTE offsets (x right, y down), anchor 4 = standing on the point
        let oy = if b.anchor & 4 != 0 { -hh } else { 0.0 };
        let corner = |dx: f32, dy: f32| {
            let (dx, dy) = (dx, dy + oy);
            let rx = dx * co - dy * s;
            let ry = dx * s + dy * co;
            if b.flat {
                c + Vec3::new(rx, 0.0, ry)
            } else {
                c + right * rx - up * ry
            }
        };
        let p = [corner(-hw, -hh), corner(hw, -hh), corner(-hw, hh), corner(hw, hh)];
        let (u0, v0) = (b.u as f32 / 256.0, b.v as f32 / 256.0);
        let (u1, v1) = ((b.u as i32 + b.uw) as f32 / 256.0, (b.v as i32 + b.vh) as f32 / 256.0);
        let uv = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
        // Additive blending happens in linear space here and in the GPU's
        // gamma space in the game: a mid-grey texel adds ~2.4x less light
        // here. Additive sprites are boosted x2.2 to keep explosions and
        // sparkles as bright as the game's (render ledger).
        let q = match (b.semi, b.abr) {
            (true, 1) => ADD_GAIN,
            (true, 3) => 0.25 * ADD_GAIN,
            _ => 1.0,
        };
        let col = if b.semi && b.abr == 2 {
            let m = b.rgb[0].max(b.rgb[1]).max(b.rgb[2]) as f32 / 128.0;
            [1.0, 1.0, 1.0, m.min(1.0)]
        } else {
            [k(b.rgb[0], q), k(b.rgb[1], q), k(b.rgb[2], q), if b.semi && b.abr == 0 { 0.5 } else { 1.0 }]
        };
        for i in [0usize, 1, 2, 1, 3, 2] {
            g.0.push(p[i].to_array());
            g.1.push(uv[i]);
            g.2.push(col);
        }
    }
    for s in &fx.streaks {
        let key: MatKey = (0, 0, true, 1, false);
        let g = groups.entry(key).or_default();
        let (a, b) = (crate::to_bevy(s.a[0], s.a[1], s.a[2]), crate::to_bevy(s.b[0], s.b[1], s.b[2]));
        // a ribbon about one game pixel wide at its distance
        let d = (a - ct.translation).length().max(0.1);
        let w = d * (2.0 * (120.0f32 / 160.0)) / 240.0 * 0.5;
        let n = (b - a).cross(ct.translation - a).normalize_or_zero() * w;
        let lin = |c: [u8; 3]| {
            let l = Color::srgb_u8(c[0], c[1], c[2]).to_linear();
            [l.red, l.green, l.blue, 1.0]
        };
        let (ca, cb) = (lin(s.rgb_a), lin(s.rgb_b));
        let p = [a + n, a - n, b + n, b - n];
        let cs = [ca, ca, cb, cb];
        for i in [0usize, 1, 2, 1, 3, 2] {
            g.0.push(p[i].to_array());
            g.1.push([0.0, 0.0]);
            g.2.push(cs[i]);
        }
    }
    // hide batches with nothing this frame
    for (key, (e, _)) in st.batches.iter() {
        if !groups.contains_key(key) {
            if let Ok(mut v) = vis.get_mut(*e) {
                *v = Visibility::Hidden;
            }
        }
    }
    for (key, (pos, uv, col)) in groups {
        let n = pos.len();
        let normals = vec![[0.0, 1.0, 0.0]; n];
        if let Some((e, h)) = st.batches.get(&key).cloned() {
            if let Some(mut m) = meshes.get_mut(&h) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
                m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            }
            if let Ok(mut v) = vis.get_mut(e) {
                *v = Visibility::Inherited;
            }
            continue;
        }
        let (tp, cl, semi, abr, textured) = key;
        // B - F (abr 2): darken the background by the texel's brightness,
        // dst * (1 - F * k), a black blend with alpha from the texel (as the
        // kart shadows do; render ledger)
        let subtract = semi && abr == 2 && textured;
        let page = if !textured {
            None
        } else if subtract {
            Some(inverted(&mut st.inverted, &tex, &mut images, tp, cl))
        } else {
            Some(tex.get(&mut images, tp, cl))
        };
        let mat = st
            .mats
            .entry(key)
            .or_insert_with(|| {
                mats.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: page,
                    unlit: true,
                    cull_mode: None,
                    alpha_mode: match (semi, abr) {
                        (true, 0) => AlphaMode::Blend,
                        (true, 1) | (true, 3) => AlphaMode::Add,
                        (true, 2) => AlphaMode::Blend,
                        (true, _) => AlphaMode::Blend,
                        _ => AlphaMode::Mask(0.5),
                    },
                    ..default()
                })
            })
            .clone();
        let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        let h = meshes.add(m);
        // big bounds: the mesh moves every frame
        let e = commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(mat), Transform::IDENTITY, Visibility::Inherited, bevy::camera::visibility::NoFrustumCulling)).id();
        st.batches.insert(key, (e, h));
    }
}
