//! Session 6, track R: the courses' animated scenery (scenery.rs) against
//! track O's course captures (analysis/oracle/courses/NAME/ram0.bin: the
//! course menu, course loaded): FUN_80084490's records 0x800D8630 as
//! `scenery::load` builds them from the rand() / random-byte streams found
//! in the oracle's RAM (the seed and byte pointer that reproduce the
//! turntables' draws), and a table of every DAT special slot of the course
//! (object, flags, who moves it).
//!
//! ```text
//! cargo run --release --offline -p mr-sim --bin mr-scenery -- [NAME...] [--slots]
//! ```

use mr_sim::course::CourseInfo;
use mr_sim::mem::Mem;
use mr_sim::oracle;
use mr_sim::scenery;
use mr_sim::session::{bios_rand, bios_rand_rewind};

fn ram_for(dir: &std::path::Path, name: &str) -> Option<Vec<u8>> {
    std::fs::read(dir.join(name).join("ram0.bin")).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let slots = args.iter().any(|a| a == "--slots");
    let mut names: Vec<String> = args.iter().filter(|a| !a.starts_with("--") && !a.contains('.')).map(|a| a.to_ascii_uppercase()).collect();
    let root = oracle::repo_root();
    let dir = root.join("analysis/oracle/courses");
    if names.is_empty() {
        let mut v: Vec<String> = std::fs::read_dir(&dir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
        v.sort();
        names = v;
    }
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let exe_bytes = disc.read("SLUS_012.37").unwrap();
    let exe = mr_sim::tables::ExeImage::parse(&exe_bytes).unwrap();
    for name in names {
        let Some(info) = CourseInfo::find(&exe, &name) else { continue };
        let dat_bytes = disc.read(&info.file("DAT")).expect("DAT");
        let dat = mr_assets::dat::Dat::parse(&dat_bytes).expect("parse DAT");
        let objs = mr_sim::objects::CourseObjects::from_dat(&dat, &dat_bytes);
        let breakables = mr_sim::breakables::table(info.id);
        println!("## {name} (course {:#x})", info.id);
        if args.iter().any(|a| a == "--objects") {
            // the course's object lists (counts; lifts from chunk 0x18)
            let w = mr_sim::world::World::course(&mut disc, info.id).expect("world");
            let (nr, _) = scenery::load_draws(info.id);
            let lo = mr_sim::lifts::objects(info.id);
            println!(
                "   jets {} turntables {} hook groups {} lifts {} | animated objects {} (lift objects {}, rand {nr}) special slots {}",
                objs.jets.len(),
                objs.turntables.len(),
                objs.hooks.len(),
                w.course.objects.lifts.len(),
                lo.len(),
                lo.iter().filter(|o| o.lift >= 0).count(),
                dat.special.iter().filter(|s| s.is_some()).count()
            );
            continue;
        }
        if let Some(i) = args.iter().position(|a| a == "--ground") {
            // dev: the TER surfaces under world x, y, z
            let v: Vec<i32> = args[i + 1].split(':').map(|x| x.parse().unwrap()).collect();
            let w = mr_sim::world::World::course(&mut disc, info.id).expect("world");
            let g = w.ter.ground(v[0], v[1], v[2]);
            println!("   ground at {v:?}: height {} nearest_y {} in grid {}", g.height, g.nearest_y, g.in_grid);
            continue;
        }
        if args.iter().any(|a| a == "--lightsel") {
            let mut c = std::collections::BTreeMap::new();
            for o in &dat.objects {
                *c.entry(dat.desc(o).b_hi & 7).or_insert(0) += 1;
            }
            println!("   light-matrix select (b_hi & 7) -> objects: {c:?}");
            continue;
        }
        if args.iter().any(|a| a == "--lit") {
            // meshes with 12-byte vertices (negative count): FUN_80012390's
            // GTE-lit path
            let lit: Vec<usize> = (0..dat.objects.len()).filter(|&i| dat.mesh(&dat.objects[i]).is_some_and(|m| m.raw_count < 0)).collect();
            let polys: usize = lit.iter().map(|&i| dat.mesh(&dat.objects[i]).map(|m| m.groups.iter().map(|g| g.prims.len()).sum::<usize>()).unwrap_or(0)).sum();
            println!("   lit meshes: {} objects, {} polygons (of {} objects)", lit.len(), polys, dat.objects.len());
            if let Some(&i) = lit.first() {
                let m = dat.mesh(&dat.objects[i]).unwrap();
                for v in m.vertices.iter().take(4) {
                    println!("     obj {i} vertex {:?} colour {:#06x} extra {:?}", v.pos, v.color, v.extra);
                }
            }
            continue;
        }
        if let Some(i) = args.iter().position(|a| a == "--gp0") {
            gp0(&mut disc, &exe_bytes, &info, &dat, &args[i + 1]);
            continue;
        }
        if args.iter().any(|a| a == "--colours") {
            // mesh vertex colours in the oracle's RAM (course menu) vs the disc
            if let Some(ram) = ram_for(&dir, &name) {
                let m = Mem::new(ram[..0x20_0000].to_vec(), ram[0x20_0000..0x20_0400].to_vec());
                let base = m.u32(0x800B_0EE8);
                let (mut same, mut diff, mut objs) = (0usize, 0usize, std::collections::BTreeSet::new());
                for (oi, o) in dat.objects.iter().enumerate() {
                    let Some(mesh) = dat.mesh(o) else { continue };
                    for (k, v) in mesh.vertices.iter().enumerate() {
                        let stride = if mesh.raw_count < 0 { 12 } else { 8 };
                        let a = base + (mesh.offset + 4 + stride * k + 6) as u32;
                        if m.u16(a) == v.color { same += 1 } else { diff += 1; objs.insert(oi); }
                    }
                }
                println!("   vertex colours in RAM: {same} as on the disc, {diff} differ ({} objects: {:?})", objs.len(), objs.iter().take(12).collect::<Vec<_>>());
            }
            continue;
        }
        let ram = std::fs::read(dir.join(&name).join("ram0.bin")).ok();
        let port = if let Some(ram) = ram.as_ref() {
            let m = Mem::new(ram[..0x20_0000].to_vec(), ram[0x20_0000..0x20_0400].to_vec());
            check(&m, &info, objs.turntables.len())
        } else {
            println!("   no ram0.bin");
            None
        };
        if let (Some(ram), Some(anim)) = (ram.as_ref(), port.as_ref()) {
            let m = Mem::new(ram[..0x20_0000].to_vec(), ram[0x20_0000..0x20_0400].to_vec());
            poses(&m, &info, &dat, anim, &mr_sim::tables::Tables::from_exe(&exe));
        }
        if slots {
            let anim = port.unwrap_or_default();
            println!("   slot object flags scaled layer angles origin | moved by");
            for (s, o) in dat.special.iter().enumerate() {
                let Some(oi) = *o else { continue };
                let ob = &dat.objects[oi];
                let d = dat.desc(ob);
                let mut by = Vec::new();
                for (i, a) in anim.iter().enumerate() {
                    if a.slot >= 0 && (a.slot as usize..a.slot as usize + a.slots as usize).contains(&s) {
                        by.push(format!("anim {i} (vel {:?} lift {} jet {})", a.base_vel, a.lift, a.jet));
                    }
                }
                for (slot, n, g) in &breakables {
                    if *slot >= 0 && (*slot as usize..*slot as usize + *n as usize).contains(&s) {
                        by.push(format!("breakable group {g}"));
                    }
                }
                println!(
                    "   {s:#04x} {oi} {:#04x} {} {} {:?} {:?} | {}",
                    ob.loaded_flags(),
                    d.scale.is_some(),
                    if oi < dat.layer0_count { 0 } else { 1 },
                    d.angles,
                    d.origin,
                    by.join(", ")
                );
            }
        }
    }
}

/// The oracle's records vs `scenery::load`. The course menu has already
/// run FUN_800859EC for N VBlanks (spin-up 0x40, angles moved on), so:
/// 1. structure: every field but the angles / spin-up / current spin, with
///    the random bytes from a pointer searched back from the RAM's;
/// 2. phases (courses with turntables): the rand() stream k draws back
///    from the RAM's seed whose turntable draws reproduce the spins and,
///    after N VBlanks, the phases; then the switch's angles after the same
///    N VBlanks must be the oracle's (objects not on a jet).
fn check(m: &Mem, info: &CourseInfo, n_turn: usize) -> Option<Vec<scenery::AnimObject>> {
    let oracle = scenery::read(m);
    let (nr, nb) = scenery::load_draws(info.id);
    let tt_phase: Vec<i16> = (0..n_turn as u32).map(|i| m.i16(0x800B_C648 + 4 * i)).collect();
    let tt_spin: Vec<i16> = (0..n_turn as u32).map(|i| m.i16(0x800B_C64A + 4 * i)).collect();
    let seed_now = m.u32(0x8000_9010);
    let ptr_now = m.u32(0x800B_0BCC);
    let shape = |a: &scenery::AnimObject| scenery::AnimObject { rot: [0; 3], fade: 0, vel: [0; 3], ..*a };
    let want: Vec<_> = oracle.iter().map(shape).collect();
    let load_at = |p: u32, rand: &mut dyn FnMut() -> i32| {
        let mut q = p;
        let mut byte = || {
            let b = m.u8(q);
            q = q.wrapping_add(1);
            b
        };
        scenery::load(info.id, &tt_spin, rand, &mut byte)
    };
    // (a rand() that only picks a sign, REST's spinner: both answers)
    let Some(p) = std::iter::once(ptr_now).chain(0x8001_F000..0x8002_0200u32).find(|&p| {
        [0, 0x7FFF].iter().any(|&r| {
            let mut z = || r;
            load_at(p, &mut z).iter().map(shape).collect::<Vec<_>>() == want
        })
    }) else {
        println!("   {} records in RAM: structure differs", oracle.len());
        let mut z = || 0;
        for (i, (a, b)) in oracle.iter().zip(load_at(ptr_now, &mut z).iter()).enumerate() {
            println!("     {i}: oracle {a:?}
        port   {b:?}");
        }
        return None;
    };
    let mut z = || 0;
    let v0 = load_at(p, &mut z);
    print!("   {} records: structure exact ({nr} rand, {nb} bytes{})", v0.len(), if nb > 0 { format!(", bytes from {p:#010X}, the pointer now {ptr_now:#010X}") } else { String::new() });
    if n_turn == 0 {
        println!("; angles: no turntable to time the menu by");
        return Some(v0);
    }
    for k in 1..400_000u32 {
        let mut s = bios_rand_rewind(seed_now, k);
        let mut ph0 = Vec::new();
        let mut ok = true;
        for i in 0..n_turn {
            ph0.push((bios_rand(&mut s) / 8) as i32);
            let r = bios_rand(&mut s);
            let mut sp: i16 = if r < 0x4000 { 5 } else { -5 };
            if (0x1A..=0x1B).contains(&info.id) {
                sp *= 3;
            }
            ok &= sp == tt_spin[i];
        }
        if !ok {
            continue;
        }
        // N (mod 4096) from turntable 0: spin is odd, invertible mod 4096
        let sp = tt_spin[0] as i32;
        let Some(n) = (0..4096).find(|n| (ph0[0] + sp * n) & 0xFFF == tt_phase[0] as i32 & 0xFFF) else { continue };
        if !(0..n_turn).all(|i| (ph0[i] + tt_spin[i] as i32 * n) & 0xFFF == tt_phase[i] as i32 & 0xFFF) {
            continue;
        }
        let mut s3 = s;
        let mut rand3 = || bios_rand(&mut s3);
        let v = load_at(p, &mut rand3);
        let mut g = mr_sim::globals::RaceGlobals::default();
        g.dt = 1;
        g.objects.anim = v.clone();
        for _ in 0..(n + 4096) {
            scenery::frame(&mut g);
        }
        let same = g.objects.anim.iter().zip(oracle.iter()).all(|(a, b)| a.jet != -1 || (a.rot == b.rot && a.fade == b.fade && a.vel == b.vel));
        if same {
            println!("; angles exact after N = {n} (mod 4096) VBlanks of the menu, seed {k} draws back");
            return Some(v);
        }
    }
    println!("; angles: no stream found");
    Some(v0)
}

/// The special slots' run-time poses in the oracle's RAM (object record
/// matrix, descriptor angles / origin) against what FUN_800859EC's calls
/// make of the RAM's own animated records, lifts, global spin angles and
/// frame counter: angles = pose + rot (FUN_8002D9E4), origin y = pose +
/// the lift's travel (FUN_8002E060), HAPPY's x scale (FUN_8002D72C).
fn poses(m: &Mem, info: &CourseInfo, dat: &mr_assets::dat::Dat, _port: &[scenery::AnimObject], t: &mr_sim::tables::Tables) {
    let sin = mr_assets::dat::SinTable((0..5120u32).map(|i| m.i16(0x800A_88C0 + 2 * i)).collect());
    let table = m.u32(0x800B_0C28);
    let pose_base = m.u32(0x800B_0C1C);
    let n = m.i32(table);
    let anim = scenery::read(m);
    let lifts: Vec<(i32, i32)> = (0..m.i8(0x800B_0E01).max(0) as u32).map(|i| (m.i32(0x800D_D708 + 20 * i), m.i32(0x800D_D708 + 20 * i + 4))).collect();
    let frame = m.i32(0x800B_0D90);
    let spin: Vec<u16> = (0..6).map(|i| m.u16(0x800A_889C + 2 * i)).collect();
    let (mut ok, mut bad) = (0, 0);
    for slot in 0..=n.max(0) as u32 {
        let obj = m.u32(table + 4 + 4 * slot);
        if obj == 0 || dat.special.get(slot as usize).copied().flatten().is_none() {
            continue;
        }
        let rec = m.u32(obj + 0x10);
        let desc = m.u32(rec + 0x14);
        let pr = pose_base + 0x1C * slot;
        let pose_ang = [m.u16(pr + 0x14), m.u16(pr + 0x16), m.u16(pr + 0x18)];
        let pose_scale = [m.i16(pr + 0xC), m.i16(pr + 0xE), m.i16(pr + 0x10)];
        let pose_org = [m.i32(pr), m.i32(pr + 4), m.i32(pr + 8)];
        let mut rot = [0u16; 3];
        let mut dy = 0;
        let mut scale_x = 0x1000;
        let mut by = Vec::new();
        for (i, a) in anim.iter().enumerate() {
            if a.slot >= 0 && (a.slot as u32..a.slot as u32 + a.slots as u32).contains(&slot) {
                rot = a.rot;
                by.push(format!("anim {i}"));
                if a.lift >= 0 {
                    if let Some(l) = lifts.get(a.lift as usize) {
                        dy = l.0 - l.1;
                        by.push(format!("lift {}", a.lift));
                    }
                }
            }
        }
        // REST: lift objects 3..8 turn their slots 2 / 3 about z by the
        // lift's counter
        if info.id == 0x17 {
            for (i, a) in anim.iter().enumerate().filter(|(i, a)| (3..=8).contains(i) && a.lift >= 0) {
                if a.slot >= 0 && (a.slot as u32 + 2..a.slot as u32 + 4).contains(&slot) {
                    let c = m.u8(0x800D_D708 + 20 * a.lift as u32 + 0x13);
                    rot = [0, 0, ((c & 0x7F) as u16) << 5];
                    by.push(format!("lift {} counter (anim {i})", a.lift));
                }
            }
        }
        // hooks: slot 0x30 + i on the hook (FUN_8002DF88) turned by its
        // +0x28 / +0x2A
        let nh = 3 * m.i8(0x800B_0E00).max(0) as u32;
        let mut hook_org = None;
        if (0x30..0x30 + nh).contains(&slot) {
            let h = 0x800D_15A0 + 0x2C * (slot - 0x30);
            rot = [m.u16(h + 0x28), 0, m.u16(h + 0x2A)];
            // the port's FUN_8003CCBC (hooks::angles) from the record's
            // hook and trailing point
            let hk = mr_sim::hooks::Hook { pos: [m.i32(h), m.i32(h + 4), m.i32(h + 8)], trail: [m.i32(h + 0x10), m.i32(h + 0x14), m.i32(h + 0x18)], ..Default::default() };
            let pa = mr_sim::hooks::angles(t, &hk);
            if pa != [rot[0], rot[2]] {
                println!("   hook {}: angles ram {:?} port {pa:?}", slot - 0x30, [rot[0], rot[2]]);
            }
            hook_org = Some([m.i32(h) >> 5, (m.i32(h + 4) >> 5) - 0x400, m.i32(h + 8) >> 5]);
            by.push("hook".into());
        }
        if (info.id == 0x0F && slot == 5) || (info.id == 0x0E && slot == 1) || (info.id == 0x19 && (0x1C..0x1F).contains(&slot)) {
            rot = [spin[0], spin[1], spin[2]];
            by.push("global spin".into());
        }
        if info.id == 0x0F && slot == 6 {
            rot = [0, spin[1], 0];
            by.push("global spin y".into());
        }
        if info.id == 9 && (0x50..0x54).contains(&slot) {
            // FUN_800859EC with the counter before this frame's step
            let f = (frame - 2).rem_euclid(0x30);
            scale_x = if f > 0xF { (0x30 - f) * 0x80 } else { f << 8 };
            by.push(format!("pulse (counter {frame})"));
        }
        if by.is_empty() {
            continue;
        }
        let ang = [0, 1, 2].map(|k| pose_ang[k].wrapping_add(rot[k]));
        let mut want = mr_assets::dat::rot_matrix(&sin, ang);
        let sc = [((pose_scale[0] as i32 * scale_x) / 4096) as i16, pose_scale[1], pose_scale[2]];
        mr_assets::dat::scale_matrix(&mut want, sc);
        let got: [[i16; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| m.i16(rec + 2 * (3 * r + c) as u32)));
        let org = [m.i32(desc), m.i32(desc + 4), m.i32(desc + 8)];
        let worg = hook_org.unwrap_or([pose_org[0], pose_org[1] + dy, pose_org[2]]);
        let ram_ang = [m.u16(desc + 0xC), m.u16(desc + 0xE), m.u16(desc + 0x10)];
        if got == want && org == worg {
            ok += 1;
        } else {
            bad += 1;
            println!("   slot {slot:#04x} ({}): matrix {} origin {} | angles ram {ram_ang:?} port {ang:?}; origin ram {org:?} port {worg:?}", by.join(", "), if got == want { "=" } else { "X" }, if org == worg { "=" } else { "X" });
            if got != want {
                println!("      ram {got:?}
      port {want:?}");
            }
        }
    }
    if ok + bad > 0 {
        println!("   poses: {ok} moved slots exact, {bad} differ (RAM at the course menu, frame counter {frame})");
    }
}

/// `--gp0 FILE` (tools/oracle/gp0_polys.py lines): every oracle polygon
/// matched to the DAT primitive with its texture page, CLUT and UVs; prints
/// the DAT vertex colours next to the colours the game sent.
fn gp0(disc: &mut mr_disc::Disc, exe: &[u8], info: &CourseInfo, dat: &mr_assets::dat::Dat, path: &str) {
    use mr_assets::raw;
    let rawp = raw::course_raw_path(exe, info.id as u32).unwrap();
    let env = raw::LoadEnv::from_exe(exe, info.id as u32).unwrap();
    let dec = raw::decode(&disc.read(&rawp).unwrap(), &env).unwrap();
    let mut slots = [None; 32];
    for (k, s) in dec.slots.iter().enumerate() {
        if let Some(s) = s {
            if let Some(tp) = s.tpage {
                slots[k] = Some(mr_assets::dat::TexSlot { tpage: tp, clut: s.clut, fixed_clut: s.mode.unwrap_or(0) });
            }
        }
    }
    // key: (tpage w/o abr, clut, uvs in GPU order) -> vertex colours
    let mut map: std::collections::HashMap<(u16, u16, Vec<[u8; 2]>), Vec<([u8; 3], usize)>> = Default::default();
    let mut keys: std::collections::HashMap<usize, (u16, u16)> = Default::default();
    for (oi, o) in dat.objects.iter().enumerate() {
        let Some(mesh) = dat.mesh(o) else { continue };
        for g in &mesh.groups {
            if !g.is_textured() {
                continue;
            }
            let Some(slot) = slots[g.tex_slot() as usize] else { continue };
            let n = if g.is_triangle() { 3 } else { 4 };
            let ord: &[usize] = if n == 4 { &[0, 1, 3, 2] } else { &[0, 1, 2] };
            for p in &g.prims {
                let (tp, cl) = mr_assets::dat::prim_tpage_clut(slot, g, p);
                for level in 0..3u8 {
                    if level > 0 && g.tex_slot() > 3 {
                        break;
                    }
                    let l = mr_assets::dat::lod_uvs(g.tex_slot() as u8, level, p.uv);
                    let uvl: Vec<[u8; 2]> = ord.iter().map(|&k| l[k]).collect();
                    let e = map.entry((0, 0, uvl)).or_default();
                    for &k in ord {
                        e.push((mesh.vertices[p.idx[k] as usize].rgb(), oi));
                    }
                }
                keys.entry(oi).or_insert((tp, cl));
            }
        }
    }
    if let Ok(q) = std::env::var("MR_RGBQ") {
        // dev: DAT prims on CLUT c with a vertex colour within 8 of (r, g, b)
        let w: Vec<i32> = q.split(',').map(|x| x.parse().unwrap()).collect();
        let mut n = 0;
        for ((_, _, uv), d) in map.iter() {
            let k = keys.get(&d[0].1).copied().unwrap_or_default();
            if w[3] >= 0 && k.1 as i32 != w[3] {
                continue;
            }
            if d.iter().any(|x| (0..3).all(|c| (x.0[c] as i32 - w[c]).abs() <= 8)) {
                n += 1;
                if n < 8 {
                    println!("   rgbq obj {} layer {} uv {:?} rgb {:?}", d[0].1, if d[0].1 < dat.layer0_count { 0 } else { 1 }, uv, d.iter().take(uv.len()).map(|x| x.0).collect::<Vec<_>>());
                }
            }
        }
        println!("   rgbq: {n} prims");
    }
    if let Ok(q) = std::env::var("MR_UVQ") {
        // dev: DAT prims containing uv (u, v) on a CLUT
        let w: Vec<i32> = q.split(',').map(|x| x.parse().unwrap()).collect();
        for ((_, _, uv), d) in map.iter() {
            let k = keys.get(&d[0].1).copied().unwrap_or_default();
            let (u0, u1) = (uv.iter().map(|p| p[0] as i32).min().unwrap(), uv.iter().map(|p| p[0] as i32).max().unwrap());
            let (v0, v1) = (uv.iter().map(|p| p[1] as i32).min().unwrap(), uv.iter().map(|p| p[1] as i32).max().unwrap());
            if k.1 as i32 == w[2] && u0 <= w[0] && w[0] <= u1 && v0 <= w[1] && w[1] <= v1 {
                println!("   uvq obj {} uv {:?} rgb {:?}", d[0].1, uv, d.iter().take(uv.len()).map(|x| x.0).collect::<Vec<_>>());
            }
        }
    }
    let text = std::fs::read_to_string(path).expect("gp0 lines");
    let (mut hit, mut miss) = (0, 0);
    for line in text.lines() {
        let v: Vec<i32> = line.split_whitespace().map(|x| x.parse().unwrap()).collect();
        let (tp, cl, n) = (v[0] as u16, v[1] as u16, v[2] as usize);
        let uv: Vec<[u8; 2]> = (0..n).map(|k| [v[3 + 2 * k] as u8, v[4 + 2 * k] as u8]).collect();
        let col: Vec<[i32; 3]> = (0..n).map(|k| [v[3 + 2 * n + 3 * k], v[4 + 2 * n + 3 * k], v[5 + 2 * n + 3 * k]]).collect();
        let xy: Vec<[i32; 2]> = (0..n).map(|k| [v[3 + 5 * n + 2 * k], v[4 + 5 * n + 2 * k]]).collect();
        match map.get(&(0, 0, uv)) {
            Some(d) => {
                hit += 1;
                let dc: Vec<[u8; 3]> = d.iter().take(n).map(|x| x.0).collect();
                let k = keys.get(&d[0].1).copied().unwrap_or_default();
                let cands: std::collections::BTreeSet<usize> = d.iter().map(|x| x.1).collect();
                println!("   pair {:?} {:?} {}", d.chunks(n).map(|c| c.iter().map(|x| x.0).collect::<Vec<_>>()).collect::<Vec<_>>(), col, cands.len());
                println!("   obj {} tp/clut dat {:#x}/{:#x} game {:#x}/{:#x} y {:?}: dat {:?} game {:?}", d[0].1, k.0, k.1, tp, cl, xy.iter().map(|p| p[1]).collect::<Vec<_>>(), dc, col);
            }
            None => miss += 1,
        }
    }
    println!("   gp0 polygons matched {hit}, unmatched {miss} (LOD UVs, karts, HUD, effects)");
}
