//! Pickup shadows: entries 6.. of the shadow list 0x800C8B38 (after the six
//! karts), built once at race load by FUN_8007B800 and drawn every frame by
//! FUN_8002ADA8 with the karts' shadows (docs/spec/effects.md "Pickup
//! shadows").
//!
//! FUN_8007B800 [CODE 8007b800]: for item boxes (slots 6 + i, size 8),
//! challenge objects (6 + 0x30 + i, size 7, only when shown) and turbo tokens
//! (6 + 0x40 + i, size 4), at the pickup's x, z (TER units << 5): the ground
//! under it (FUN_8003DDB0; skipped when no surface is found), y = that
//! height, the ground matrix FUN_800252A4(ab4, ab0) from FUN_8003CCBC(the
//! face normal), and four corners from table 0x800AC168 (dx, dz) * size / 16
//! rotated by it (view units). Tile: slot 0x14, u 0xA0, v 0, 0x20 x 0x20,
//! intensity 0x80, tpage add 0x40 (B - F). The scale byte +0x29 is the
//! pickup's draw scale, written by FUN_8007BBF0 each frame (so a collected
//! pickup's shadow shrinks and vanishes with it).

use bevy::prelude::*;
use mr_sim::mem::Mem;
use mr_sim::world::World;

use crate::kartshadow::{record_corners, shadow_prim, ShadowRec, ShadowState};
use crate::psxfx::FxPrims;

/// One pickup's shadow record and its pickup slot (0..0xC0).
#[derive(Clone, Debug)]
pub struct PickupShadow {
    pub pickup: usize,
    pub rec: ShadowRec,
}

fn exe_off(a: u32) -> usize {
    (a - 0x8001_0000) as usize + 0x800
}

/// FUN_8007B800 (challenge objects left out: hidden in a race).
pub fn build(w: &World, exe: &[u8]) -> Vec<PickupShadow> {
    let t = &w.tables;
    let corner = |k: usize| {
        let o = exe_off(0x800A_C168) + 4 * k;
        (i16::from_le_bytes([exe[o], exe[o + 1]]) as i32, i16::from_le_bytes([exe[o + 2], exe[o + 3]]) as i32)
    };
    let mut out = Vec::new();
    let groups = [(&w.pickups.boxes, 0usize, 8), (&w.pickups.tokens, 0x40, 4)];
    for (list, first, size) in groups {
        let Some(list) = list else { continue };
        for (i, p) in list.points.iter().enumerate() {
            let (x, z) = (p[0] << 5, p[2] << 5);
            let g = w.ter.ground(x, p[1] << 5, z);
            let Some(near) = g.nearest else { continue };
            let n = w.ter.faces[near.face].normal;
            let (ab0, ab4) = mr_sim::init::tilt_from_normal(w, [n[0] as i32, n[1] as i32, n[2] as i32]);
            let m = mr_sim::render::rot_lean(t, ab4, ab0);
            let mut offs = [[0i16; 3]; 4];
            for (k, o) in offs.iter_mut().enumerate() {
                let (dx, dz) = corner(k);
                let q = |v: i32| (if v < 0 { v + 0xF } else { v }) >> 4;
                let (dx, dz) = (q(dx * size), q(dz * size));
                for r in 0..3 {
                    o[r] = ((dx * m[r][0] as i32 + dz * m[r][2] as i32) >> 12) as i16;
                }
            }
            out.push(PickupShadow {
                pickup: first + i,
                rec: ShadowRec {
                    origin: [x, near.y, z],
                    offs,
                    uv0: [0xA0, 0],
                    size: [0x20, 0x20],
                    slot: crate::kartshadow::SHADOW_SLOT,
                    scale: 0,
                    count: 1,
                    intensity: 0x80,
                    tpage_add: 0x40,
                },
            });
        }
    }
    out
}

#[derive(Resource)]
pub struct PickupShadows(pub Vec<PickupShadow>);

/// The pickups' draw scales this frame (pickup slot -> +0x16 low byte), from
/// the live session or an fx capture.
fn scales(p: &crate::Player, cap: Option<&crate::capture::CaptureReplay>, rows: Option<&mut crate::effects::CapRows>) -> Option<Vec<u8>> {
    if let (Some(c), Some(r)) = (cap, rows) {
        let m: Mem = r.mem(c.vb)?;
        return Some((0..0xC0u32).map(|i| m.u8(0x800E_1B70 + 0x20 * i + 0x16)).collect());
    }
    let crate::Driver::Sim(run) = &p.driver else { return None };
    Some(run.session.items().pickups.iter().map(|q| if q.state != 0 { q.scale as u8 } else { 0 }).collect())
}

pub fn setup(mut commands: Commands) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let Ok(w) = World::course(&mut disc, crate::courses::info().id) else { return };
    commands.insert_resource(PickupShadows(build(&w, &exe)));
}

/// Push this frame's pickup shadows (after the kart ones: list order).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    p: Res<crate::Player>,
    cap: Option<Res<crate::capture::CaptureReplay>>,
    rows: Option<ResMut<crate::effects::CapRows>>,
    st: Option<Res<ShadowState>>,
    sh: Option<Res<PickupShadows>>,
    mut prims: ResMut<FxPrims>,
    mut cache: Local<(i64, Vec<u8>)>,
    mut boot_seen: Local<u32>,
) {
    if crate::boot::fresh(&mut boot_seen) {
        *cache = Default::default();
    }
    let (Some(st), Some(sh)) = (st, sh) else { return };
    let Some((eye, cm)) = p.pose.cam else { return };
    let key = cap.as_ref().map(|c| c.vb).unwrap_or(p.tick);
    if cache.1.is_empty() || cache.0 != key {
        let mut rows = rows;
        let Some(s) = scales(&p, cap.as_deref(), rows.as_deref_mut()) else { return };
        *cache = (key, s);
    }
    // phase 13: per eye in split screen (eyes.rs)
    let eyes: Vec<(([i32; 3], [i32; 9]), Option<u8>)> = match p.pose.cam2 {
        Some(c2) => vec![((eye, cm), Some(crate::psxfx::VIEW_MAIN)), (c2, Some(crate::psxfx::VIEW_MAIN2))],
        None => vec![((eye, cm), None)],
    };
    for ((eye, cm), view) in eyes {
        for ps in &sh.0 {
            let mut rec = ps.rec;
            rec.scale = cache.1.get(ps.pickup).copied().unwrap_or(0);
            if !rec.visible(eye) {
                continue;
            }
            if let Some((mut prim, z, c)) = shadow_prim(&rec, record_corners(&rec), eye, cm, &st.slot) {
                if std::env::var("MRM_SHADOW_DEBUG").is_ok() {
                    eprintln!("pickup shadow {} key {} {:?} z {} colour {}", ps.pickup, key, rec, z, c);
                }
                if let Some(v) = view {
                    prim.view = v;
                    prim.eye_only = true;
                }
                prims.list.push(prim);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// FUN_8007B800's records against an oracle capture of the shadow list
    /// (tools/oracle/fx_capture.py, Session 5: analysis/s5/fx/race3d);
    /// skipped when the capture or the disc is absent.
    #[test]
    fn pickup_shadow_records_match_the_oracle() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../analysis/s5/fx/race3d");
        let (Ok(meta), Ok(data)) = (std::fs::read_to_string(format!("{dir}/meta.json")), std::fs::read(format!("{dir}/frames.bin"))) else { return };
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe = disc.read("SLUS_012.37").unwrap();
        let w = mr_sim::world::World::dock(&mut disc).unwrap();
        let recs = super::build(&w, &exe);
        // regions from meta.json
        let i = meta.find("\"regions\"").unwrap();
        let body = &meta[i..meta[i..].find("]]").unwrap() + i];
        let nums: Vec<i64> = body.split(|c: char| !c.is_ascii_digit()).filter_map(|t| t.parse().ok()).collect();
        let regions: Vec<(u32, usize)> = nums.chunks_exact(2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        let size: usize = regions.iter().map(|r| r.1).sum();
        let row = &data[100 * size..101 * size];
        let read = |addr: u32, n: usize| -> Option<Vec<u8>> {
            let mut o = 0;
            for &(a, l) in &regions {
                if addr >= a && addr as usize + n <= a as usize + l {
                    return Some(row[o + (addr - a) as usize..o + (addr - a) as usize + n].to_vec());
                }
                o += l;
            }
            None
        };
        let (mut ok, mut n) = (0, 0);
        for ps in &recs {
            let Some(b) = read(0x800C_8B38 + 0x30 * (6 + ps.pickup as u32), 0x30) else { continue };
            n += 1;
            let i32at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
            let i16at = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
            let offs: [[i16; 3]; 4] = std::array::from_fn(|k| std::array::from_fn(|j| i16at(0xC + 6 * k + 2 * j)));
            let r = &ps.rec;
            if r.origin == [i32at(0), i32at(4), i32at(8)] && r.offs == offs && b[0x24] == r.uv0[0] && b[0x26] == r.uv0[1] && b[0x28] == r.slot && b[0x2A] == r.count {
                ok += 1;
            } else {
                eprintln!("pickup {}: port {:?}\n  game origin {:?} offs {:?}", ps.pickup, r, [i32at(0), i32at(4), i32at(8)], offs);
            }
        }
        eprintln!("pickup shadow records: {ok} / {n} exact");
        assert!(n > 0 && ok == n);
    }
}
