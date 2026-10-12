//! DAT decoder tests against the owner's disc (skipped with a message when
//! the disc is not available). Values marked RAM were read from the running
//! recomp (DOCK, 1-player race, DAT loaded at 0x80137E68) on 2026-10-12;
//! see docs/spec/formats/DAT.md "Verification".

use mr_assets::dat::{self, ChunkBody, Dat, Layer, SinTable};

fn disc() -> Option<mr_disc::Disc> {
    match mr_disc::Disc::open_default() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("SKIP: disc not available ({e})");
            None
        }
    }
}

fn load(disc: &mut mr_disc::Disc, path: &str) -> (Vec<u8>, Dat, SinTable) {
    let raw = disc.read(path).expect("read dat");
    let exe = disc.read("SLUS_012.37").expect("read exe");
    let sin = SinTable::from_exe(&exe).expect("sine table");
    let d = Dat::parse(&raw).expect("parse");
    (raw, d, sin)
}

#[test]
fn dock_layout_fully_accounted() {
    let Some(mut disc) = disc() else { return };
    let (raw, d, _) = load(&mut disc, "COURSE_A/DOCK.DAT");
    assert_eq!(raw.len(), 344_624);
    assert!(d.gaps().is_empty(), "unaccounted bytes: {:x?}", d.gaps());
    // Sections are contiguous and end at the file end.
    let mut p = 0;
    for s in &d.sections {
        assert_eq!(s.start, p, "section {} starts at 0x{:x}", s.name, s.start);
        p = s.end;
    }
    assert_eq!(p, raw.len());
    // Meshes are packed back to back (each padded to 4 after 0xFFFF).
    let ms: Vec<_> = d.meshes.values().collect();
    for w in ms.windows(2) {
        assert_eq!(w[0].end, w[1].offset);
    }

    let ids: Vec<i16> = d.chunks.iter().map(|c| c.id).collect();
    assert_eq!(ids, [0x40, 1, 3, 5, 7, 9, 0xA, 0xB, 0xC, 0x12, 0x13, 0x15]);
    let counts: Vec<i16> = d.chunks.iter().map(|c| c.count).collect();
    assert_eq!(counts, [174, 40, 4, 270, 9, 2, 290, 290, 12, 92, 44, 6]);
    assert!(d.block128.is_empty());

    // RAM: list heads DAT_800b0f00 = base+0x3a04, DAT_800b0fb4 = base+0x68cc,
    // DAT_800b0c28 = base+0x681c, DAT_800b0bc8 (object count) = 0x340.
    assert_eq!(d.layer(Layer::World).len(), 589);
    assert_eq!(d.layer(Layer::Quarter).len(), 243);
    assert_eq!(d.layer(Layer::World)[0].offset, 0x3A04);
    assert_eq!(d.layer(Layer::Quarter)[0].offset, 0x68CC);
    assert_eq!(d.special_offset, 0x681C);
    assert_eq!(d.objects.len(), 0x340);
    assert_eq!(d.special.len(), 43);
    let used: Vec<(usize, usize)> = d.special.iter().enumerate().filter_map(|(i, s)| s.map(|k| (i, k))).collect();
    assert_eq!(
        used,
        [(32, 274), (33, 436), (34, 563), (35, 565), (36, 566), (37, 567), (38, 569), (39, 570), (40, 572), (41, 574), (42, 269)]
    );
    // RAM: DAT_800b0c1c = base + file length; FUN_8002e3dc returns this size.
    assert_eq!(d.loader_output_size(), 832 * 0x28 + 43 * 0x1C);

    assert_eq!(d.descriptors.len(), 832);
    assert_eq!(d.meshes.len(), 609);
    let nv: usize = d.meshes.values().map(|m| m.vertices.len()).sum();
    let np: usize = d.meshes.values().map(|m| m.prim_count()).sum();
    assert_eq!((nv, np), (21_265, 9_798));
    assert!(d.meshes.values().all(|m| m.raw_count >= 0));
    assert!(d.orphans.is_empty() && d.parts.is_empty());
}

#[test]
fn dock_primitives() {
    let Some(mut disc) = disc() else { return };
    let (_, d, _) = load(&mut disc, "COURSE_A/DOCK.DAT");
    let first = &d.meshes[&0xDB7C];
    assert_eq!(first.vertices.len(), 37);
    assert_eq!(first.vertices[0].pos, [-755, -6741, -272]);
    assert_eq!(first.vertices[0].color, 0x3D8C);
    assert_eq!(first.vertices[0].rgb(), [0x78, 0x60, 0x60]);
    let g = &first.groups[0];
    assert_eq!((g.header, g.prims.len()), (0x0060, 20));
    assert_eq!((g.kind(), g.tex_slot(), g.semi_transparent(), g.gpu_code()), (0, 0, false, 0x3C));
    assert_eq!(g.prims[0].idx, [0, 1, 2, 3]);
    assert_eq!(g.prims[0].uv, [[0x40, 0xBF], [0x7F, 0xBF], [0x7F, 0x80], [0x40, 0x80]]);
    // Every group's kind is one the renderer and FUN_80025b8c handle.
    for m in d.meshes.values() {
        for g in &m.groups {
            assert!(matches!(g.kind(), 0 | 1 | 2 | 4 | 6 | 0x10 | 0x11 | 0x14), "kind {:x}", g.kind());
        }
    }
    // Texture attributes: slot table entry 9 of DOCK (RAM 0x800DFF90) is an
    // 8-bit page with fixed CLUT; entry 0 is 4-bit with per-tile CLUTs.
    let slot0 = dat::TexSlot { tpage: 0x0000, clut: 0x3070, fixed_clut: 0 };
    let p = &g.prims[0];
    assert_eq!(dat::prim_tpage_clut(slot0, g, p), (0x0060, 0x3070 + 1 + 4 * 2));
}

#[test]
fn dock_matrices_match_ram() {
    let Some(mut disc) = disc() else { return };
    let (raw, d, sin) = load(&mut disc, "COURSE_A/DOCK.DAT");
    // RAM: object records at 0x8018C54C + 0x28*i.
    let cases: [(usize, [i16; 9], u16); 6] = [
        (0, [4096, 0, 0, 0, 4096, 0, 0, 0, 4096], 30),
        (426, [3663, 288, 1153, 72, 4018, -502, -1827, 738, 2289], 8),
        (427, [1195, 0, 3917, 0, 4444, 0, -3917, 0, 1195], 15),
        (432, [4153, 0, 18, 0, 4096, 0, -18, 0, 4128], 18),
        (589, [-376, 1, -4079, 1, 4097, 1, 4079, 0, -376], 14),
        (831, [3101, 0, -2676, 0, 4096, 0, 2676, 0, 3101], 5),
    ];
    let base = 0x8013_7E68u32;
    let img = d.loader_image(&raw, base, &sin);
    for (i, m, slots) in cases {
        let o = &d.objects[i];
        let got = d.desc(o).matrix(&sin);
        let flat: Vec<i16> = got.iter().flatten().copied().collect();
        assert_eq!(flat, m, "object {i}");
        let ro = 0x1C * d.special.len() + 0x28 * i;
        assert_eq!(u16::from_le_bytes([img.out[ro + 0x20], img.out[ro + 0x21]]), slots, "object {i} slots");
        assert_eq!(d.mesh(o).unwrap().slot_count(), slots as usize);
    }
    // RAM: object 0's descriptor pointer 0x8013FA44 = base + 0x7BDC.
    let ro = 0x1C * d.special.len();
    assert_eq!(u32::from_le_bytes(img.out[ro + 0x14..ro + 0x18].try_into().unwrap()), 0x8013_FA44);
    // RAM: special slot 42 (object 269) pose record.
    let sp = &img.out[0x1C * 42..0x1C * 42 + 0x1A];
    assert_eq!(sp[0..12], [0x97, 0x24, 0x01, 0x00, 0, 0, 0, 0, 0x4E, 0x2F, 0x01, 0x00]);
    assert_eq!(sp[0x0C..0x14], [0x00, 0x10, 0x00, 0x10, 0x00, 0x10, 0x19, 0x00]);
}

#[test]
fn dock_world_transform() {
    let Some(mut disc) = disc() else { return };
    let (_, d, sin) = load(&mut disc, "COURSE_A/DOCK.DAT");
    // Object centre = bbox centre of its transformed mesh (layer units).
    let (mut worst, mut off) = (0, 0);
    for o in &d.objects {
        let t = d.transform(o, &sin);
        let mut mn = [i32::MAX; 3];
        let mut mx = [i32::MIN; 3];
        for v in &d.mesh(o).unwrap().vertices {
            let w = t.apply(v.pos);
            for k in 0..3 {
                mn[k] = mn[k].min(w[k] / t.scale);
                mx[k] = mx[k].max(w[k] / t.scale);
            }
        }
        if d.special.contains(&Some(d.objects.iter().position(|x| x.offset == o.offset).unwrap())) {
            continue;
        }
        for k in 0..3 {
            let c = (mn[k] + mx[k]) / 2;
            let dev = (c - o.centre[k]).abs();
            worst = worst.max(dev);
            if dev > 3 {
                off += 1;
            }
        }
    }
    // [INFER] The stored centre is the mesh's bbox centre up to a small
    // authoring error: within 3 units on > 95% of axes (worst 77 on DOCK).
    let axes = 3 * (d.objects.len() - 11);
    assert!(off * 20 < axes && worst <= 80, "worst {worst}, off {off} of {axes}");
    // GTE: the first object's mesh sits at origin (-8192, 8192, -8192), y
    // -6741 + 8192 = 1451 = its centre y.
    let o = &d.objects[0];
    assert_eq!(d.desc(o).origin, [-8192, 8192, -8192]);
    assert_eq!(d.transform(o, &sin).apply([-755, -6741, -272]), [-8947, 1451, -8464]);
    // Layer 1 is quarter scale.
    let q = &d.layer(Layer::Quarter)[0];
    assert_eq!(d.transform(q, &sin).scale, 4);
}

#[test]
fn dock_visibility_stream() {
    let Some(mut disc) = disc() else { return };
    let (_, d, _) = load(&mut disc, "COURSE_A/DOCK.DAT");
    assert_eq!(d.vis.len(), 1);
    let z = &d.vis[0];
    assert_eq!((z.offset, z.len), (0x52554, 7378));
    // RAM table DAT_800e3478: bbox and size of zone 0.
    assert_eq!(z.bbox_min, [-13099, 0, 5861]);
    assert_eq!(z.bbox_max, [-11786, 0, 7118]);
    assert_eq!(z.size2, 3202);
    let pts = match &d.chunks[z.chunk].body {
        ChunkBody::Points(p) => p.len(),
        _ => 0,
    };
    assert_eq!(z.separators().len(), pts - 2, "one separator per path node");
    // RAM: at node 44 the mask at 0x800C1AC8 had 157 bits set and the stream
    // pointer DAT_800b0f98 was at record offset 0x7E2 = 0x104 + separator 44.
    let m = z.mask_at(44);
    assert_eq!(m.iter().map(|b| b.count_ones()).sum::<u32>(), 157);
    assert_eq!(z.separators()[44] + 0x104, 0x7E2);
}

#[test]
fn every_dat_on_disc_parses() {
    let Some(mut disc) = disc() else { return };
    let mut paths: Vec<String> = disc.paths().filter(|p| p.ends_with(".DAT")).map(|s| s.to_string()).collect();
    paths.sort();
    assert!(!paths.is_empty());
    let mut n = 0;
    for p in &paths {
        let raw = disc.read(p).unwrap();
        match Dat::parse(&raw) {
            Ok(d) => {
                assert!(d.gaps().is_empty(), "{p}: unaccounted {:x?}", d.gaps());
                let raw_orphans: usize = d.orphans.iter().filter(|o| o.mesh.is_none()).map(|o| o.end - o.offset).sum();
                eprintln!(
                    "{p}: {} objects, {} meshes, {} parts models, {} orphan meshes, {raw_orphans} orphan raw bytes, {} vis zones",
                    d.objects.len(),
                    d.meshes.len(),
                    d.parts.len(),
                    d.orphans.iter().filter(|o| o.mesh.is_some()).count(),
                    d.vis.len()
                );
                n += 1;
            }
            Err(e) => panic!("{p}: {e}"),
        }
    }
    eprintln!("parsed {n} DAT files");
}

