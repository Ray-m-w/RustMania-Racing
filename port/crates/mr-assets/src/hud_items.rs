//! hud_items: the parts of the 1-player race HUD that come with items, as
//! the game builds them, from its own code and the tables in the executable
//! (read from SLUS_012.37 at run time; no game data here). Session 4 track B.
//!
//! Game functions ported (docs/spec/hud.md "Item HUD"):
//! - FUN_8002fd04: textured quad (POLY_FT4) with optional rotation, the
//!   x * 512/320 aspect stretch and off-screen cull. [`quad`]
//! - FUN_8002f8f4: textured sprite (SPRT) + DR_MODE. [`sprt`]
//! - FUN_8004f688 head: the +0x4F9 icon.
//! - FUN_80049440: "SPECIAL MOVE", the direction arrow (FUN_80032a40, 7
//!   LineF2 + "WRONG WAY") and the message banner (+0x4FC / +0x444: "LAP 2",
//!   "FINAL LAP", "-1 HIT", ...).
//! - FUN_8004cbc8 / FUN_8004c184: the two weapon slots (icon + charge ring).
//! - FUN_8004a310: the turbo meter (thermometer, fill, sparkles when full).
//! - FUN_80048a0c / FUN_800307d4: the minimap (96x64 SPRT, kart PolyF3 arrows).
//! - FUN_8005c2ec tail: the HUD pulse DAT_800B0E10 / 0x800B0E12. [`HudPulse`]
//!
//! Everything is integer and in the game's add order; the GPU draws the
//! last-added primitive first (ordering-table prepend), so earlier = in front.

use crate::hud::{HudTables, Size, Sprite};
use crate::le;

const EXE_BASE: u32 = 0x8001_0000;
const EXE_HDR: usize = 0x800;

fn off(addr: u32) -> usize {
    (addr - EXE_BASE) as usize + EXE_HDR
}

/// Exe addresses read here [CODE].
pub mod addr {
    /// 1.14 sine table, 4096 entries + 1024 (the cosine is sin + 1024,
    /// pointer *0x800AFF20 = 0x800A90C0).
    pub const SIN: u32 = 0x800A_88C0;
    /// Weapon-icon animation frames (u, v, uw) shorts, 16 frames.
    pub const ANIM: u32 = 0x800A_8848;
    /// Per-character table, stride 0x14 (minimap colour +0xF..+0x11,
    /// icon u/v +0x12/+0x13, slot +0x14). Indexed by kart +0x4B2.
    pub const CHARS: u32 = 0x800A_C218;
    /// Message strings (+0x4FC ids): "LAP 2", ..., "FINAL LAP", ..., "HITS".
    pub const MESSAGES: u32 = 0x800A_B424;
    pub const FMT_NEG: u32 = 0x800B_0690;
    pub const FMT_POS: u32 = 0x800B_0694;
    pub const PTR_HIT: u32 = 0x800B_0224;
    pub const PTR_HITS: u32 = 0x800B_0228;
    pub const PTR_NEW_VEHICLE: u32 = 0x800B_05B0;
    pub const PTR_SPECIAL_MOVE: u32 = 0x800B_05B4;
    pub const PTR_WRONG_WAY: u32 = 0x800A_B448;
    /// Direction-arrow outline: 8 x shorts then 8 z shorts (FUN_80032a40's
    /// stack copy of 0x80020F1C..0x80020F3B).
    pub const ARROW: u32 = 0x8002_0F1C;
    /// Turbo-meter sparkles: rgb + size, two sets (0x800B07B8..0x800B07C7).
    pub const SPARKLE: u32 = 0x800B_07B8;
    /// Course flags, stride 0xC (+2: bit 1 = minimap from the table below).
    pub const COURSE_FLAGS: u32 = 0x800A_CB38;
    /// Minimap per course, stride 0x14: u, v, w, h, x0, z0, x1, z1.
    pub const MAP: u32 = 0x800A_C6DC;
}

/// A textured quad as FUN_8002fd04 builds it (POLY_FT4 0x2C, |2 unless abr
/// 0x60). Vertices in GPU order v0 v1 v2 v3 (v0-v1 top edge, v2-v3 bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quad {
    pub v: [[i32; 2]; 4],
    pub uv: [[u8; 2]; 4],
    pub slot: u8,
    pub clut_off: u16,
    pub rgb: [u8; 3],
    pub semi: bool,
    /// Blend mode 0..3 in the tpage (from the game's 0/0x20/0x40/0x60 argument).
    pub abr: u8,
}

/// One HUD primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    /// SPRT (font glyphs, FUN_8002f8f4 sprites) or an axis-aligned FT4.
    Sprt(Sprite),
    /// Rotated / general POLY_FT4.
    Ft4(Quad),
    /// Flat triangle (PolyF3 0x20, minimap kart arrows).
    F3 { v: [[i32; 2]; 3], rgb: [u8; 3] },
    /// Flat line (LineF2 0x40, direction arrow).
    Line { v: [[i32; 2]; 2], rgb: [u8; 3] },
    /// Flat rectangle (TILE 0x60, FUN_8002f5e4: the stunt progress bar).
    Tile { x: i32, y: i32, w: i32, h: i32, rgb: [u8; 3] },
}

/// Tables of the item HUD, read from the executable.
#[derive(Clone, Debug)]
pub struct ItemTables {
    pub sin: Vec<i16>,
    pub anim: Vec<[i16; 3]>,
    pub chars: Vec<u8>,
    pub messages: Vec<String>,
    pub fmt_neg: String,
    pub fmt_pos: String,
    pub hit: String,
    pub hits: String,
    pub new_vehicle: String,
    pub special_move: String,
    pub wrong_way: String,
    pub arrow_x: [i16; 8],
    pub arrow_z: [i16; 8],
    pub sparkle: [i16; 8],
    pub course_flags: Vec<u16>,
    pub maps: Vec<u8>,
}

fn cstr(exe: &[u8], a: u32) -> Option<String> {
    let s = exe.get(off(a)..)?;
    let n = s.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&s[..n]).into_owned())
}
fn pstr(exe: &[u8], p: u32) -> Option<String> {
    cstr(exe, le::u32(exe, off(p)))
}

impl ItemTables {
    pub fn from_exe(exe: &[u8]) -> Option<ItemTables> {
        let i16s = |a: u32, n: usize| -> Option<Vec<i16>> {
            let o = off(a);
            exe.get(o..o + 2 * n)?;
            Some((0..n).map(|k| le::i16(exe, o + 2 * k)).collect())
        };
        let sin = i16s(addr::SIN, 4096 + 1024)?;
        let a = i16s(addr::ANIM, 16 * 3)?;
        let anim = a.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        let chars = exe.get(off(addr::CHARS)..off(addr::CHARS) + 0x14 * 32)?.to_vec();
        let messages = (0..24).map(|i| pstr(exe, addr::MESSAGES + 4 * i)).collect::<Option<Vec<_>>>()?;
        let ar = i16s(addr::ARROW, 16)?;
        let mut arrow_x = [0; 8];
        let mut arrow_z = [0; 8];
        arrow_x.copy_from_slice(&ar[..8]);
        arrow_z.copy_from_slice(&ar[8..]);
        let mut sparkle = [0; 8];
        sparkle.copy_from_slice(&i16s(addr::SPARKLE, 8)?);
        let course_flags = (0..0x25).map(|c| le::u16(exe, off(addr::COURSE_FLAGS) + 0xC * c + 2)).collect();
        let maps = exe.get(off(addr::MAP)..off(addr::MAP) + 0x14 * 0x25)?.to_vec();
        Some(ItemTables {
            sin,
            anim,
            chars,
            messages,
            fmt_neg: cstr(exe, addr::FMT_NEG)?,
            fmt_pos: cstr(exe, addr::FMT_POS)?,
            hit: pstr(exe, addr::PTR_HIT)?,
            hits: pstr(exe, addr::PTR_HITS)?,
            new_vehicle: pstr(exe, addr::PTR_NEW_VEHICLE)?,
            special_move: pstr(exe, addr::PTR_SPECIAL_MOVE)?,
            wrong_way: pstr(exe, addr::PTR_WRONG_WAY)?,
            arrow_x,
            arrow_z,
            sparkle,
            course_flags,
            maps,
        })
    }

    fn sin(&self, a: i32) -> i32 {
        self.sin[(a as usize) & 0xFFF] as i32
    }
    fn cos(&self, a: i32) -> i32 {
        self.sin[((a as usize) & 0xFFF) + 1024] as i32
    }
    fn chr(&self, c: u8, field: usize) -> u8 {
        self.chars.get(c as usize * 0x14 + field).copied().unwrap_or(0)
    }
}

/// C division toward zero by a power of two, as the game's
/// `if (v < 0) v += 2^n - 1; v >> n`.
fn sra_tz(v: i32, n: u32) -> i32 {
    if v < 0 {
        (v + ((1 << n) - 1)) >> n
    } else {
        v >> n
    }
}
fn s16(v: i32) -> i32 {
    v as i16 as i32
}

/// FUN_8002fd04(x, y, hw, hh, flags = 0, angle, u, v, uw, vh, r, g, b, slot,
/// clut_off, abr): a quad of half-size (hw, hh) around (x, y), x offsets
/// stretched by 512/320, rotated by `angle` (4096 = 360 deg) when non-zero.
/// None when the game culls it (off the 512x240 screen). `flags` (anchor
/// bits) are 0 at every call ported here.
#[allow(clippy::too_many_arguments)]
pub fn quad(t: &ItemTables, x: i32, y: i32, hw: i32, hh: i32, angle: i32, u: u8, v: u8, uw: i32, vh: i32, rgb: [u8; 3], slot: u8, clut_off: u16, abr: i32) -> Option<Quad> {
    quad_f(t, x, y, hw, hh, 0, angle, u, v, uw, vh, rgb, slot, clut_off, abr)
}

/// `quad` with FUN_8002fd04's anchor flags (param 5, low nibble): 1 = top
/// edge at y (quad below), 4 = bottom edge at y, 2 = left edge at x, 8 =
/// right edge at x (5 and 10 cancel); the quad then rotates about the
/// anchor point, corner by corner.
#[allow(clippy::too_many_arguments)]
pub fn quad_f(t: &ItemTables, x: i32, y: i32, hw: i32, hh: i32, flags: u32, angle: i32, u: u8, v: u8, uw: i32, vh: i32, rgb: [u8; 3], slot: u8, clut_off: u16, abr: i32) -> Option<Quad> {
    let hw = s16(hw);
    let hh = s16(hh);
    let mut f = flags & 0xF;
    if f & 5 == 5 {
        f ^= 5;
    }
    if f & 10 == 10 {
        f ^= 10;
    }
    // l[0..4] x offsets of the corners, l[4..8] y offsets
    let (ya, yb) = if f & 4 != 0 { (-2 * hh, 0) } else if f & 1 != 0 { (0, 2 * hh) } else { (-hh, hh) };
    let (xa, xb) = if f & 8 != 0 { (-2 * hw, 0) } else if f & 2 != 0 { (0, 2 * hw) } else { (-hw, hw) };
    let mut l = [xa, xb, xb, xa, ya, ya, yb, yb].map(s16);
    let angle = s16(angle);
    let (vx, vy);
    if angle != 0 && f != 0 {
        // every corner rotated about the anchor (x, y)
        let s = t.sin(angle);
        let c = t.cos(angle);
        for k in 0..4 {
            let (px, py) = (l[k], l[k + 4]);
            l[k] = s16((sra_tz(px * c - py * s, 14) << 9) / 0x140);
            l[k + 4] = s16(sra_tz(px * s + py * c, 14));
        }
        let xs = [x + l[0], x + l[1], x + l[3], x + l[2]];
        let ys = [y + l[4], y + l[5], y + l[7], y + l[6]];
        if xs.iter().all(|&v| v < 0) || xs.iter().all(|&v| v > 0x1FF) || ys.iter().all(|&v| v < 0) || ys.iter().all(|&v| v > 0xEF) {
            return None;
        }
        vx = xs;
        vy = ys;
    } else if angle == 0 {
        l[0] = s16((l[3] << 9) / 0x140);
        l[2] = s16((l[2] << 9) / 0x140);
        let x0 = x + l[0];
        if x0 > 0x1FF {
            return None;
        }
        let x1 = x + l[2];
        if x1 < 0 {
            return None;
        }
        let y0 = y + l[5];
        if y0 > 0xEF {
            return None;
        }
        let y1 = y + l[7];
        if y1 < 0 {
            return None;
        }
        vx = [x0, x1, x0, x1];
        vy = [y0, y0, y1, y1];
    } else {
        let s = t.sin(angle);
        let c = t.cos(angle);
        let a = l[3] * c - l[5] * s;
        let b = l[3] * s + l[5] * c;
        l[4] = s16(sra_tz(b, 14));
        let d = l[3] * c - l[7] * s;
        l[0] = s16((sra_tz(a, 14) << 9) / 0x140);
        let e = l[3] * s + l[7] * c;
        l[7] = s16(sra_tz(e, 14));
        l[5] = s16(-l[7]);
        l[6] = s16(-l[4]);
        l[3] = s16((sra_tz(d, 14) << 9) / 0x140);
        l[1] = s16(-l[3]);
        l[2] = s16(-l[0]);
        let xs = [x + l[0], x + l[1], x + l[3], x + l[2]];
        let ys = [y + l[4], y + l[5], y + l[7], y + l[6]];
        if xs.iter().all(|&v| v < 0) || xs.iter().all(|&v| v > 0x1FF) || ys.iter().all(|&v| v < 0) || ys.iter().all(|&v| v > 0xEF) {
            return None;
        }
        vx = xs;
        vy = ys;
    }
    let u1 = u.wrapping_add(uw as u8);
    let v1 = v.wrapping_add(vh as u8);
    Some(Quad {
        v: [[s16(vx[0]), s16(vy[0])], [s16(vx[1]), s16(vy[1])], [s16(vx[2]), s16(vy[2])], [s16(vx[3]), s16(vy[3])]],
        uv: [[u, v], [u1, v], [u, v1], [u1, v1]],
        slot,
        clut_off,
        rgb,
        semi: abr != 0x60,
        abr: ((abr >> 5) & 3) as u8,
    })
}

/// FUN_8004EEDC (a league's POINTS table): the karts' faces in table
/// order from x0 every 0x40 px, hung from y - 1, the same icons and swing
/// as `intro_faces` (FUN_8002fd04, flags 4, the character's icon 0x2F x 0x2F).
pub fn table_faces(t: &ItemTables, x0: i32, y: i32, chars: &[u8], strip: i32) -> Vec<Prim> {
    let mut out = Vec::new();
    let mut x = x0;
    let mut ph = (strip & 0x1F) << 7;
    for &c in chars {
        let mut a = t.sin(ph);
        if a < 0 {
            a += 0x7F;
        }
        let ang = (a >> 7) & 0xFFF;
        out.extend(quad_f(t, x, y - 1, 0x10, 0x10, 4, ang, t.chr(c, 0x12), t.chr(c, 0x13), 0x2F, 0x2F, [0x80; 3], t.chr(c, 0x14), 0, 0x60).map(Prim::Ft4));
        x += 0x40;
        ph = (ph - 0x333) & 0xFFF;
    }
    out
}

/// FUN_80050F64 after "RACE n LAPS" (1 player, race flag 0x10 clear): the
/// opponents' faces in a row, kart 1.. at x = 0x100 - (n - 2) * 0x20 + 0x40 * i,
/// y 0x3C: FUN_8002fd04(x, 0x3C, 0x10, 0x10, flags 4 (hung from the bottom
/// edge), angle, the character's icon (table 0x800AC218 + c * 0x14: +0x12 u,
/// +0x13 v, +0x14 slot), 0x2F x 0x2F, 0x80 grey, opaque), swinging by
/// angle = (sin[((DAT_800B07D4 & 0x1F) << 7) - 0x333 * (i - 1)] / 128) & 0xFFF.
pub fn intro_faces(t: &ItemTables, s: &ItemHud) -> Vec<Prim> {
    let mut out = Vec::new();
    let n = s.karts.len() as i32;
    if s.players != 1 || s.race_flags & 0x10 != 0 || n < 2 {
        return out;
    }
    let mut x = 0x100 - (n - 2) * 0x20;
    let mut ph = ((s.strip & 0x1F) << 7) as i32;
    for k in &s.karts[1..] {
        let mut a = t.sin(ph);
        if a < 0 {
            a += 0x7F;
        }
        let ang = (a >> 7) & 0xFFF;
        let c = k.character;
        out.extend(quad_f(t, x, 0x3C, 0x10, 0x10, 4, ang, t.chr(c, 0x12), t.chr(c, 0x13), 0x2F, 0x2F, [0x80; 3], t.chr(c, 0x14), 0, 0x60).map(Prim::Ft4));
        x += 0x40;
        ph = (ph - 0x333) & 0xFFF;
    }
    out
}

/// FUN_8002f8f4(x, y, u, v, w, h, r, g, b, slot, clut_off, abr): SPRT 0x64
/// (|2 unless abr 0x60), modulated by rgb.
#[allow(clippy::too_many_arguments)]
pub fn sprt(x: i32, y: i32, u: u8, v: u8, w: i32, h: i32, rgb: [u8; 3], slot: u8, clut_off: u16, abr: i32) -> Sprite {
    Sprite { x: s16(x), y: s16(y), w, h, u, v, uw: w, vh: h, slot, clut_off, rgb, raw: false, semi: abr != 0x60, abr: ((abr >> 5) & 3) as u8 }
}

/// DAT_800B0E10 / DAT_800B0E12: the HUD pulse (weapon icon and turbo
/// sparkle sizes). FUN_8005c2ec, every game frame after the frame counter
/// DAT_800B0D90 += dt: target (0x10, 0x1C) while frame % 120 < 60, else
/// (0x1C, 0x10); each value steps toward its target by dt, clamped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudPulse(pub [i16; 2]);

impl Default for HudPulse {
    fn default() -> Self {
        HudPulse([0x10, 0x1C])
    }
}

impl HudPulse {
    /// `frame` = DAT_800B0D90 after this frame's increment.
    pub fn step(&mut self, frame: i32, dt: i32) {
        let target = if frame % 0x78 < 0x3C { [0x10i16, 0x1C] } else { [0x1C, 0x10] };
        for i in 0..2 {
            let v = self.0[i] as i32;
            let tg = target[i] as i32;
            if v < tg {
                let n = v + dt;
                self.0[i] = if s16(n) > tg { tg as i16 } else { n as i16 };
            } else if tg < v {
                let n = v - dt;
                self.0[i] = if s16(n) < tg { tg as i16 } else { n as i16 };
            }
        }
    }
}

/// One kart as the minimap reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapKart {
    /// Kart +0x0 / +0x8 (world units).
    pub x: i32,
    pub z: i32,
    /// Kart +0xE.
    pub heading: i16,
    /// Kart +0x496 (bits 0x60: the kart shows as its character icon instead).
    pub flags: u8,
    /// Kart +0x4B2: character.
    pub character: u8,
    /// Kart +0x440: damage (race flag 8: a kart at 1000 is not shown).
    pub damage: i16,
}

/// Everything the item HUD reads, for one player kart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemHud {
    /// DAT_800B0D90: frame counter (dt per game frame).
    pub frame: i32,
    /// DAT_800B07D4: the animation strip counter (+dt per frame; the intro
    /// faces swing with it). Live sessions use the frame counter (its phase
    /// at race load is not modelled, as animtex.rs).
    pub strip: i32,
    /// DAT_800B0E10 / 0x800B0E12.
    pub pulse: HudPulse,
    /// DAT_800B0A94: race clock (the player's own minimap marker blinks with it).
    pub race_time: i32,
    /// DAT_800B0C70.
    pub countdown: i32,
    /// DAT_800B0D46 (1 = race).
    pub mode: u8,
    /// DAT_800B0FAC: course (DOCK = 3).
    pub course: u8,
    /// DAT_800B0E38 (selects the arrow's projection; 4 on DOCK).
    pub e38: i16,
    /// DAT_800B07F4 / DAT_800B07F6 / DAT_800B0E95: the global special move
    /// announcement (07F4 == -1: none).
    pub special: (i16, i16, i8),
    /// The player's camera matrix (0x800C4F40 + player*0x70, 3x3 4.12 shorts).
    pub cam: [[i16; 3]; 3],
    /// The player's kart record (0x504 bytes).
    pub kart: Vec<u8>,
    /// All karts (index order), for the minimap; `players` of them are human.
    pub karts: Vec<MapKart>,
    /// DAT_800B0E28: human players.
    pub players: usize,
    /// DAT_800B07E2: race option bits (8: the damage race, DISCO).
    pub race_flags: u16,
    /// The colour FUN_80049FD8 drew the damage gauge's heart with (three
    /// random bytes while +0x43E is 0x321..0x3E7); None = 0x80.
    pub gauge_rgb: Option<[u8; 3]>,
}

/// Kart record offsets the item HUD reads.
pub mod kart {
    pub const SPECIAL_TIMER: usize = 0x468;
    pub const MSG_VALUE: usize = 0x444;
    pub const METER_SLIDE: usize = 0x47E;
    pub const TURBO: usize = 0x48A;
    pub const ARROW: usize = 0x48E;
    pub const MAP_FLAGS: usize = 0x496;
    pub const CHARACTER: usize = 0x4B2;
    pub const WEAPONS: usize = 0x4D0;
    pub const CHARGE: usize = 0x4D4;
    pub const PLAYER: usize = 0x4D7;
    pub const SPECIAL: usize = 0x4F1;
    pub const ICON_4F9: usize = 0x4F9;
    pub const MESSAGE: usize = 0x4FC;
    pub const WRONG_WAY: usize = 0x503;
}

impl ItemHud {
    fn k8(&self, o: usize) -> i32 {
        self.kart[o] as i8 as i32
    }
    fn k16(&self, o: usize) -> i32 {
        le::i16(&self.kart, o) as i32
    }
}

/// The item HUD of a 1-player race in add order: the +0x4F9 icon, then
/// FUN_8004f688 case 1's FUN_80049440 / FUN_8004cbc8 / FUN_8004a310 (the
/// place and LAP texts in between come from `HudTables::layout`), then the
/// minimap. Use [`race_hud`] for the combined order.
pub struct Parts {
    pub icon: Vec<Prim>,
    pub status: Vec<Prim>,
    pub weapons: Vec<Prim>,
    pub meter: Vec<Prim>,
    pub minimap: Vec<Prim>,
}

pub fn parts(ht: &HudTables, t: &ItemTables, s: &ItemHud) -> Parts {
    let mut p = Parts { icon: vec![], status: vec![], weapons: vec![], meter: vec![], minimap: vec![] };
    // FUN_8004f688 head: +0x4F9 icon (1P: y = 0x3C - 0xC racing, 0xB4 - 0xC counting down)
    if s.kart[kart::ICON_4F9] & 0x7F != 0 {
        let y = if s.countdown == 0 { 0x3C } else { 0xB4 };
        p.icon.push(Prim::Sprt(sprt(0xED, y - 0xC, 0x80, 0x40, 0x25, 0x18, [0x80; 3], 0x15, 6, 0x60)));
    }
    if s.countdown > 0 || s.mode != 1 || s.kart[0x4C7] != 0 {
        return p;
    }
    p.status = status(ht, t, s);
    p.weapons = weapons(t, s, 0x2D);
    p.meter = meter(t, s);
    p
}

/// FUN_80049440 (1 player): SPECIAL MOVE, the direction arrow and the
/// message banner.
pub fn status(ht: &HudTables, t: &ItemTables, s: &ItemHud) -> Vec<Prim> {
    let mut out = Vec::new();
    let text = |out: &mut Vec<Prim>, st: &str, x: i32, y: i32, size: Size, colour: u8| {
        out.extend(ht.text(st.as_bytes(), x, y, 0, size, colour, 0x60).into_iter().map(Prim::Sprt));
    };
    let player = s.k8(kart::PLAYER);
    if s.special.0 != -1 {
        let y = 0x78;
        if player == s.special.2 as i32 {
            let f6 = s.special.1 as i32;
            if f6 < 0x5A && s16(f6 % 0x1E) < 0xF {
                text(&mut out, &t.special_move, 0x100, y, Size::Large, 5);
            }
        } else if s.k8(kart::SPECIAL) != -1 {
            let tm = s.k16(kart::SPECIAL_TIMER);
            let x = s16((tm << 10) / 300 - 0x100);
            if s16(tm % 0x1E) < 0xF {
                text(&mut out, &t.special_move, x, y, Size::Large, 7);
            }
            let c = s.kart[kart::SPECIAL];
            if t.chr(c, 0x15) != 0 {
                let (u, v, sl) = (t.chr(c, 0x12), t.chr(c, 0x13), t.chr(c, 0x14));
                out.extend(quad(t, x - 0xA0, y, 0x18, 0x18, 0, u, v, 0x2F, 0x2F, [0x80; 3], sl, 0, 0x60).map(Prim::Ft4));
                out.extend(quad(t, x + 0xA0, y, 0x18, 0x18, 0, u.wrapping_add(0x2F), v, -0x2F, 0x2F, [0x80; 3], sl, 0, 0x60).map(Prim::Ft4));
            }
        }
    }
    if s.kart[kart::ICON_4F9] & 0x7F == 0 {
        out.extend(arrow(ht, t, s, 0x3C, s.k16(kart::ARROW)));
    }
    let msg = s.k8(kart::MESSAGE);
    if msg == -1 {
        return out;
    }
    let y = if s.mode == 2 { 0xD2 } else { 0xB4 };
    let val = s.k16(kart::MSG_VALUE);
    let signed = |t: &ItemTables| if val < 0 { t.fmt_neg.replace("%i", &val.to_string()) } else { t.fmt_pos.replace("%i", &val.to_string()) };
    let by_sign = if val > 0 { 5 } else { 7 };
    let (txt, colour) = match msg - 10 {
        0 | 1 | 3 | 5 => {
            let tail = if msg == 15 {
                if val.abs() >= 2 { t.hits.clone() } else { t.hit.clone() }
            } else {
                t.messages[msg as usize].clone()
            };
            (signed(t) + &tail, by_sign)
        }
        2 => {
            let a = t.sin((s.frame & 0x3F) * 0x40);
            let ang = sra_tz(a, 7) & 0xFFF;
            out.extend(quad(t, 0x100, y, 0x14, 0x14, ang, 0x90, 0x90, 0x2F, 0x2F, [0x80; 3], 9, 0, 0x60).map(Prim::Ft4));
            return out;
        }
        4 => {
            let s = if val == 1 { t.new_vehicle.clone() } else { t.messages[msg as usize].replace("%i", &val.to_string()) };
            (s, by_sign)
        }
        _ => (t.messages.get(msg as usize).cloned().unwrap_or_default(), 5),
    };
    text(&mut out, &txt, 0x100, y, Size::Large, colour);
    out
}

/// FUN_80032a40(y, angle, player) in a 1-player race: "WRONG WAY" when kart
/// +0x503 > 0xE, then the 3D outline arrow pointing along `angle`, projected
/// with the player's camera matrix and drawn as 7 white LineF2.
pub fn arrow(ht: &HudTables, t: &ItemTables, s: &ItemHud, y: i32, angle: i32) -> Vec<Prim> {
    let mut out = Vec::new();
    if !matches!(s.mode, 1 | 2 | 7 | 0x14) {
        return out;
    }
    let sn = t.sin(angle);
    let cs = t.cos(angle);
    if s.k8(kart::WRONG_WAY) > 0xE {
        out.extend(ht.text(t.wrong_way.as_bytes(), 0x100, y, 0, Size::Medium, 7, 0x60).into_iter().map(Prim::Sprt));
    }
    let m = s.cam;
    let mut px = [0i32; 8];
    let mut py = [0i32; 8];
    if s.e38 < 7 {
        let k = 0xC;
        for i in 0..8 {
            let x = t.arrow_x[i] as i32;
            let z = t.arrow_z[i] as i32;
            let xr = s16(sra_tz(x * cs + z * sn, 14));
            let b = z * cs - x * sn;
            let zr = (if b < 0 { b + 0x3FFF } else { b }).wrapping_shl(2) >> 16;
            let cx = sra_tz(xr * m[0][0] as i32 + zr * m[0][2] as i32, 12);
            let cy = sra_tz(xr * m[1][0] as i32 + zr * m[1][2] as i32, 12);
            let cz = sra_tz(xr * m[2][0] as i32 + zr * m[2][2] as i32, 12);
            let zz = s16(cz + 0x300);
            let den = zz * 0x140;
            px[i] = s16(if den == 0 { 0 } else { (cx << 16) / den });
            let ny = s16(cy + 0x180) << 7;
            py[i] = s16(if zz == 0 { 0 } else { ny / zz });
        }
        let y7 = py[7];
        for i in 0..7 {
            py[i] = s16(py[i] - y7);
            px[i] = s16(sra_tz(px[i] * k, 5));
            py[i] = s16(sra_tz(py[i] * k, 5));
        }
    } else {
        let k = 0xF;
        for i in 0..7 {
            let x = t.arrow_x[i] as i32;
            let z = t.arrow_z[i] as i32;
            let a = x * cs + z * sn;
            let b = z * cs - x * sn;
            let b = (if b < 0 { b + 0x3FFF } else { b }).wrapping_shl(2) >> 16;
            px[i] = s16((s16(sra_tz(a, 14)) * k * 0x200) / 0x14000);
            let c = -(b * k);
            py[i] = s16(sra_tz(c, 8));
        }
    }
    for i in 0..7 {
        let j = (i + 1) % 7;
        out.push(Prim::Line { v: [[s16(px[i] + 0x100), s16(y + py[i])], [s16(px[j] + 0x100), s16(y + py[j])]], rgb: [0xFF; 3] });
    }
    out
}

/// FUN_8004cbc8(y, kart) + FUN_8004c184 (1 player, x = 0x30): the two
/// weapon slots at y - 0xE and y + 0xE, each its icon then the charge ring.
pub fn weapons(t: &ItemTables, s: &ItemHud, y: i32) -> Vec<Prim> {
    let mut out = Vec::new();
    let pl = (s.kart[kart::PLAYER] & 1) as usize;
    for slot in 0..2usize {
        let scale = s.pulse.0[pl ^ slot] as i32;
        let yy = s16(y + if slot == 0 { -0xE } else { 0xE });
        icon(t, s, &mut out, 0x30, yy, slot, scale);
        let charge = s.kart[kart::CHARGE + slot] as i8;
        if charge < 8 {
            let h = s16(scale) / 2;
            let abr = if charge == -1 { 0x60 } else { 0 };
            out.extend(quad(t, 0x30, yy, h, h, 0, 0xC0, 0x80, 0x37, 0x37, [0x80; 3], 0x15, 0xB, abr).map(Prim::Ft4));
        }
    }
    out
}

/// FUN_8004c184(x, y, kart, slot, scale): the weapon icon of a slot. The
/// charge timer +0x4D4+slot: -1 empty; 0x40 or |t| < 4 the held weapon
/// +0x4D0+2*slot opaque; else the next one +0x4D1+2*slot then the held one,
/// semi-transparent (except type 3).
fn icon(t: &ItemTables, s: &ItemHud, out: &mut Vec<Prim>, x: i32, y: i32, slot: usize, scale: i32) {
    let timer = s.kart[kart::CHARGE + slot] as i8 as i32;
    if timer == -1 {
        return;
    }
    let primary = s.kart[kart::WEAPONS + 2 * slot] as i8;
    let opaque = timer == 0x40 || timer / 4 == 0;
    if !opaque {
        let second = s.kart[kart::WEAPONS + 2 * slot + 1] as i8;
        if second != -1 {
            if let Some(q) = weapon_quad(t, s, x, y, second, scale, false) {
                out.push(Prim::Ft4(q));
            }
        }
    }
    if primary == -1 {
        return;
    }
    if let Some(q) = weapon_quad(t, s, x, y, primary, scale, opaque) {
        out.push(Prim::Ft4(q));
    }
}

fn weapon_quad(t: &ItemTables, s: &ItemHud, x: i32, y: i32, kind: i8, scale: i32, opaque: bool) -> Option<Quad> {
    let f = s.frame;
    let abr = if opaque { 0x60 } else { 0 };
    let sc = s16(scale);
    match kind {
        0 => {
            let h = sc * 0x2A / 0x50;
            quad(t, x, y, h, h, (f & 0x3F) << 6, 0x40, 0x56, 0x29, 0x29, [0x80; 3], 0x15, 5, abr)
        }
        1 => quad(t, x, y, sc * 0x14 / 0x60, sc * 0x38 / 0x60, (f & 0x3F) << 6, 0xC0, 0xC0, 0x13, 0x37, [0x80; 3], 0x15, 0xF, abr),
        2 => {
            let ang = sra_tz(t.sin((f & 0x3F) * 0x40), 7) & 0xFFF;
            let idx = ((s16((f & 0x3F) * 0x40 + 0x92) & 0xFFF) / 0x125) as usize;
            let fr = t.anim.get(idx).copied().unwrap_or([0; 3]);
            let h = sc / 2;
            quad(t, x, y, h, h, ang, fr[0] as u8, fr[1] as u8, fr[2] as i32, 0x1F, [0x80; 3], 0x14, 7, abr)
        }
        3 => {
            let hw = sra_tz(sc * 0x28, 7);
            let hh = sra_tz(sc * 0x3E, 7);
            let ang = sra_tz(t.sin((f & 0xF) * 0x100), 7) & 0xFFF;
            quad(t, x, y, hw, hh, ang, 0x40, 0x82, 0x27, 0x3D, [0x80; 3], 0x14, 9, 0x60)
        }
        _ => None,
    }
}

/// FUN_8004a310 (1 player, y base 0x30): the turbo meter. Two additive
/// sparkle quads at x = 0x1D0 + slide (+0x47E; 96 = parked off-screen,
/// 0 when the meter is full), the item ring quad, the thermometer (two
/// SPRT at 0x230 - slide) and its fill from +0x48A (0..0x1A4 -> 0..0x22 px).
pub fn meter(t: &ItemTables, s: &ItemHud) -> Vec<Prim> {
    let mut out = Vec::new();
    let y0 = 0x30;
    let pl = (s.kart[kart::PLAYER] & 1) as usize;
    let slide = le::u16(&s.kart, kart::METER_SLIDE) as i32;
    let x = s16(slide + 0x1D0);
    let sp = &t.sparkle;
    let p0 = s.pulse.0[pl] as i32;
    let sz = |k: i32| {
        let v = p0 * k * 0x20;
        (if v < 0 { v + 0x3FFF } else { v }).wrapping_shl(2) >> 16
    };
    let a = sz(sp[3] as i32);
    out.extend(quad(t, x, y0 - 0xE, a, a, (s.frame & 0x3F) << 6, 0xA0, 0xC0, 0x1F, 0x1F, [sp[0] as u8, sp[1] as u8, sp[2] as u8], 0x15, 0xE, 0x20).map(Prim::Ft4));
    let b = sz(sp[7] as i32);
    out.extend(quad(t, x, y0 - 0xE, b, b, (s.frame & 0x3F) * -0x40 + 0xFFF, 0xA0, 0xC0, 0x1F, 0x1F, [sp[4] as u8, sp[5] as u8, sp[6] as u8], 0x15, 0xE, 0x20).map(Prim::Ft4));
    let p1 = s.pulse.0[pl ^ 1] as i32;
    out.extend(quad(t, x, y0 | 0xE, (p1 << 6) / 0x60, (p1 * 0x2D) / 0x60, 0, 0x40, 0x80, 0x3F, 0x2C, [0x80; 3], 0x15, 9, 0x60).map(Prim::Ft4));
    let x2 = s16(0x230 - slide);
    out.push(Prim::Sprt(sprt(x2 - 0xC, y0 - 0x13, 0, 0xC0, 0x18, 0x23, [0x80; 3], 0x14, 0xC, 0x20)));
    out.push(Prim::Sprt(sprt(x2 - 0x11, y0 - 0x13, 0x18, 0xC0, 0x22, 0x25, [0x80; 3], 0x14, 0xC, 0x60)));
    let fill = s16(s.k16(kart::TURBO) * 0x22 / 0x1A4);
    if fill > 0 {
        out.push(Prim::Sprt(sprt(x2 - 0xF, y0 - (fill - 0x11), 0x60, (0x22 - fill) as u8, 0x1E, fill, [0x80; 3], 0x15, 3, 0x60)));
    }
    out
}

/// FUN_80048a0c (1 player race): the kart markers (FUN_800307d4 PolyF3
/// arrows; the human player's blinks with the race clock), then the map
/// sprite (slot 5) last so it is drawn first. Battle/special branches not
/// ported (07E6 / mode 0x14).
pub fn minimap(t: &ItemTables, s: &ItemHud) -> Vec<Prim> {
    let mut out = Vec::new();
    let c = s.course as usize;
    let use_table = t.course_flags.get(c).is_some_and(|f| f & 2 != 0);
    if !use_table {
        // The default bounds live in globals (DAT_800B0B6C..) set at load; not
        // needed on DOCK.
        return out;
    }
    let m = &t.maps[c * 0x14..c * 0x14 + 0x14];
    let (u, v, w, h) = (m[0], m[1], m[2] as i32, m[3] as i32);
    let x0 = le::i32(m, 4);
    let z0 = le::i32(m, 8);
    let dx = (le::i32(m, 12) - x0).abs();
    let dz = (le::i32(m, 16) - z0).abs();
    let ox = 0x1E0 - w;
    let oy = 0xE1 - h;
    for (i, k) in s.karts.iter().enumerate() {
        // knocked out (race flag 8, battles): off the map
        if (s.mode == 2 || s.mode == 7 || s.race_flags & 8 != 0) && k.damage >= 1000 {
            continue;
        }
        let mx = ox + w * ((k.x >> 5) - x0) / dx;
        let my = oy - h * ((k.z >> 5) - z0) / dz;
        if k.flags & 0x60 == 0 {
            let (a, b) = if i < s.players {
                if s.race_time % 0x96 > 0x4A {
                    continue;
                }
                (5, 10)
            } else {
                (4, 8)
            };
            let ch = k.character;
            if let Some(p) = marker(t, s16(mx), s16(my), k.heading as i32, [t.chr(ch, 0xF), t.chr(ch, 0x10), t.chr(ch, 0x11)], a, b) {
                out.push(p);
            }
        } else {
            let a = t.sin((s.frame & 0x3F) * 0x40) / 0x60;
            let ang = if k.flags & 0x20 == 0 { -a } else { a };
            let ch = k.character;
            out.extend(quad(t, s16(mx), s16(my), 10, 10, ang & 0xFFF, t.chr(ch, 0x12), t.chr(ch, 0x13), 0x2F, 0x2F, [0x80; 3], t.chr(ch, 0x14), 0, 0x60).map(Prim::Ft4));
        }
    }
    out.push(Prim::Sprt(sprt(ox, oy, u, v, w, h, [0x80; 3], 5, ((v >> 6) as u16) * 4 + (u >> 6) as u16, 0x60)));
    out
}

/// FUN_800307d4(x, y, heading, r, g, b, a, b): a flat triangle (0, -b),
/// (a, a), (-a, a) rotated by the heading, x / 0x2800 (1.6 x the y scale).
fn marker(t: &ItemTables, x: i32, y: i32, heading: i32, rgb: [u8; 3], a: i32, b: i32) -> Option<Prim> {
    let lx = [0, a, -a];
    let ly = [-b, a, a];
    let s = t.sin(heading);
    let c = t.cos(heading);
    let mut v = [[0i32; 2]; 3];
    for i in 0..3 {
        let e = lx[i] * s + ly[i] * c;
        v[i][0] = s16(x + s16((lx[i] * c - ly[i] * s) / 0x2800));
        v[i][1] = s16(y + s16(sra_tz(e, 14)));
    }
    let xs = v.map(|p| p[0]);
    let ys = v.map(|p| p[1]);
    let vis = xs.iter().any(|&q| q >= 0) && xs.iter().any(|&q| q < 0x200) && ys.iter().any(|&q| q >= 0) && ys.iter().any(|&q| q < 0xF0);
    vis.then_some(Prim::F3 { v, rgb })
}

/// FUN_80049FD8(x, y, kart, 1): the damage gauge of the race flag 8 (DISCO)
/// - the white fill (what is left: 20 - damage / 50 rows from the bottom),
/// the heart (semi-transparent B/2 + F/2, flickering with three random
/// bytes from 0x321 damage) and its outline; texture slot 0x15, CLUT + 3.
/// Damage = kart +0x43E. [CODE 80049fd8, 8004f688]
pub fn damage_gauge(s: &ItemHud, x: i32, y: i32) -> Vec<Prim> {
    let d = le::i16(&s.kart, 0x43E) as i32;
    let rgb = s.gauge_rgb.unwrap_or([0x80; 3]);
    let fill = (0x14 - d / 0x32) as i16 as i32;
    let mut out = Vec::new();
    if fill > 0 {
        out.push(Prim::Sprt(sprt(x - 0xD, y - (fill - 10), 0xE2, (0x40 - fill) as u8, 0x1A, fill, [0x80; 3], 0x15, 3, 0x60)));
    }
    out.push(Prim::Sprt(sprt(x - 0xD, y - 10, 0xE2, 0x2C, 0x1A, 0x14, rgb, 0x15, 3, 0)));
    out.push(Prim::Sprt(sprt(x - 0x11, y - 0xC, 0xC0, 0x28, 0x22, 0x18, [0x80; 3], 0x15, 3, 0x60)));
    out
}

/// FUN_8002f5e4(x, y, w, h, r, g, b): an opaque flat rectangle.
fn tile(x: i32, y: i32, w: i32, h: i32, rgb: [u8; 3]) -> Prim {
    Prim::Tile { x: s16(x), y: s16(y), w: s16(w), h: s16(h), rgb }
}

/// FUN_80047F1C (1 player, y 0x1E; the stunt trial, racing): the kart's
/// face on the bar at 0x80 + its strip place * 0x100 / the strip's length
/// (shown while race clock % 150 <= 0x4A; the face spins with flags
/// 0x60), the start banner's mark (black; white with race flag 4), the
/// finish banner's chequer, then the bar (0x100 x 4, black), in add order.
pub fn progress_bar(t: &ItemTables, s: &ItemHud, sh: &crate::hud::StuntHud) -> Vec<Prim> {
    let y = 0x1E;
    let mut out = Vec::new();
    let span = (sh.quads as i32) << 7;
    let c = sh.character;
    if sh.kart_flags & 0x60 == 0 {
        if sh.quads != 0 && s.race_time % 0x96 <= 0x4A && span != 0 {
            let x = (sh.progress * 0x100) / span + 0x80;
            out.extend(quad_f(t, x, y, 8, 8, 0, 0, t.chr(c, 0x12), t.chr(c, 0x13), 0x2F, 0x2F, [0x80; 3], t.chr(c, 0x14), 0, 0x60).map(Prim::Ft4));
        }
    } else if span != 0 {
        let a = (t.sin((s.frame & 0x3F) * 0x40) as i16 as i32) / 0x60;
        let ang = (if sh.kart_flags & 0x20 == 0 { -a } else { a }) & 0xFFF;
        let x = (sh.progress * 0x100) / span + 0x80;
        out.extend(quad_f(t, x, y, 10, 10, 0, ang, t.chr(c, 0x12), t.chr(c, 0x13), 0x2F, 0x2F, [0x80; 3], t.chr(c, 0x14), 0, 0x60).map(Prim::Ft4));
    }
    let mark = |i: usize| if (i as u8) < sh.n_marks && span != 0 { Some((sh.marks[i] << 8) / span + 0x80) } else { None };
    let rev = sh.race_flags & 4 != 0;
    let (w, b) = ([0xFF; 3], [0; 3]);
    if let Some(x) = mark(0) {
        if rev {
            out.extend([tile(x - 3, y - 3, 3, 2, w), tile(x - 3, y + 1, 3, 2, w), tile(x - 3, y - 5, 3, 10, b), tile(x, y - 3, 3, 2, b), tile(x, y + 1, 3, 2, b), tile(x, y - 5, 3, 10, w)]);
        } else {
            out.push(tile(x - 2, y - 5, 4, 10, b));
        }
    }
    if let Some(x) = mark(1) {
        if !rev {
            out.extend([tile(x - 3, y - 3, 3, 2, b), tile(x - 3, y + 1, 3, 2, b), tile(x - 3, y - 5, 3, 10, w), tile(x, y - 3, 3, 2, w), tile(x, y + 1, 3, 2, w), tile(x, y - 5, 3, 10, b)]);
        } else {
            out.push(tile(x - 2, y - 6, 4, 10, b));
        }
    }
    out.push(tile(0x80, y - 2, 0x100, 4, b));
    out
}

/// The stunt trial's HUD (race type 3; FUN_80051698 / FUN_8004F688 case
/// 3, case 10 -> FUN_8004E2A8), in add order: the +0x4F9 icon; while
/// counting down "READY" (blinking) or, above the start value + 15,
/// "FINISH THE COURSE"; racing: the turbo meter, the weapon slots, the
/// progress bar, the timer (0x100, 0xD2); out (+0x4C7) or in game mode 10:
/// the result; mode 10: "PRESS START" (blinking).
pub fn stunt_hud(ht: &HudTables, t: &ItemTables, st: &crate::hud::HudState, s: &ItemHud, sh: &crate::hud::StuntHud) -> Vec<Prim> {
    let p = parts(ht, t, s);
    let mut out = p.icon;
    let c70 = st.countdown;
    let out_of_race = st.finished || st.mode == 10;
    if out_of_race {
        out.extend(ht.stunt_results(sh).into_iter().map(Prim::Sprt));
    } else if c70 == 0 {
        out.extend(meter(t, s));
        out.extend(weapons(t, s, 0x2D));
    }
    if st.mode != 10 && c70 == 0 {
        out.extend(progress_bar(t, s, sh));
    }
    if st.mode == 10 {
        if let Some(r) = st.results {
            if r.press_start && r.frame.rem_euclid(0x3C) < 0x2D {
                out.extend(ht.text(ht.press_start.as_bytes(), 0x100, 0xD2, 0, Size::Medium, 4, 0x60).into_iter().map(Prim::Sprt));
            }
        }
        return out;
    }
    // FUN_80050F64
    if c70 != 0 {
        if c70 < st.countdown_start {
            if c70 <= 0x77 && c70 % 0x1E >= 0xF {
                out.extend(ht.text(ht.ready.as_bytes(), 0x100, 0xD2, 0, Size::Large, 6, 0x60).into_iter().map(Prim::Sprt));
            }
        } else if c70 >= st.countdown_start + 0xF {
            out.extend(ht.text(ht.finish_the_course.as_bytes(), 0x100, 0xD2, 0, Size::Large, 5, 0x60).into_iter().map(Prim::Sprt));
        }
        return out;
    }
    // FUN_80050B2C
    out.extend(ht.text(crate::hud::timer_text(st.race_time).as_bytes(), 0x100, 0xD2, 0, Size::Medium, 4, 0x60).into_iter().map(Prim::Sprt));
    out
}

/// A battle's HUD (game mode 2; FUN_80051698 / FUN_8004F688 case 2, case
/// 10 -> FUN_8004DAE0; phase 4, docs/spec/battle.md), in add order: the
/// +0x4F9 icon; racing: SPECIAL MOVE / arrow / banner, the weapon slots,
/// the turbo meter, the damage gauge (0x30, 0xCF), the minimap; out
/// (+0x4C7) or in game mode 10: the result; while counting down "READY"
/// (blinking) or "BATTLE THE MUPPETS" with the opponents' faces; racing:
/// the clock (0x100, 0x1E); mode 10: "PRESS START" (blinking).
pub fn battle_hud(ht: &HudTables, t: &ItemTables, st: &crate::hud::HudState, s: &ItemHud, bh: &crate::hud::BattleHud) -> Vec<Prim> {
    let p = parts(ht, t, s);
    let mut out = p.icon;
    let c70 = st.countdown;
    let sprites = |v: Vec<crate::hud::Sprite>| v.into_iter().map(Prim::Sprt);
    if c70 <= 0 {
        if st.finished || st.mode == 10 {
            out.extend(sprites(ht.battle_results(bh)));
        } else {
            out.extend(status(ht, t, s));
            out.extend(weapons(t, s, 0x2D));
            out.extend(meter(t, s));
            out.extend(damage_gauge(s, 0x30, 0xCF));
        }
        if st.mode != 10 {
            out.extend(minimap(t, s));
        }
    }
    if st.mode == 10 {
        if let Some(r) = st.results {
            if r.press_start && r.frame.rem_euclid(0x3C) < 0x2D {
                out.extend(sprites(ht.text(ht.press_start.as_bytes(), 0x100, 0xD2, 0, Size::Medium, 4, 0x60)));
            }
        }
        return out;
    }
    // FUN_80050F64
    if c70 != 0 {
        if c70 < st.countdown_start {
            if c70 <= 0x77 && c70 % 0x1E >= 0xF {
                out.extend(sprites(ht.text(ht.ready.as_bytes(), 0x100, 0xD2, 0, Size::Large, 6, 0x60)));
            }
        } else if !(s.e38 > 6 && c70 < st.countdown_start + 0xF) {
            out.extend(sprites(ht.text(ht.battle_the_muppets.as_bytes(), 0x100, 0xD2, 0, Size::Large, 5, 0x60)));
            out.extend(intro_faces(t, s));
        }
        return out;
    }
    // FUN_80050B2C: the race clock at the top in mode 2
    out.extend(sprites(ht.text(crate::hud::timer_text(st.race_time).as_bytes(), 0x100, 0x1E, 0, Size::Medium, 4, 0x60)));
    out
}

/// The whole 1-player race HUD in the game's add order (FUN_80051698 ->
/// FUN_8004f688 for the player, then the minimap; the timer FUN_80050b2c
/// last): the minimal HUD of `HudTables::layout` with the item parts
/// inserted where the game adds them.
pub fn race_hud(ht: &HudTables, t: &ItemTables, st: &crate::hud::HudState, s: &ItemHud) -> Vec<Prim> {
    if let Some(sh) = &st.stunt {
        return stunt_hud(ht, t, st, s, sh);
    }
    if let Some(bh) = &st.battle {
        return battle_hud(ht, t, st, s, bh);
    }
    let p = parts(ht, t, s);
    let base: Vec<Prim> = ht.layout(st).into_iter().map(Prim::Sprt).collect();
    let racing = st.countdown <= 0 && st.mode == 1 && !st.finished;
    let mut out = Vec::new();
    out.extend(p.icon);
    if !racing {
        let intro = st.countdown != 0 && st.countdown >= st.countdown_start && st.mode == 1;
        // FUN_80050F64: on the overhead-camera courses (cup > 6) no intro
        // text in its last 15 frames
        if intro && s.e38 > 6 && st.countdown < st.countdown_start + 0xF {
            return out;
        }
        out.extend(base);
        if intro {
            out.extend(intro_faces(t, s));
        }
        return out;
    }
    // layout(): [place digit, suffix, LAP, n/m, (timer)] when racing
    let (mut head, mut tail) = (Vec::new(), Vec::new());
    // Split the minimal layout: place sprites (y < 0x40), LAP/n-m (x < 0x80), timer (rest).
    for s in base {
        match s {
            Prim::Sprt(sp) if sp.y < 0x40 => head.push(Prim::Sprt(sp)),
            other => tail.push(other),
        }
    }
    let (lap, timer): (Vec<Prim>, Vec<Prim>) = tail.into_iter().partition(|p| matches!(p, Prim::Sprt(sp) if sp.x < 0x80));
    out.extend(p.status);
    out.extend(head);
    out.extend(p.weapons);
    out.extend(lap);
    // race flag 8: the damage gauge after "LAP n/m" (FUN_8004F688 case 1)
    if s.race_flags & 8 != 0 {
        out.extend(damage_gauge(s, 0x30, 0xB4));
    }
    out.extend(p.meter);
    out.extend(minimap(t, s));
    out.extend(timer);
    out
}
