//! Animated textures (RAW type 0x22 strips): the game copies one frame of
//! each strip into VRAM every race frame, FUN_8007A9DC -> FUN_800311AC(slot,
//! 0, frame) [CODE 8007a9dc, 800311ac]; docs/spec/effects.md "Animated
//! textures". On DOCK: the water (slots 0xA, 0xC), the turbo-meter and
//! weapon-slot stars (0x19, 0x27; texture slot 0x15), and 0x1E.
//!
//! - Frame per slot from the frame counter DAT_800B0D90 (course 3):
//!   0xA: (f & 0x3F) >> 2; 0xB: (f % 0x36) / 2; 0xC: (f & 0x3F) >> 3
//!   (>> 4 on other courses); 0xD: (f & 0x3F) >> 2; 0x19, 0x27:
//!   (f & 0x1F) >> 1. Slot 0x1E (FUN_8005C2EC): 7 - ((c & 0xF) >> 1) of
//!   the counter DAT_800B07D4 (+dt every frame; its phase is approximated
//!   by the frame counter, render ledger).
//! - Destination: the sprite definition *0x800D1B40[slot] (exe table
//!   0x800ACF40): its texture slot's page + frame 0's (u, v); 4-bit strips
//!   (DAT_800BBB20) are w/4 halfwords wide, 8-bit (DAT_800CB680) w/2.

use bevy::prelude::*;
use mr_assets::raw::{self, AnimStrip};

use crate::vramtex::VramTextures;

struct Strip {
    slot: u32,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    frames: Vec<Vec<u16>>,
    shown: Option<usize>,
}

/// A per-course VRAM scroll of FUN_8007A9DC's switch (FUN_8005E38C(slot,
/// u, v, w, h, 0, sy, du, dv)): the w x h texels at (u, v) of texture slot
/// `slot` become the same-sized block at (u + du, v + dv) scrolled up by sy
/// rows (wrapping), every race frame. [CODE 8007a9dc, 8005e38c]
struct Scroll {
    /// Destination and source, VRAM halfwords (4-bit texels / 4).
    x: usize,
    y: usize,
    sx: usize,
    sy: usize,
    w: usize,
    h: usize,
    /// The row scroll's source.
    rows: Rows,
    shown: Option<usize>,
}

/// Where a scroll's row offset comes from: the frame counter, or (course
/// 0x17) lift i's animation counter +0x13 (anim & 0x3F).
#[derive(Clone, Copy)]
enum Rows {
    Frame(fn(i32) -> usize),
    Lift(usize),
}

/// FUN_8007A9DC's switch on the course (0x800B0FAC): (slot, u, v, w, h, du,
/// dv, rows), every FUN_8005E38C call of the case in order. Phase 14: every
/// case (before: GALLERY, ISLAND, NEW_YORK, DISCO only): the arenas CANYON
/// (5), LAB (0xE), T_SQUARE (0x13), the stunt courses
/// MOUNTAIN (0xC), FUN (0x16), REST (0x17: its lifts' strips), DINER (0x18),
/// MLABS (0x19), and THE END (0x1D:
/// texture slot 2, the source below the rectangle).
fn scrolls(course: u32) -> Vec<(usize, i32, i32, i32, i32, i32, i32, Rows)> {
    fn up(f: i32) -> usize {
        (f & 0x3F) as usize
    }
    fn down(f: i32) -> usize {
        ((f & 0x1F) * -2 + 0x3F) as usize
    }
    fn down4(f: i32) -> usize {
        ((f & 0xF) * -4 + 0x3F) as usize
    }
    fn steps(f: i32) -> usize {
        (((f & 0x1FF) >> 5) << 2) as usize
    }
    let (up, down) = (Rows::Frame(up), Rows::Frame(down));
    match course {
        4 => vec![(5, 0x40, 0x40, 8, 0x40, 0, -0x40, up)],
        5 => vec![(5, 0x40, 0xC0, 0x40, 0x40, 0, -0x40, Rows::Frame(down4))],
        6 | 0x13 => vec![(5, 0xC0, 0xC0, 0x40, 0x40, 0, -0x40, down)],
        0xC => vec![(5, 0xC0, 0xC0, 0x40, 0x40, 0, -0x40, down), (5, 0x80, 0xC0, 0x40, 0x40, 0, -0x40, down)],
        0xE => vec![(5, 0, 0x40, 0x40, 0x40, 0, -0x40, up)],
        0x11 | 0x16 | 0x18 => vec![(5, 0xC0, 0xC0, 0x40, 0x40, 0, -0x40, up)],
        // lifts 3..8: an 8-texel strip each at u 0xC0 + 8 k
        0x17 => (0..6).map(|k| (5, 0xC0 + 8 * k as i32, 0xC0, 8, 0x40, 0, -0x40, Rows::Lift(k + 3))).chain(std::iter::once((5, 0x80, 0xC0, 4, 0x40, 0, -0x40, Rows::Frame(steps)))).collect(),
        0x19 => vec![(5, 0xC0, 0xC0, 0x40, 0x40, 0, -0x40, up), (5, 0x80, 0xC0, 0x40, 0x40, 0, -0x40, up)],
        0x1D => vec![(2, 0xE0, 0, 0x20, 0x40, 0, 0x40, up)],
        0x1F => vec![(5, 0xC0, 0, 0x40, 0x40, -0x40, 0, up), (5, 0x40, 0xC0, 0x40, 0x40, 0, -0x40, up)],
        _ => Vec::new(),
    }
}

#[derive(Resource, Default)]
struct AnimTex {
    strips: Vec<Strip>,
    scrolls: Vec<Scroll>,
}

pub struct AnimTexPlugin;

impl Plugin for AnimTexPlugin {
    fn build(&self, app: &mut App) {
        crate::boot::on_boot(app, |w| w.insert_resource(AnimTex::default()));
        app.add_systems(crate::boot::Boot, setup.after(crate::scene::setup)).add_systems(Update, step);
    }
}

fn exe_u32(exe: &[u8], a: u32) -> u32 {
    let o = (a - 0x8001_0000) as usize + 0x800;
    u32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]])
}

fn setup(mut at: ResMut<AnimTex>) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let path = raw::course_raw_path(&exe, crate::courses::id()).expect("course");
    let file = disc.read(&path).expect("read RAW");
    let env = raw::LoadEnv::from_exe(&exe, crate::courses::id()).expect("env");
    let dec = raw::decode(&file, &env).expect("decode RAW");
    // dev: MRM_ANIM_DUMP=PATH writes every strip's frames (VRAM halfwords)
    // as JSON, to match oracle read_vram dumps (timing checks)
    if let Ok(path) = std::env::var("MRM_ANIM_DUMP") {
        let mut j = String::from("{");
        for (n, a) in dec.anims.iter().enumerate() {
            let fr: Vec<String> = a.frames.iter().map(|f| format!("{:?}", f)).collect();
            j += &format!("{}\"{}\": [{}]", if n > 0 { ", " } else { "" }, a.slot, fr.join(", "));
        }
        j += "}";
        let _ = std::fs::write(path, j);
    }
    // FUN_8005E38C's rectangles: x = slot x + u / 4 (C division), y =
    // slot y + v; the source du / 4, dv away
    // dev: MRM_NO_SCROLL leaves them out (side-by-sides)
    let list = if std::env::var("MRM_NO_SCROLL").is_ok() { Vec::new() } else { scrolls(crate::courses::id()) };
    for (slot, u, v, w, h, du, dv, rows) in list {
        let Some(ts) = dec.slots.get(slot).copied().flatten() else { continue };
        let q = |a: i32| (if a < 0 { a + 3 } else { a }) >> 2;
        let (x, y) = (ts.x as i32 + q(u), ts.y as i32 + v);
        at.scrolls.push(Scroll { x: x as usize, y: y as usize, sx: (x + q(du)) as usize, sy: (y + dv) as usize, w: q(w) as usize, h: h as usize, rows, shown: None });
    }
    for a in &dec.anims {
        if let Some(s) = place(&exe, &dec.slots, a) {
            if std::env::var("MRM_ANIM_DEBUG").is_ok() {
                eprintln!("animtex strip 0x{:X}: vram ({}, {}) {}x{} halfwords, {} frames", s.slot, s.x, s.y, s.w, s.h, s.frames.len());
            }
            at.strips.push(s);
        }
    }
}

/// Where FUN_800311AC puts frames of `a` (4-bit strips; the RAW decoder
/// splits frames as 4-bit, which every DOCK strip is).
fn place(exe: &[u8], slots: &[Option<raw::TexSlot>; 32], a: &AnimStrip) -> Option<Strip> {
    let def = exe_u32(exe, 0x800A_CF40 + 4 * a.slot);
    if def == 0 {
        return None;
    }
    let o = (def - 0x8001_0000) as usize + 0x800;
    let tex_slot = u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
    let (u, v) = (exe[o + 8] as usize, exe[o + 9] as usize);
    let ts = slots.get(tex_slot)?.as_ref()?;
    let tp = ts.tpage?;
    let (px, py, _) = mr_assets::vram::tpage(tp);
    Some(Strip { slot: a.slot, x: px + (u >> 2), y: py + v, w: a.w as usize / 4, h: a.h as usize, frames: a.frames.clone(), shown: None })
}

/// The frame FUN_8007A9DC (or FUN_8005C2EC for 0x1E) shows at frame count `f` on DOCK.
pub fn frame_for(slot: u32, f: i32) -> Option<usize> {
    let r = match slot {
        0xA | 0xD => (f & 0x3F) >> 2,
        0xB => (f % 0x36) / 2,
        0xC => (f & 0x3F) >> 3,
        0x19 | 0x27 => (f & 0x1F) >> 1,
        0x1E => 7 - ((f & 0xF) >> 1),
        _ => return None,
    };
    Some(r as usize)
}

fn step(
    p: Res<crate::Player>,
    cap: Option<Res<crate::capture::CaptureReplay>>,
    rows: Option<ResMut<crate::effects::CapRows>>,
    mut at: ResMut<AnimTex>,
    tex: Option<ResMut<VramTextures>>,
    mut pages: ResMut<crate::psxfx::FxPages>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut tex) = tex else { return };
    // an fx capture replay carries the game's frame counter
    let captured = match (cap, rows) {
        // (phase 14: less the captured dt, 3 in DISCO's heavy frames)
        (Some(c), Some(mut r)) => r.u32_at(c.vb, 0x800B_0D90).map(|v| v as i32 - r.u32_at(c.vb, 0x800B_0AEC).map_or(2, |d| (d as i32).clamp(1, 4))),
        _ => None,
    };
    // The counter steps after the frame's draw (S4 HUD timing): the frame
    // on screen was textured with the value one race frame (dt) earlier.
    // Phase 14 (DISCO's scrolls): FUN_8007916C's FUN_8007A9DC reads it
    // before main()'s FUN_8005C2EC adds dt (0x80078B1C / 0x80078B24); oracle
    // VRAM samples of DISCO's scrolled block = rows(counter before the step)
    // on 10 of 15 reads, the other 5 one step either way where the counter
    // and VRAM reads straddle a frame (tools/oracle/scroll_phase.py).
    // Side-by-sides against the oracle (VB 2510, 3010) put the item-box star
    // within one strip frame with this (two frames ahead without it).
    let f = match (&p.driver, captured) {
        (_, Some(f)) => f,
        (crate::Driver::Sim(run), None) => run.session.globals.frame - run.session.globals.dt.max(1) as i32,
        // trace replays: the frame counter advances 2 per tick
        _ => (p.tick * 2) as i32 - 2,
    };
    // dev: MRM_ANIM_OFFSET=n shifts the counter the strips see (timing checks)
    let f = f - std::env::var("MRM_ANIM_OFFSET").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    // course 0x17: the lifts' animation counters as FUN_8007A9DC reads them
    // (before the frame's FUN_800859EC step: undo it by the speed's sign)
    let lift_anim = |i: usize| -> Option<i32> {
        let crate::Driver::Sim(run) = &p.driver else { return None };
        let g = &run.session.globals;
        let l = g.objects.lifts.get(i)?;
        let dt = g.dt.max(1);
        Some(l.anim as i32 - if l.speed > 0 { dt } else if l.speed < 0 { -dt } else { 0 })
    };
    for s in at.scrolls.iter_mut() {
        let r = match s.rows {
            Rows::Frame(rows) => rows(f),
            Rows::Lift(i) => match lift_anim(i) {
                Some(a) => (a & 0x3F) as usize,
                None => continue,
            },
        }
        .min(s.h);
        if s.shown == Some(r) {
            continue;
        }
        // MoveImage of rows r.. then rows ..r (no column scroll on the
        // race courses)
        let mut data = Vec::with_capacity(s.w * s.h);
        for row in (r..s.h).chain(0..r) {
            for col in 0..s.w {
                data.push(tex.vram.get(s.sx + col, s.sy + row));
            }
        }
        tex.write_rect(&mut images, s.x, s.y, s.w, s.h, &data);
        pages.refresh(&tex.vram, &mut images, s.x, s.y, s.w, s.h);
        s.shown = Some(r);
    }
    for s in at.strips.iter_mut() {
        let Some(k) = frame_for(s.slot, f) else { continue };
        let k = k.min(s.frames.len().saturating_sub(1));
        if s.shown == Some(k) || s.frames.is_empty() {
            continue;
        }
        let data = &s.frames[k];
        if data.len() < s.w * s.h {
            continue;
        }
        tex.write_rect(&mut images, s.x, s.y, s.w, s.h, data);
        // the effects pass keeps its own (raw colour, STP) page images
        pages.refresh(&tex.vram, &mut images, s.x, s.y, s.w, s.h);
        s.shown = Some(k);
    }
}
