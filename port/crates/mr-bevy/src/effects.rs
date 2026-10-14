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

use std::io::{Read, Seek, SeekFrom};

use bevy::prelude::*;
use mr_assets::raw::{self, TexSlot};
use mr_sim::mem::Mem;
use mr_sim::particles::{Particles, Sparks};

use crate::psxfx::{Blend, FxPrim, FxPrims, FxVert};

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
    /// Ordering-table bias (FUN_80011E3C sorts at OTZ * 4 + bias * 4; the
    /// status effects use -0x20 / -0x60 to sit in front of their kart).
    pub bias: i16,
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
    /// FUN_800313BC list quads (the status effects' shapes, kartfx.rs).
    pub quads: Vec<crate::kartfx::Quad3>,
    /// The flat list 0x800C84B8 (splash rings, skid quads; FUN_800342B8).
    pub flats: Vec<mr_sim::particles::Flat>,
    /// The ground decals of the specials (FUN_800861F0 entries of the
    /// shadow list, drawn by FUN_8002ADA8 like the kart shadows; kartfx.rs).
    pub decals: Vec<crate::kartshadow::ShadowRec>,
    /// The eye (world units) the flats are culled against.
    pub eye: [i32; 3],
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
    pub kart: crate::kartfx::KartFxTables,
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
        FxTables { defs, slots, tokens, sin, types, anim, kart: crate::kartfx::KartFxTables::load(exe) }
    }

    pub(crate) fn slot_attr(&self, slot: usize, clut_off: u16, abr: u16) -> (u16, u16) {
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
            bias: 0,
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
    /// +0x28: the eye, world units (kart LOD, list-quad cull).
    pub eye: [i32; 3],
}

/// Karts drawn as the game's far impostor instead of their model this frame
/// (bit = slot), with `faithful_draw_distance`: kart LOD +0x4B4 >= 0x10
/// (karts.rs hides those models).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct KartLodHidden(pub u8);

/// Everything drawn from one kart record: the special glow and the inline
/// status billboards (kart_effects), the status shapes (FUN_80037B80's
/// function cases) and, with the game's LOD, the far impostor.
/// `face`: a TER face pointer's normal when the source can resolve it (a live
/// session); else the kart's own smoothed ground normal +0x2D0 stands in.
fn kart_all(t: &FxTables, rec: &[u8], cam: FxCamera, frame: i32, faithful_lod: bool, face: &dyn Fn(u32) -> Option<[i16; 3]>, out: &mut FxFrame) -> bool {
    kart_effects(t, rec, cam, frame, &mut out.billboards);
    crate::kartfx::status_quads(&t.kart, rec, cam.eye, &mut out.quads);
    let own = [0x2D0usize, 0x2D2, 0x2D4].map(|o| i16::from_le_bytes([rec[o], rec[o + 1]]));
    let normal = |p: u32| if p == 0 { None } else { face(p).or(Some(own)) };
    crate::kartfx::special_decals(&t.kart, rec, &normal, &mut out.decals);
    if faithful_lod && crate::kartfx::lod(rec, cam.eye) >= 0x10 {
        crate::kartfx::impostor(t, &t.kart, rec, cam.yaw, &mut out.billboards);
        return true;
    }
    false
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
                bias: 0,
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
            out.push(Billboard { pos: [m.i32(a) << 5, m.i32(a + 4), m.i32(a + 8) << 5], hw: s, hh: s, angle: cam.roll & 0xFFF, anchor: 0, flat: false, u: 0xC0, v: 0x80, uw: 0x37, vh: 0x37, tpage: tp, clut: cl, rgb: [0x80; 3], semi: false, abr: 3, bias: 0 });
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
            Billboard { pos, hw, hh, angle, anchor, flat: false, u, v, uw, vh, tpage: tp, clut: cl, rgb: [0x80; 3], semi: false, abr: 3, bias: 0 }
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
    // the inline status cases sort 0x20 in front of the kart (0x10: 0x60)
    let bb = |pos, hw, hh, angle, anchor, u, v, uw, vh, slot: usize, clut: u16, rgb: [u8; 3], abr: u16| {
        let (tp, cl) = t.slot_attr(slot, clut, abr);
        Billboard { pos, hw, hh, angle, anchor, flat: false, u, v, uw, vh, tpage: tp, clut: cl, rgb, semi: abr != 0x60, abr: (abr >> 5) as u8, bias: -0x20 }
    };
    if rec[0x4D6] != 0 {
        let f = 0x1000 - i16at(0x442);
        let s1 = { let v = f * sp[3]; (if v < 0 { v + 0xFFF } else { v }) >> 12 };
        let s2 = { let v = f * sp[7]; (if v < 0 { v + 0xFFF } else { v }) >> 12 };
        out.push(Billboard { bias: 0, ..bb(pos, s1, s1, (frame & 0x3F) << 6, 0, 0xA0, 0xC0, 0x1F, 0x1F, 0x15, 0xE, [sp[0] as u8, sp[1] as u8, sp[2] as u8], 0x20) });
        out.push(Billboard { bias: -1, ..bb(pos, s2, s2, ((frame & 0x3F) * -0x40 + 0xFFF) & 0xFFF, 0, 0xA0, 0xC0, 0x1F, 0x1F, 0x15, 0xE, [sp[4] as u8, sp[5] as u8, sp[6] as u8], 0x20) });
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
            out.push(Billboard { bias: -0x60, ..bb([i32at(0x330), i32at(0x338), i32at(0x340)], s, s, 0, 0, 0x00, 0x40, 0x1F, 0x1F, 0x15, 4, [0, 0x60, 0xC0], 0x20) });
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

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::psxfx::PsxFxPlugin)
            .init_resource::<FxFrame>()
            .init_resource::<KartLodHidden>()
            .init_resource::<FxLookahead>()
            .add_systems(Startup, setup.after(crate::scene::setup))
            .add_systems(Update, (capture_source, session_source, draw.after(capture_source).after(session_source)))
            .add_systems(Update, fx_shots.after(capture_source))
            .add_systems(Startup, crate::pickupshadow::setup)
            .add_systems(Update, crate::pickupshadow::draw.after(crate::kartshadow::update_shadows));
    }
}

fn setup(mut commands: Commands) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let path = raw::course_raw_path(&exe, crate::courses::id()).expect("course");
    let file = disc.read(&path).expect("read RAW");
    let env = raw::LoadEnv::from_exe(&exe, crate::courses::id()).expect("env");
    let dec = raw::decode(&file, &env).expect("decode RAW");
    commands.insert_resource(FxTables::load(&exe, dec.slots));
    let a: Vec<String> = std::env::args().collect();
    if let Some(dir) = a.iter().position(|x| x == "--replay-capture").and_then(|i| a.get(i + 1)) {
        if let Some(c) = CapRows::open(dir) {
            commands.insert_resource(c);
        }
    }
}

fn capture_source(
    cap: Option<Res<crate::capture::CaptureReplay>>,
    rows: Option<ResMut<CapRows>>,
    t: Option<Res<FxTables>>,
    settings: Res<crate::settings::Settings>,
    mut fx: ResMut<FxFrame>,
    mut hidden: ResMut<KartLodHidden>,
    mut last: Local<i64>,
) {
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
    out.flats = mr_sim::particles::Flats::read(&m).list.iter().filter(|f| f.life != 0).copied().collect();
    let c = 0x800C_4F40;
    let cam = FxCamera { roll: m.i16(c + 0x54) as i32, yaw: m.i16(c + 0x52) as i32, eye: [m.i32(c + 0x28), m.i32(c + 0x2C), m.i32(c + 0x30)] };
    out.eye = cam.eye;
    items_from_mem(&t, &m, cam, m.i32(0x800B_0D90), &mut out.billboards);
    let mut h = 0u8;
    for k in 0..6u32 {
        if kart_all(&t, m.slice(0x800C_30A8 + 0x504 * k, 0x504), cam, m.i32(0x800B_0D90), settings.faithful_draw_distance(), &|_| None, &mut out) {
            h |= 1 << k;
        }
    }
    if hidden.0 != h {
        hidden.0 = h;
    }
    if std::env::var("MRM_FX_DEBUG").is_ok() {
        for b in &out.billboards {
            eprintln!("fx vb {} {:?}", cap.vb, b);
        }
    }
    *fx = out;
}

/// What the effects are drawn from at one tick of a live session: the
/// particle lists, item state, kart records, frame counter and camera.
#[derive(Clone)]
pub struct FxSource {
    /// `Session::ticks` of this state.
    pub ticks: u64,
    particles: Option<Particles>,
    sparks: Option<Sparks>,
    flats: Option<mr_sim::particles::Flats>,
    items: mr_sim::items::ItemState,
    recs: Vec<(usize, Vec<u8>)>,
    frame: i32,
    cam: FxCamera,
}

impl FxSource {
    pub fn of(s: &mr_sim::session::Session) -> FxSource {
        FxSource {
            ticks: s.ticks,
            particles: s.particles().cloned(),
            sparks: s.sparks().cloned(),
            flats: s.flats().cloned(),
            items: s.items().clone(),
            recs: s.kart_records().map(|(slot, _, _, r)| (slot, r.to_vec())).collect(),
            frame: s.globals.frame,
            cam: FxCamera { roll: s.camera.roll as i32, yaw: s.camera.yaw as i32, eye: s.camera.pos },
        }
    }
}

/// The coming tick's effect state, computed by the low-latency look-ahead
/// (pacing.rs `predict`, on the throw-away copy of the session): the
/// effects interpolate toward it exactly as the karts do.
#[derive(Resource, Default)]
pub struct FxLookahead(pub Option<FxSource>);

fn lerp_i(a: i32, b: i32, t: f32) -> i32 {
    (a as f64 + (b as f64 - a as f64) * t as f64).round() as i32
}

fn lerp3(a: [i32; 3], b: [i32; 3], t: f32) -> [i32; 3] {
    std::array::from_fn(|k| lerp_i(a[k], b[k], t))
}

/// Lerp the i32 at `o` of two byte records into `out`.
fn lerp_word(out: &mut [u8], a: &[u8], b: &[u8], o: usize, t: f32) {
    let r = |x: &[u8]| i32::from_le_bytes([x[o], x[o + 1], x[o + 2], x[o + 3]]);
    out[o..o + 4].copy_from_slice(&lerp_i(r(a), r(b), t).to_le_bytes());
}

/// The effect state shown at weight `t` between `a` (from) and `b` (to), as
/// the karts are shown: everything that exists in both (a particle, spark,
/// flat quad, projectile or kart continuing from `a` to `b`) at the
/// interpolated position; the rest (counters, frames, colours, what spawns
/// or dies) from the nearer endpoint. Display only: the session is not
/// touched.
fn interp_source(a: &FxSource, b: &FxSource, t: f32, ma: &mut Mem, mb: &mut Mem) -> (FxSource, bool) {
    let near_b = t >= 0.5;
    let mut out = if near_b { b.clone() } else { a.clone() };
    if let (Some(pa), Some(pb), Some(po)) = (&a.particles, &b.particles, out.particles.as_mut()) {
        for i in 0..pa.list.len().min(pb.list.len()) {
            let (x, y) = (&pa.list[i], &pb.list[i]);
            if x.life > 0 && y.life > 0 && x.sprite == y.sprite && x.kind == y.kind && y.life <= x.life {
                po.list[i].pos = lerp3(x.pos, y.pos, t);
            }
        }
    }
    if let (Some(sa), Some(sb), Some(so)) = (&a.sparks, &b.sparks, out.sparks.as_mut()) {
        for i in 0..sa.list.len().min(sb.list.len()) {
            let (x, y) = (&sa.list[i], &sb.list[i]);
            if x.life < 0x80 && y.life < 0x80 && y.life <= x.life {
                so.list[i].pos = lerp3(x.pos, y.pos, t);
            }
        }
    }
    if let (Some(fa), Some(fb), Some(fo)) = (&a.flats, &b.flats, out.flats.as_mut()) {
        for i in 0..fa.list.len().min(fb.list.len()) {
            let (x, y) = (&fa.list[i], &fb.list[i]);
            if x.life != 0 && y.life != 0 && (y.life & 0x7FFF) <= (x.life & 0x7FFF) && x.slot == y.slot {
                fo.list[i].pos = lerp3(x.pos, y.pos, t);
            }
        }
    }
    for (slot, rec) in out.recs.iter_mut() {
        let (Some((_, ra)), Some((_, rb))) = (a.recs.iter().find(|r| r.0 == *slot), b.recs.iter().find(|r| r.0 == *slot)) else { continue };
        for o in [0usize, 4, 8] {
            lerp_word(rec, ra, rb, o, t);
        }
        // a status effect's own points (+0x300.., +0x330..) while it lasts
        if ra[0x4F1] == rb[0x4F1] && ra[0x4F1] != 0 {
            for o in [0x300usize, 0x304, 0x308, 0x330, 0x338, 0x340] {
                lerp_word(rec, ra, rb, o, t);
            }
        }
    }
    out.cam.eye = lerp3(a.cam.eye, b.cam.eye, t);
    // items: written to game memory; projectiles in flight in both move
    a.items.write(ma);
    b.items.write(mb);
    let mut moved = false;
    for i in 0..4u32 {
        let p = 0x800D_DC50 + 0x48 * i;
        if ma.u8(p + 0x2E) != 0 && mb.u8(p + 0x2E) != 0 {
            moved = true;
        }
    }
    (out, moved)
}

/// A live mr-sim session: its item state, kart records, frame counter and
/// camera roll written into a scratch game-layout memory, then the same
/// conversions as for captures; particles and sparks straight from
/// `Session::particles()` / `sparks()` (track A owns them in the race state).
/// Presentation: the effects are drawn at the karts' display state: between
/// the last two ticks (classic), or between the last tick and the coming one
/// (`low_latency`: `FxLookahead`), at `Time<Fixed>`'s overstep.
#[allow(clippy::too_many_arguments)]
fn session_source(
    p: Res<crate::Player>,
    cap: Option<Res<crate::capture::CaptureReplay>>,
    t: Option<Res<FxTables>>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time<Fixed>>,
    la: Res<FxLookahead>,
    feed: Res<crate::karts::KartFeed>,
    mut fx: ResMut<FxFrame>,
    mut hidden: ResMut<KartLodHidden>,
    mut st: Local<SourceCache>,
) {
    let Some(t) = t else { return };
    if fx.manual || cap.is_some() {
        return;
    }
    let crate::Driver::Sim(run) = &p.driver else { return };
    let s = &run.session;
    let st = &mut *st;
    if st.now.as_ref().map(|n| n.ticks) != Some(s.ticks) {
        st.prev = st.now.take();
        st.now = Some(FxSource::of(s));
    }
    let now = st.now.as_ref().unwrap();
    let lookahead = crate::pacing::low_latency();
    let (from, to) = if lookahead {
        (Some(now), la.0.as_ref().filter(|l| l.ticks == now.ticks + 1))
    } else {
        (st.prev.as_ref().filter(|q| q.ticks + 1 == now.ticks), Some(now))
    };
    let a = time.overstep_fraction().clamp(0.0, 1.0);
    let ma = st.ma.get_or_insert_with(|| Mem::new(vec![0; 0x20_0000], vec![0; 0x400]));
    let mb = st.mb.get_or_insert_with(|| Mem::new(vec![0; 0x20_0000], vec![0; 0x400]));
    let (src, m): (FxSource, &mut Mem) = match (from, to) {
        (Some(f), Some(g)) => {
            let (o, moved) = interp_source(f, g, a, ma, mb);
            // memory of the nearer endpoint, projectiles in flight lerped
            let (near, far) = if a >= 0.5 { (mb, &*ma) } else { (ma, &*mb) };
            if moved {
                for i in 0..4u32 {
                    let pa = 0x800D_DC50 + 0x48 * i;
                    if far.u8(pa + 0x2E) != 0 && near.u8(pa + 0x2E) != 0 {
                        for o in [0u32, 4, 8] {
                            let (x, y) = if a >= 0.5 { (far.i32(pa + o), near.i32(pa + o)) } else { (near.i32(pa + o), far.i32(pa + o)) };
                            near.write(pa + o, &lerp_i(x, y, a).to_le_bytes());
                        }
                    }
                }
            }
            (o, near)
        }
        _ => {
            let n = now.clone();
            n.items.write(ma);
            (n, ma)
        }
    };
    for (slot, rec) in &src.recs {
        m.write(0x800C_30A8 + 0x504 * *slot as u32, rec);
    }
    let cam = src.cam;
    let mut out = FxFrame::default();
    if let Some(parts) = &src.particles {
        out.billboards.extend(parts.list.iter().filter_map(|p| t.particle(p)));
    }
    if let Some(sparks) = &src.sparks {
        out.streaks.extend(sparks.list.iter().filter_map(|k| t.spark(k)));
    }
    if let Some(f) = &src.flats {
        out.flats = f.list.iter().filter(|f| f.life != 0).copied().collect();
    }
    out.eye = cam.eye;
    items_from_mem(&t, m, cam, src.frame, &mut out.billboards);
    let mut h = 0u8;
    for (slot, rec) in &src.recs {
        let w = &s.world;
        let face = |p: u32| w.face_index(p).and_then(|i| w.ter.faces.get(i)).map(|f| f.normal);
        if kart_all(&t, rec, cam, src.frame, settings.faithful_draw_distance(), &face, &mut out) {
            h |= 1 << slot;
        }
    }
    if hidden.0 != h {
        hidden.0 = h;
    }
    // MRM_FX_LAG: per frame, the player kart's displayed position (karts.rs:
    // the feed endpoints at the overstep) minus the position the effects use
    // now, and minus the last real tick's (what they used before)
    if std::env::var("MRM_FX_LAG").is_ok() {
        let pos = |r: &[u8]| [0usize, 4, 8].map(|o| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]) as f64);
        let fr = |v: &[crate::karts::KartFrame]| v.iter().find(|k| k.slot == 0).map(|k| pos(&k.record));
        if let (Some(p0), Some(p1), Some((_, r))) = (fr(&feed.prev), fr(&feed.now), src.recs.iter().find(|r| r.0 == 0)) {
            let disp: [f64; 3] = std::array::from_fn(|k| p0[k] + (p1[k] - p0[k]) * a as f64);
            let d = |q: [f64; 3]| ((q[0] - disp[0]).powi(2) + (q[1] - disp[1]).powi(2) + (q[2] - disp[2]).powi(2)).sqrt();
            let real = now.recs.iter().find(|r| r.0 == 0).map(|r| pos(&r.1)).unwrap_or(disp);
            eprintln!("fxlag a {a:.3} new {:.0} old {:.0}", d(pos(r)), d(real));
        }
    }
    *fx = out;
}

/// session_source's state: the last two real ticks' sources and two
/// scratch game-layout memories.
#[derive(Default)]
struct SourceCache {
    prev: Option<FxSource>,
    now: Option<FxSource>,
    ma: Option<Mem>,
    mb: Option<Mem>,
}

const S: f32 = 1.0 / crate::UNITS;

/// Billboards and streaks -> PS1 primitives for the effects pass (psxfx.rs:
/// blending in the game's colour space, depth-tested against the scene).
fn draw(fx: Res<FxFrame>, t: Option<Res<FxTables>>, p: Res<crate::Player>, mut prims: ResMut<FxPrims>, cam: Query<&Transform, With<crate::ChaseCamera>>) {
    let Ok(ct) = cam.single() else { return };
    // the specials' ground decals: FUN_8002ADA8 as for the kart shadows
    if let (Some(t), Some((eye, cm))) = (t.as_ref(), p.pose.cam) {
        for d in &fx.decals {
            let Some(rs) = t.slots.get(d.slot as usize).copied().flatten() else { continue };
            let Some(tpage) = rs.tpage else { continue };
            let slot = mr_assets::dat::TexSlot { tpage, clut: rs.clut, fixed_clut: 0 };
            if !d.visible(eye) {
                continue;
            }
            let pos = crate::kartshadow::record_corners(d);
            if let Some((prim, _, _)) = crate::kartshadow::shadow_prim(d, pos, eye, cm, &slot) {
                prims.list.push(prim);
            }
        }
    }
    let right = ct.rotation * Vec3::X;
    let up = ct.rotation * Vec3::Y;
    for b in &fx.billboards {
        prims.list.push(billboard_prim(b, right, up));
    }
    for s in &fx.streaks {
        prims.list.push(streak_prim(s, ct.translation));
    }
    if let Some(t) = t {
        prims.list.extend(fx.quads.iter().filter_map(|q| crate::kartfx::quad_prim(&t, q, ct.translation, ct.rotation)));
        for f in &fx.flats {
            flat_prims(&t, f, fx.eye, &mut prims.list);
        }
    }
}

/// FUN_800342B8 for one entry of the flat list: within 0x40000 of the eye
/// on every axis; corners * scale / 128 around the position (world
/// orientation, no rotation of its own); FUN_80033C44's colour (gradient:
/// the far edge black; fade: * life / 16 over the last 16 VBlanks) and
/// blend; flag 0x80 splits it 2 x 2 with the texture mirrored per quarter.
pub fn flat_prims(t: &FxTables, f: &mr_sim::particles::Flat, eye: [i32; 3], out: &mut Vec<FxPrim>) {
    if f.life == 0 || !(0..3).all(|k| ((f.pos[k].wrapping_sub(eye[k])) as u32).wrapping_add(0x40000) < 0x80001) {
        return;
    }
    let sc = |c: i16| {
        let v = c as i32 * f.scale as i32;
        (if v < 0 { v + 0x7F } else { v }) >> 7
    };
    let v: [[i32; 3]; 4] = std::array::from_fn(|k| f.corners[k].map(sc));
    let rgb = if f.flags & 1 == 0 && f.flags & 2 != 0 && (f.life as i16 as i32) < 0x10 {
        let l = f.life as i16 as i32;
        f.rgb.map(|c| {
            let x = (c as u32).wrapping_mul(l as u32);
            let x = if (x as i32) < 0 { x.wrapping_add(0xF) } else { x };
            (x >> 4) as u8
        })
    } else {
        f.rgb
    };
    let (u, vv) = (f.uv[0], f.uv[1]);
    let (u1, v1) = (f.uv[2].wrapping_add(u).wrapping_sub(1), f.uv[3].wrapping_add(vv).wrapping_sub(1));
    let (tp, cl) = t.slot_attr(f.slot as usize, f.clut as u16, f.abr as u16);
    let blend = if f.abr == 0x60 { Blend::Opaque } else { Blend::Abr(((f.abr >> 5) & 3) as u8) };
    let w = |p: [i32; 3]| crate::to_bevy(f.pos[0] + p[0] * 32, f.pos[1] + p[1] * 32, f.pos[2] + p[2] * 32);
    // one GT4 in GPU order (p0, p1, p2, p3) with uvs, the first two vertices
    // coloured and, with the gradient flag, the last two black
    let mut quad = |p: [[i32; 3]; 4], uv: [[u8; 2]; 4]| {
        let far = if f.flags & 1 != 0 { [0u8; 3] } else { rgb };
        let cols = [rgb, rgb, far, far];
        let pos: [Vec3; 4] = std::array::from_fn(|k| w(p[k]));
        out.push(FxPrim {
            v: std::array::from_fn(|k| FxVert { pos: pos[k], uv: [uv[k][0] as f32, uv[k][1] as f32], rgb: cols[k].map(|c| c as f32 / 128.0) }),
            page: Some((tp & !0x60, cl)),
            blend,
            center: (pos[0] + pos[1] + pos[2] + pos[3]) * 0.25,
            // lying on the ground / water
            ground_eps: 0.25 * (pos[0] - pos[3]).length().max((pos[1] - pos[2]).length()),
            ground_radius: (pos[0] - pos[3]).length().max((pos[1] - pos[2]).length()),
            depth_pull: 0.0,
            ..Default::default()
        });
    };
    let (a, b, c, d) = ([u, vv], [u1, vv], [u, v1], [u1, v1]);
    if f.flags & 0x80 == 0 {
        // corners v0, v1, v2, v3 -> GPU v0, v1, v3, v2
        quad([v[0], v[1], v[3], v[2]], [a, b, c, d]);
    } else {
        let mid = |p: [i32; 3], q: [i32; 3]| [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2, (p[2] + q[2]) / 2];
        let (m01, m12, m23, m30, cen) = (mid(v[0], v[1]), mid(v[1], v[2]), mid(v[2], v[3]), mid(v[3], v[0]), mid(v[0], v[2]));
        quad([v[0], m01, m30, cen], [a, b, c, d]);
        quad([m01, v[1], cen, m12], [b, a, d, c]);
        quad([cen, m12, m23, v[2]], [d, c, b, a]);
        quad([m30, cen, v[3], m23], [c, d, a, b]);
    }
}

/// A billboard as FUN_80011E3C draws it: a quad of half size (w, h) view
/// units around the point, in the screen plane (or flat on the ground),
/// rotated by the angle; colour modulation texel * rgb / 128.
pub fn billboard_prim(b: &Billboard, right: Vec3, up: Vec3) -> FxPrim {
    let (hw, hh) = (b.hw as f32 * 32.0 * S, b.hh as f32 * 32.0 * S);
    let c = crate::to_bevy(b.pos[0], b.pos[1], b.pos[2]);
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
    let (u0, v0) = (b.u as f32, b.v as f32);
    let (u1, v1) = ((b.u as i32 + b.uw) as f32, (b.v as i32 + b.vh) as f32);
    let uv = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
    let rgb = b.rgb.map(|x| x as f32 / 128.0);
    let center = if b.flat { c } else { corner(0.0, 0.0) };
    FxPrim {
        v: std::array::from_fn(|i| FxVert { pos: p[i], uv: uv[i], rgb }),
        page: Some((b.tpage & !0x60, b.clut)),
        blend: if b.semi { Blend::Abr(b.abr & 3) } else { Blend::Opaque },
        center,
        // ground-flat sprites lie on the ground: let the ground under them
        // (within half their size) not hide them
        ground_eps: if b.flat { 0.5 * hw.abs().max(hh.abs()) } else { 0.0 },
        ground_radius: 2.0 * hw.abs().max(hh.abs()),
        // OT index = OTZ * 4 + bias * 4 with OTZ = view z / 4 (ZSF4 0x100,
        // the PSYQ default [INFER]): bias b moves the sprite -4b view units
        depth_pull: (-(b.bias as f32) * 4.0 * 32.0 * S).max(0.0),
        ..Default::default()
    }
}

/// A spark streak: LineG2, additive, black at the spark to its colour, as a
/// ribbon about one game pixel (of 240 lines) wide at its distance.
pub fn streak_prim(s: &Streak, eye: Vec3) -> FxPrim {
    let (a, b) = (crate::to_bevy(s.a[0], s.a[1], s.a[2]), crate::to_bevy(s.b[0], s.b[1], s.b[2]));
    let d = ((a + b) * 0.5 - eye).length().max(0.1);
    let w = d * (2.0 * (120.0f32 / 160.0)) / 240.0 * 0.5;
    let n = (b - a).cross(eye - a).normalize_or_zero() * w;
    let g = |c: [u8; 3]| c.map(|x| x as f32 / 255.0);
    let (ca, cb) = (g(s.rgb_a), g(s.rgb_b));
    let p = [a + n, a - n, b + n, b - n];
    let cs = [ca, ca, cb, cb];
    FxPrim {
        v: std::array::from_fn(|i| FxVert { pos: p[i], uv: [0.0, 0.0], rgb: cs[i] }),
        page: None,
        blend: Blend::Abr(1),
        center: (a + b) * 0.5,
        ground_eps: 0.0,
        ground_radius: (b - a).length(),
        depth_pull: 0.0,
        ..Default::default()
    }
}

/// Dev tool for the side-by-side checks (tools/oracle/fx_sbs.py):
/// `--fx-shots DIR VB,VB,...` with `--replay-capture` takes a window shot
/// DIR/b_<VB>.png at the first replayed tick whose VBlank is >= each VB
/// (time paused while it lands), then exits; `--fx-speed N` replays N times
/// faster in between. Prints the VBlank each shot landed on. Without a
/// capture the numbers are logic ticks of the live session.
#[derive(Default)]
struct ShotRun {
    dir: String,
    vbs: Vec<i64>,
    wait: u32,
    init: bool,
}

fn fx_shots(
    cap: Option<Res<crate::capture::CaptureReplay>>,
    p: Res<crate::Player>,
    mut run: Local<ShotRun>,
    mut time: ResMut<Time<Virtual>>,
    pages: Res<crate::psxfx::FxPages>,
    images: Res<Assets<Image>>,
    mut c: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if !run.init {
        run.init = true;
        let a: Vec<String> = std::env::args().collect();
        if let Some(i) = a.iter().position(|x| x == "--fx-shots") {
            run.dir = a.get(i + 1).cloned().unwrap_or_default();
            run.vbs = a.get(i + 2).map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            run.vbs.sort();
            run.vbs.reverse();
            if let Some(k) = a.iter().position(|x| x == "--fx-speed").and_then(|k| a.get(k + 1)).and_then(|v| v.parse::<f32>().ok()) {
                time.set_relative_speed(k);
            }
        }
    }
    let Some(&want) = run.vbs.last() else { return };
    let now = cap.map(|c| c.vb).unwrap_or(p.tick as i64);
    if run.wait > 0 {
        run.wait -= 1;
        if run.wait == 0 {
            run.vbs.pop();
            time.unpause();
            if run.vbs.is_empty() {
                exit.write(AppExit::Success);
            }
        }
        return;
    }
    if now >= want {
        time.pause();
        let path = format!("{}/b_{}.png", run.dir, want);
        eprintln!("fx-shot target {} vblank {}", want, now);
        if std::env::var("MRM_FX_DUMP").is_ok() {
            pages.dump(&images, &run.dir, &want.to_string());
        }
        c.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| {
            write_png(&ev.image, &path);
        });
        run.wait = 20;
    }
}

fn write_png(img: &Image, path: &str) {
    use bevy::render::render_resource::TextureFormat;
    let Some(data) = img.data.as_ref() else { return };
    let (w, h) = (img.width(), img.height());
    let mut rgba = data.clone();
    if matches!(img.texture_descriptor.format, TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb) {
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let _ = std::fs::write(path, mr_assets::img::png_rgba(w, h, &rgba));
}
