//! mr-particles: check mr_sim::particles against an oracle capture
//! (tools/oracle/fx_capture.py: the particle list 0x800E0C20, its side table
//! 0x800D73D0 and the spark streaks 0x800B4540 every VBlank).
//!
//! usage: mr-particles CAPDIR
//!
//! Teacher-forced, one race frame at a time: the frames are the steps of the
//! frame counter DAT_800B0D90 (dt = its step). For each frame the list read
//! at the end of the previous frame is moved with `Particles::update` /
//! `Sparks::update` and compared with the list read at the end of this frame,
//! slot by slot. Slots the game (re)spawned during the frame are told apart
//! (the ring index moved past them and their spawn fields changed) and
//! counted separately: their spawns are the item code's (track A).

use mr_sim::mem::Mem;
use mr_sim::particles::{Flats, Particles, Sparks};

struct Cap {
    regions: Vec<(u32, usize)>,
    vbs: Vec<u32>,
    size: usize,
    data: Vec<u8>,
}

fn numbers(s: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() || (ch == '-' && cur.is_empty()) {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse() {
                out.push(v);
            }
            cur.clear();
        }
    }
    out
}

fn section<'a>(meta: &'a str, key: &str) -> &'a str {
    let i = meta.find(&format!("\"{key}\"")).unwrap_or_else(|| panic!("{key}"));
    let rest = &meta[i..];
    let a = rest.find('[').unwrap();
    let mut depth = 0;
    for (k, ch) in rest[a..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[a..a + k + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated {key}")
}

impl Cap {
    fn open(dir: &str) -> Cap {
        let meta = std::fs::read_to_string(format!("{dir}/meta.json")).expect("meta.json");
        let r = numbers(section(&meta, "regions"));
        let regions: Vec<(u32, usize)> = r.chunks_exact(2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        let vbs = numbers(section(&meta, "vb")).into_iter().map(|v| v as u32).collect();
        let size = regions.iter().map(|r| r.1).sum();
        Cap { regions, vbs, size, data: std::fs::read(format!("{dir}/frames.bin")).expect("frames.bin") }
    }
    /// The captured regions of row `i` laid into a sparse Mem.
    fn mem(&self, i: usize) -> Mem {
        let mut m = Mem::new(vec![0; 0x20_0000], vec![0; 0x400]);
        let row = &self.data[i * self.size..(i + 1) * self.size];
        let mut o = 0;
        for &(a, n) in &self.regions {
            m.write(a, &row[o..o + n]);
            o += n;
        }
        m
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: mr-particles CAPDIR");
    let cap = Cap::open(&dir);
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let exe = disc.read("SLUS_012.37").expect("exe");
    let types = Particles::from_exe(&exe).types;
    let sin: Vec<i16> = {
        let o = (mr_sim::particles::addr::SIN - 0x8001_0000) as usize + 0x800;
        (0..4096).map(|k| i16::from_le_bytes([exe[o + 2 * k], exe[o + 2 * k + 1]])).collect()
    };
    let d90 = |m: &Mem| m.i32(0x800B_0D90);
    let (mut frames, mut p_ok, mut p_bad, mut p_new, mut p_live, mut p_spawn_ok) = (0, 0, 0, 0, 0, 0);
    let (mut s_ok, mut s_bad, mut s_new) = (0, 0, 0);
    // flat quads 0x800C84B8 (captures with that region, Session 5)
    let has_flats = cap.regions.iter().any(|r| r.0 == mr_sim::particles::flat_addr::LIST);
    let w = mr_sim::world::World::dock(&mut disc).expect("world");
    let (mut f_ok, mut f_bad, mut f_live, mut f_skid, mut f_splash) = (0, 0, 0, 0, 0);
    let mut prev: Option<Mem> = None;
    let mut shown = 0;
    for i in 0..cap.vbs.len() {
        let m = cap.mem(i);
        let Some(pm) = prev.as_ref() else {
            prev = Some(m);
            continue;
        };
        // compare frame-end to frame-end: rows where the frame counter steps
        // (the particle updates fall between two such rows)
        if d90(&m) == d90(pm) {
            continue;
        }
        let dt = d90(&m) - d90(pm);
        frames += 1;
        // particles
        let mut a = Particles::read(pm, types.clone());
        let b = Particles::read(&m, types.clone());
        a.update(dt, &sin);
        // slots handed out this frame: ring positions after the old index up to the new one
        let mut fresh = vec![false; 64];
        let mut k = a.next;
        while k != b.next {
            k = (k + 1) & 0x3F;
            fresh[k as usize] = true;
        }
        for s in 0..64 {
            if fresh[s] {
                p_new += 1;
                // the spawn's table-derived fields (FUN_800870E8)
                let q = b.list[s];
                let t = types.get(q.kind as usize).copied().unwrap_or([0; 4]);
                let code = if t[2] & 0x60 == 0x60 { 0x2C } else { 0x2E };
                if (q.sprite == t[0] || q.life <= 0) && q.behaviour == t[3] && q.frames == t[1] && q.flags & !0x200 == t[2] as u16 && q.code == code && q.link == -1 {
                    p_spawn_ok += 1;
                }
                continue;
            }
            if b.list[s].life > 0 {
                p_live += 1;
            }
            // flags bit 0x200 is the drawer's (FUN_80010E90 sets it when the
            // quad lands on screen, clears it otherwise): not simulation
            let mut pa = a.list[s];
            let mut pb = b.list[s];
            pa.flags &= !0x200;
            pb.flags &= !0x200;
            if pa == pb {
                p_ok += 1;
            } else {
                p_bad += 1;
                if shown < 8 {
                    shown += 1;
                    eprintln!("vb {} slot {s} dt {dt}:\n  port {:?}\n  game {:?}", cap.vbs[i], a.list[s], b.list[s]);
                }
            }
        }
        // sparks
        let mut sa = Sparks::read(pm);
        let sb = Sparks::read(&m);
        sa.update(dt);
        let mut fresh = [false; 32];
        let mut k = sa.next;
        while k != sb.next {
            fresh[k as usize] = true;
            k = (k + 1) % 32;
        }
        for s in 0..32 {
            if fresh[s] {
                s_new += 1;
            } else if sa.list[s] == sb.list[s] {
                s_ok += 1;
            } else {
                s_bad += 1;
                if shown < 16 {
                    shown += 1;
                    eprintln!("vb {} spark {s} dt {dt}:\n  port {:?}\n  game {:?}", cap.vbs[i], sa.list[s], sb.list[s]);
                }
            }
        }
        if has_flats {
            // teacher-forced FUN_8007916C tail for the human kart: splash
            // (when the game spawned one: its random decision is not
            // replayed), FUN_80033440 skids from this frame's record with
            // last frame's intensities (+0x3F), then FUN_80033B98
            let mut fa = Flats::read(pm);
            let fb = Flats::read(&m);
            let rp = pm.slice(0x800C_30A8, 0x504).to_vec();
            let rec = m.slice(0x800C_30A8, 0x504).to_vec();
            for wh in 0..8 {
                fa.skid[0][wh] = rp[0x3F + 0x2C * wh.min(5)];
            }
            let first = fa.next as usize;
            let splashed = fb.list[first].abr == 0x20 && fb.list[first].uv[0] == 0x60 && fa.list[first] != fb.list[first];
            if splashed {
                let i16r = |o: usize| i16::from_le_bytes([rec[o], rec[o + 1]]) as i32;
                let i32r = |o: usize| i32::from_le_bytes(rec[o..o + 4].try_into().unwrap());
                let size = { let v = i16r(0x176) * i16r(0x442); (if v < 0 { v + 0xFFF } else { v }) >> 12 };
                fa.splash([i32r(0), i32r(0x398), i32r(8)], size);
                f_splash += 1;
            }
            let before = fa.next;
            fa.skids(0, &rec, dt, &w.tables);
            f_skid += (fa.next as i32 - before as i32).rem_euclid(32);
            fa.update(dt);
            for s in 0..32 {
                if fb.list[s].life != 0 {
                    f_live += 1;
                }
                if fa.list[s] == fb.list[s] {
                    f_ok += 1;
                } else {
                    f_bad += 1;
                    if shown < 24 {
                        shown += 1;
                        eprintln!("vb {} flat {s} dt {dt}:
  port {:?}
  game {:?}", cap.vbs[i], fa.list[s], fb.list[s]);
                    }
                }
            }
            if fa.next != fb.next {
                f_bad += 1;
            }
        }
        prev = Some(m);
    }
    if has_flats {
        println!("flats:     {f_ok} slot-frames exact, {f_bad} differ, {f_live} live slot-frames, {f_skid} skid quads spawned by the port, {f_splash} splashes taken from the game");
    }
    println!("frames {frames}");
    println!("particles: {p_ok} slot-frames exact, {p_bad} differ, {p_new} spawned this frame ({p_spawn_ok} with the type table's fields), {p_live} live slot-frames");
    println!("sparks:    {s_ok} slot-frames exact, {s_bad} differ, {s_new} spawned this frame (skipped)");
}
