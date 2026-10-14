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


#[test]
fn layer0_texture_lod() {
    use dat::{lod_level_for_sz, lod_uvs, LodThresholds};
    let uv = [[0x80, 0xFE], [0xBF, 0xFF], [0x9E, 0xC1], [0x41, 0x83]];
    assert_eq!(lod_uvs(0, 0, uv), uv);
    assert_eq!(lod_uvs(4, 2, uv), uv, "slots >= 4 have no reduced copies");
    // GPU: a far sand quad stored (0x80,0xFE).. was sent as 0x3F20 (u 0x20, v 0x3F).
    assert_eq!(lod_uvs(1, 2, uv)[0], [0x20, 0x3F]);
    assert_eq!(lod_uvs(3, 1, uv), [[0x40, 0x7F], [0x5F, 0x7F], [0x4F, 0x60], [0x20, 0x41]]);
    let t = LodThresholds::ONE_PLAYER;
    // RAM table: T[1901] = 0xC00, T[1902] > 0xC00; T[4900] <= 0xF80 < T[4901].
    assert_eq!((t.level(950), t.level(951), t.level(2450), t.level(2451)), (0, 1, 1, 2));
    // GPU (DOCK, sand bias 96+16*4): avg SZ ~3000 -> LOD 0, ~9000 -> 1, ~9700 -> 2.
    let bias = dat::lod_bias(96, 4);
    assert_eq!(lod_level_for_sz([3000; 4], bias, t), 0);
    assert_eq!(lod_level_for_sz([9000; 4], bias, t), 1);
    assert_eq!(lod_level_for_sz([9700; 4], bias, t), 2);
    assert_eq!(LodThresholds::NONE.level(1 << 20), 0);
    let Some(mut disc) = disc() else { return };
    let exe = disc.read("SLUS_012.37").unwrap();
    assert_eq!(dat::course_depth_bias(&exe, 3).unwrap(), 96);
}

#[test]
fn layer1_overlaps_layer0() {
    // [INFER check] Layer 1 (mostly untextured) covers the same places as
    // layer-0 scenery: its stand-ins for distant views.
    let Some(mut disc) = disc() else { return };
    let (_, d, sin) = load(&mut disc, "COURSE_A/DOCK.DAT");
    let bbox = |o: &dat::Object| {
        let t = d.transform(o, &sin);
        let mut mn = [i32::MAX; 3];
        let mut mx = [i32::MIN; 3];
        for v in &d.mesh(o).unwrap().vertices {
            let w = t.apply(v.pos);
            for k in 0..3 {
                mn[k] = mn[k].min(w[k]);
                mx[k] = mx[k].max(w[k]);
            }
        }
        (mn, mx)
    };
    let l0: Vec<_> = d.layer(Layer::World).iter().map(bbox).collect();
    let mut hit = 0;
    let l1 = d.layer(Layer::Quarter);
    for o in l1 {
        let (a, b) = bbox(o);
        if l0.iter().any(|(c, e)| a[0] <= e[0] && c[0] <= b[0] && a[2] <= e[2] && c[2] <= b[2]) {
            hit += 1;
        }
    }
    eprintln!("layer-1 objects whose x/z bbox overlaps a layer-0 object: {hit} of {}", l1.len());
    assert!(hit * 10 >= l1.len() * 9);
}

#[test]
fn dock_vis_node_and_selection() {
    let Some(mut disc) = disc() else { return };
    let (_, d, _) = load(&mut disc, "COURSE_A/DOCK.DAT");
    // RAM: eye/32 = (27164, 1206, 19247) (from the frame's GTE) -> DAT_800b0d40 = 121.
    let eye = [27164, 1206, 19247];
    assert_eq!(d.vis_zone(eye, 0), 0);
    assert_eq!(d.vis_node(0, eye), Some(121));
    // Layer 1 is never selected right next to the camera (z < cd0/4 - r),
    // except objects whose radius reaches past cd0/4.
    let cam = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
    let mask = vec![0xFF; 256];
    for o in d.layer(Layer::Quarter).iter().filter(|o| o.flags & 0x80 == 0 && (o.radius as i32) < (2386 >> 2) - 30) {
        let e = [o.centre[0] * 4, o.centre[1] * 4, o.centre[2] * 4 - 100];
        let sel = d.drawn(e, cam, &mask, dat::DrawParams::DOCK_1P);
        assert!(!sel.iter().any(|x| x.object == d.objects.iter().position(|y| y.offset == o.offset).unwrap()));
    }
}

#[test]
fn dock_banner_matches_gte() {
    let Some(mut disc) = disc() else { return };
    let (_, d, sin) = load(&mut disc, "COURSE_A/DOCK.DAT");
    let b = d.banners().expect("DOCK has a banner");
    assert_eq!(b.banners.len(), 1);
    let bn = b.banners[0];
    assert_eq!((bn.mid, bn.angle, bn.spacing), ([-13519, -33, -1112], 3848, 347));
    // GTE: the 33 vertices FUN_800313bc transformed (V = vertex - mid, i16)
    // in one oracle frame, reproduced exactly at cloth phase (119, 1614).
    let gte = "-1667 347 -502;-1651 0 -543;-1621 -347 -618;-1287 -347 -519;-1282 0 -531;-1279 347 -538;-953 -347 -419;-916 0 -510;-895 347 -562;-627 -347 -300;-579 0 -421;-552 347 -487;-310 -347 -157;-278 0 -238;-260 347 -283;-2 347 5;-1 0 3;0 -347 0;256 347 293;275 0 244;309 -347 159;551 347 488;579 0 421;627 -347 300;899 347 553;919 0 505;954 -347 417;1284 347 525;1285 0 522;1287 -347 517;1622 -347 616;1653 0 536;1671 347 492";
    let want: std::collections::HashSet<[i32; 3]> = gte
        .split(';')
        .map(|t| {
            let v: Vec<i32> = t.split_whitespace().map(|x| x.parse().unwrap()).collect();
            [v[0], v[1], v[2]]
        })
        .collect();
    let cloth = b.cloth(&sin, (119, 1614));
    let qs = b.quads(0, &cloth, &sin);
    let got: std::collections::HashSet<[i32; 3]> =
        qs.iter().flat_map(|q| q.pos).map(|p| [p[0] - bn.mid[0], p[1] - bn.mid[1], p[2] - bn.mid[2]]).collect();
    assert_eq!(got, want);
    assert_eq!(qs.len(), 20);
    assert_eq!(qs[1].uv, [[0, 0x80], [31, 0x80], [31, 0x9F], [0, 0x9F]]);
    assert_eq!(qs[0].uv[0], [0x40, 0x10]);
    assert!(b.visible(0, [-13519, 0, -1112]) && !b.visible(0, [0, 0, 0]));
}

/// PARK's tree canopies are kind 2 parts models; the oracle's GP0 frame
/// (grid, VB 300: PolyGT4 colour (82, 80, 72), UVs (191,127) (191,64)
/// (128,127) (128,64), CLUT = slot CLUT + 6) carries a record's values.
#[test]
fn park_parts_records_match_the_oracle_canopies() {
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("SKIP: disc not available");
        return;
    };
    let dat = mr_assets::dat::Dat::parse(&disc.read("COURSE_A/PARK.DAT").unwrap()).unwrap();
    let mut found = 0;
    for o in &dat.objects[..dat.layer0_count] {
        let Some(m) = dat.parts_model(o) else { continue };
        for (_, q) in m.quads() {
            let gpu = [q.uv[0], q.uv[1], q.uv[3], q.uv[2]];
            let mut set = gpu.to_vec();
            set.sort();
            if q.rgb[0] == [82, 80, 72] && set == vec![[128, 64], [128, 127], [191, 64], [191, 127]] {
                // CLUT from uv0: (u >> 6) + 4 (v >> 6) = 6 for every corner
                // the oracle's packet starts with
                assert!(gpu.iter().any(|uv| (uv[0] >> 6) as u16 + 4 * (uv[1] >> 6) as u16 == 6));
                found += 1;
            }
        }
    }
    assert!(found > 0);
}
