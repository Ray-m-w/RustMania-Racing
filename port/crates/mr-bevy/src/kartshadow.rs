//! Kart shadows: the game's 0x800C8B38 list (one 48-byte quad per kart).
//!
//! Built every race frame by FUN_8003E91C's tail from the kart record
//! (FUN_8003E59C makes the ground-aligned matrix, FUN_8003E3A0 writes the
//! record), drawn by FUN_8002ADA8 as one flat-textured quad (FT4) with
//! subtractive blending (tpage ABR 2), faded with view depth and culled at
//! +-0x30000 world units around the eye. See docs/spec/render-ledger.md.
//!
//! Inputs are kart-record fields (the physics port keeps them; the replay
//! sidecar has the game's): position +0x00/+0x08, heading +0x0E, shadow
//! rectangle +0x164..+0x16A (x0, x1, z0, z1, i16), texture tile +0x16C/+0x16D
//! (u, v), ground normal +0x2D0..+0x2D4 (i16 4.12, smoothed by the kart
//! update), ground y +0x31C, scale +0x442 (4096 = 1.0), flags +0x354 (bit 27
//! = no shadow), list slot +0x4D7.

use mr_assets::dat::{ratan2, SinTable};

pub const SHADOW_SLOT: u8 = 0x14;
pub const SHADOW_TPAGE_ADD: u16 = 0x40;
pub const SHADOW_INTENSITY: u8 = 0x80;

/// One shadow record as FUN_8003E3A0 writes it (world units; corner offsets
/// are (corner - origin) >> 5 after an i16 wrap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowRec {
    pub origin: [i32; 3],
    /// Corners A, B, C, D in polygon order (GPU order A, B, D, C).
    pub offs: [[i16; 3]; 4],
    pub uv0: [u8; 2],
    pub size: [u8; 2],
    pub slot: u8,
    /// kart+0x442 / 32 (0x80 = 1.0); 0 = not drawn.
    pub scale: u8,
    pub count: u8,
    pub intensity: u8,
    pub tpage_add: u16,
}

fn i16_at(k: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([k[o], k[o + 1]]) as i32
}
fn i32_at(k: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(k[o..o + 4].try_into().unwrap())
}
/// C-style (a * b) >> 12 rounded toward zero.
fn mulz(a: i32, b: i32) -> i32 {
    let p = a * b;
    (if p < 0 { p + 0xFFF } else { p }) >> 12
}

/// FUN_8003E59C(a, h, b): the shadow's ground-aligned rotation (4.12).
pub fn ground_matrix(sin: &SinTable, a: u16, h: u16, b: u16) -> [i32; 9] {
    let (sx, cx) = (sin.sin12(a), sin.cos12(a));
    let (sh, ch) = (sin.sin12(h), sin.cos12(h));
    let (sb, cb) = (sin.sin12(b), sin.cos12(b));
    let ch_sb = mulz(ch, sb) as i16 as i32;
    let sh_sb = mulz(sh, sb) as i16 as i32;
    let nsh = -sh;
    let r = |p: i32| (if p < 0 { p + 0xFFF } else { p }) >> 12;
    [
        mulz(ch, cb),
        -sb,
        mulz(sh, cb),
        r(ch_sb * cx - nsh * sx),
        mulz(cb, cx),
        r(sh_sb * cx - ch * sx),
        r(ch_sb * sx + nsh * cx),
        mulz(cb, sx),
        r(sh_sb * sx + ch * cx),
    ]
}

/// FUN_8003E91C (tail) + FUN_8003E3A0: the shadow record of a kart, or None
/// when flag +0x354 bit 27 suppresses it.
pub fn shadow_record(k: &[u8], sin: &SinTable) -> Option<ShadowRec> {
    if i32_at(k, 0x354) as u32 & 0x0800_0000 != 0 {
        return None;
    }
    let (n0, n1, n2) = (i16_at(k, 0x2D0), i16_at(k, 0x2D2), i16_at(k, 0x2D4));
    let a = ((-ratan2(n2, -n1)) & 0xFFF) as u16;
    let t = {
        let p = -n1 * sin.cos12(a) - n2 * sin.sin12(a);
        (if p < 0 { p + 0xFFF } else { p }) >> 12
    };
    let b = (ratan2(n0, t) & 0xFFF) as u16;
    let h = (i16_at(k, 0x0E) & 0xFFF) as u16;
    let m = ground_matrix(sin, a, h, b);
    let pos = [i32_at(k, 0), i32_at(k, 4), i32_at(k, 8)];
    let gy = i32_at(k, 0x31C);
    let xs = [i16_at(k, 0x164), i16_at(k, 0x166), i16_at(k, 0x166), i16_at(k, 0x164)];
    let zs = [i16_at(k, 0x168), i16_at(k, 0x168), i16_at(k, 0x16A), i16_at(k, 0x16A)];
    let origin = [pos[0], gy, pos[2]];
    let mut offs = [[0i16; 3]; 4];
    for i in 0..4 {
        let c = [
            pos[0] + ((xs[i] * m[0] + zs[i] * m[2]) >> 7),
            gy + ((xs[i] * m[3] + zs[i] * m[5]) >> 7),
            pos[2] + ((xs[i] * m[6] + zs[i] * m[8]) >> 7),
        ];
        for d in 0..3 {
            offs[i][d] = ((((c[d] - origin[d]) << 16) >> 16) >> 5) as i16;
        }
    }
    let s442 = i16_at(k, 0x442);
    Some(ShadowRec {
        origin,
        offs,
        uv0: [k[0x16C], k[0x16D]],
        size: [0x20, 0x20],
        slot: SHADOW_SLOT,
        scale: ((if s442 < 0 { s442 + 0x1F } else { s442 }) >> 5) as u8,
        count: 1,
        intensity: SHADOW_INTENSITY,
        tpage_add: SHADOW_TPAGE_ADD,
    })
}

impl ShadowRec {
    /// FUN_8002ADA8's cull: drawn when count and scale are nonzero and the
    /// origin is within 0x30000 world units of the eye on every axis.
    pub fn visible(&self, eye: [i32; 3]) -> bool {
        self.count != 0
            && self.scale != 0
            && (0..3).all(|d| ((self.origin[d] - eye[d]) as u32).wrapping_add(0x30000) < 0x60001)
    }
    /// Corner offsets as drawn: off * scale / 128 (toward zero), in world
    /// units / 32.
    pub fn draw_offsets(&self) -> [[i32; 3]; 4] {
        self.offs.map(|o| {
            o.map(|v| {
                let p = v as i32 * self.scale as i32;
                (if p < 0 { p + 0x7F } else { p }) >> 7
            })
        })
    }
    /// Texture coordinates of corners A, B, C, D.
    pub fn uvs(&self) -> [[u8; 2]; 4] {
        let [u, v] = self.uv0;
        // u8 texture coordinates wrap as on the GPU (a 0-size shadow gives u-1).
        let (u1, v1) = (u.wrapping_add(self.size[0]).wrapping_sub(1), v.wrapping_add(self.size[1]).wrapping_sub(1));
        [[u, v], [u1, v], [u1, v1], [u, v1]]
    }
    /// CLUT offset added to the slot's CLUT: (u >> 6) | ((v & 0xC0) >> 4),
    /// 0 when v == 0x90.
    pub fn clut_add(&self) -> u16 {
        if self.uv0[1] == 0x90 {
            0
        } else {
            (self.uv0[0] >> 6) as u16 | ((self.uv0[1] & 0xC0) >> 4) as u16
        }
    }
    /// Colour (all channels) for an origin at view depth `z` (world >> 5):
    /// full below 0x1000, linear fade to 0 at 0x1800.
    pub fn colour(&self, z: i32) -> u8 {
        if z < 0x1000 {
            self.intensity
        } else if z < 0x1801 {
            let u = self.intensity as i32 * (0x1800 - z);
            ((if u < 0 { u + 0x7FF } else { u }) >> 11) as u8
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Bevy side: one quad entity per kart record, updated every frame.
// ---------------------------------------------------------------------------

use bevy::prelude::*;
use mr_assets::dat::TexSlot;

use crate::psxfx::{Blend, FxPrim, FxPrims, FxVert};

#[derive(Resource)]
pub struct ShadowState {
    pub sin: SinTable,
    /// Texture slot 0x14 of the course (0x800DFF00 + 0x140).
    pub slot: TexSlot,
}

impl ShadowState {
    pub fn new(sin: SinTable, slot: TexSlot) -> Self {
        ShadowState { sin, slot }
    }
}

/// Kart records to shadow, in list-slot order.
/// Every fed kart (crate::karts::KartFeed: the session's six karts, a capture
/// replay, or the player alone), with its previous tick for interpolation.
fn kart_records(feed: &crate::karts::KartFeed) -> Vec<(&[u8], Option<&[u8]>)> {
    feed.now
        .iter()
        .map(|f| (&f.record[..], feed.prev.iter().find(|q| q.slot == f.slot).map(|q| &q.record[..])))
        .collect()
}

/// One FT4 per kart, as FUN_8002ADA8 draws it: the slot 0x14 silhouette tile
/// with colour = `ShadowRec::colour` (fade by view depth) and the tpage's
/// ABR 2, B - F, drawn by the effects pass in the game's colour space
/// (psxfx.rs: exact subtraction, depth-tested with the ground exempt).
pub fn update_shadows(
    p: Res<crate::Player>,
    feed: Res<crate::karts::KartFeed>,
    time: Res<Time<Fixed>>,
    st: Res<ShadowState>,
    mut prims: ResMut<FxPrims>,
) {
    let Some((eye, cm)) = p.pose.cam else { return };
    let karts = kart_records(&feed);
    let a = time.overstep_fraction();
    for (k, prev) in &karts {
        let Some(rec) = shadow_record(k, &st.sin).filter(|r| r.visible(eye)) else { continue };
        // Display interpolation like the kart model: lerp the corners from
        // the previous tick's record.
        let prev = prev.and_then(|k| shadow_record(k, &st.sin)).unwrap_or(rec);
        let corners = |r: &ShadowRec| -> [Vec3; 4] {
            r.draw_offsets().map(|o| {
                crate::to_bevy(r.origin[0] + o[0] * 32, r.origin[1] + o[1] * 32, r.origin[2] + o[2] * 32)
            })
        };
        let (c0, c1) = (corners(&prev), corners(&rec));
        let pos: [Vec3; 4] = std::array::from_fn(|i| c0[i].lerp(c1[i], a));
        // View depth of the origin, world >> 5 (FUN_8002ADA8's local_70).
        let d = [rec.origin[0] - eye[0], rec.origin[1] - eye[1], rec.origin[2] - eye[2]];
        let z = ((cm[6] as i64 * d[0] as i64 + cm[7] as i64 * d[1] as i64 + cm[8] as i64 * d[2] as i64) >> 17) as i32;
        let colour = rec.colour(z);
        if colour == 0 {
            continue;
        }
        let tp = st.slot.tpage.wrapping_add(rec.tpage_add);
        let cl = st.slot.clut.wrapping_add(rec.clut_add());
        let uv = rec.uvs();
        let rgb = [colour as f32 / 128.0; 3];
        // corners A, B, C, D -> GPU order A, B, D, C
        let ord = [0usize, 1, 3, 2];
        // texel units (the interpolated coordinate is floored, so the
        // GPU's u..u+31 covers texels u..u+30 as on the PS1)
        let tuv = |i: usize| [uv[i][0] as f32, uv[i][1] as f32];
        let center = (pos[0] + pos[1] + pos[2] + pos[3]) * 0.25;
        let ext = pos.iter().map(|q| (*q - center).length()).fold(0.0f32, f32::max);
        prims.list.push(FxPrim {
            v: std::array::from_fn(|j| FxVert { pos: pos[ord[j]], uv: tuv(ord[j]), rgb }),
            page: Some((tp & !0x60, cl)),
            blend: Blend::Abr(((tp >> 5) & 3) as u8),
            center,
            ground_eps: 0.5 * ext,
            ground_radius: 2.0 * ext,
            depth_pull: 0.0,
        });
        if std::env::var("MRM_SHADOW_DEBUG").is_ok() {
            eprintln!("shadow tick {} rec {:?} z {} colour {}", p.tick, rec, z, colour);
        }
    }
}
