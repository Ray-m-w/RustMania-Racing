//! TER decoder and terrain-query tests. They read the owner's disc (skipped
//! with a message when it is absent). Expected query results below were read
//! from the running recomp (kart records at 0x800c30a8 + k*0x504 and the
//! globals FUN_8003ddb0 / FUN_8003da18 write) with `mr-ter oracle`; see
//! docs/spec/formats/TER.md "Verification". No game bytes are embedded.

use mr_assets::ter::{Ter, FACE_DISABLED, KIND_WATER};

fn disc() -> Option<mr_disc::Disc> {
    match mr_disc::Disc::open_default() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("SKIP: disc not available ({e:?})");
            None
        }
    }
}

fn dock() -> Option<(Vec<u8>, Ter)> {
    let mut d = disc()?;
    let b = d.read("COURSE_A/DOCK.TER").expect("read DOCK.TER");
    let t = Ter::parse(&b).expect("parse DOCK.TER");
    Some((b, t))
}

#[test]
fn dock_header_and_counts() {
    let Some((b, t)) = dock() else { return };
    assert_eq!(b.len(), 70_180);
    assert_eq!((t.grid_w, t.grid_h), (32, 32));
    assert_eq!(t.min, [-20953, -3303, -29421]);
    assert_eq!(t.max, [46668, 4822, 44446]);
    assert_eq!((t.cell_x, t.span_y, t.cell_z), (2113, 8125, 2308));
    // cell size = extent / grid, truncated
    assert_eq!(t.cell_x, (t.max[0] - t.min[0]) / t.grid_w);
    assert_eq!(t.cell_z, (t.max[2] - t.min[2]) / t.grid_h);
    assert_eq!(t.span_y, t.max[1] - t.min[1]);
    assert_eq!(t.no_water_y, 21206);
    assert_eq!(t.vertices.len(), 1368);
    assert_eq!(t.faces.len(), 943);
    assert_eq!(t.wall_points.len(), 927);
    assert_eq!(t.walls.len(), 672);
    assert_eq!(t.groups.len(), 0);
    assert_eq!(t.cell_list.len(), 5097);
    assert_eq!((t.pad_list, t.pad_links), (2, 0));
    assert_eq!(t.path_points.len(), 357);
    assert_eq!(t.path_segments.len(), 27);
    assert_eq!(t.links.len(), 48);
    assert_eq!(t.to_bytes(), b, "lossless round trip");
}

#[test]
fn every_ter_on_disc_parses_and_round_trips() {
    let Some(mut d) = disc() else { return };
    let mut paths: Vec<String> = d.paths().filter(|p| p.ends_with(".TER")).map(String::from).collect();
    paths.sort();
    assert_eq!(paths.len(), 36);
    for p in paths {
        let b = d.read(&p).unwrap();
        let t = Ter::parse(&b).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert_eq!(t.to_bytes(), b, "{p} round trip");
        invariants(&p, &t);
    }
}

fn invariants(p: &str, t: &Ter) {
    // Normals are unit (4096) within rounding; floors face up (-y).
    for (i, f) in t.faces.iter().enumerate() {
        let n = f.normal;
        let len2 = n.iter().map(|&c| (c as i64) * (c as i64)).sum::<i64>();
        // (a few faces on other tracks store a zero normal)
        assert!(len2 == 0 || (len2 - 4096 * 4096).abs() < 4096 * 64, "{p} face {i} normal {n:?}");
        assert_eq!(f.flags & 0x1f, 0, "{p} face {i} low flag bits");
    }
    // Every enabled face and wall is reachable through some cell list.
    let cells = (t.grid_w * t.grid_h) as usize;
    let mut face_seen = vec![false; t.faces.len()];
    let mut wall_seen = vec![false; t.walls.len()];
    for c in 0..cells {
        t.cell_faces(c).for_each(|f| face_seen[f] = true);
        t.cell_wall_list(c).for_each(|w| wall_seen[w] = true);
    }
    let lost_faces = face_seen.iter().filter(|s| !**s).count();
    assert!(lost_faces * 50 <= t.faces.len().max(50), "{p}: {lost_faces} faces in no cell");
    // Path graph indices stay in range.
    for (i, s) in t.path_segments.iter().enumerate() {
        assert!(s.first >= 0 && (s.first + s.count) as usize <= t.path_points.len(), "{p} seg {i}");
        let e = s.link_start as usize + (s.n_next + s.n_prev) as usize;
        assert!(e <= t.links.len(), "{p} seg {i} links");
        for &l in &t.links[s.link_start as usize..e] {
            assert!((l as usize) < t.path_segments.len(), "{p} seg {i} link {l}");
        }
        for k in 0..3 {
            assert!(s.bb_min[k] <= s.bb_max[k], "{p} seg {i} bbox");
        }
    }
    for pt in &t.path_points {
        assert!(pt.heading < 4096, "{p} heading");
    }
}

#[test]
fn dock_normals_follow_winding() {
    let Some((_, t)) = dock() else { return };
    // normal ~= -(v1 - v0) x (v2 - v0), unit 4096 (cosine > 0.99) on 882 of
    // 943 faces; 39 more are exactly reversed (normal pointing down, +y).
    let (mut same, mut reversed) = (0, 0);
    for f in &t.faces {
        let [a, b, c] = [f.v[0], f.v[1], f.v[2]].map(|k| t.vertices[k as usize]);
        let u = [(b.x - a.x) as f64, (b.y - a.y) as f64, (b.z - a.z) as f64];
        let v = [(c.x - a.x) as f64, (c.y - a.y) as f64, (c.z - a.z) as f64];
        let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        let cos = -(0..3).map(|i| cr[i] * f.normal[i] as f64).sum::<f64>() / (l * 4096.0);
        if cos > 0.99 {
            same += 1;
        } else if cos < -0.99 {
            reversed += 1;
        }
    }
    assert_eq!((same, reversed), (882, 39));
}

#[test]
fn dock_path_graph() {
    let Some((_, t)) = dock() else { return };
    // Each segment's links are contiguous and in order: segment k+1 starts
    // where segment k's next+prev ids end.
    let mut at = 0;
    for s in &t.path_segments {
        assert_eq!(s.link_start, at);
        at += s.n_next + s.n_prev;
    }
    assert_eq!(at as usize, t.links.len());
    // Segment 0's successor is 13 and predecessor 21 (read by FUN_80090424).
    assert_eq!(t.links[0], 13);
    assert_eq!(t.links[1], 21);
    // Every path point lies inside its segment's bounding box.
    for s in &t.path_segments {
        for k in 0..s.count as usize {
            let p = t.path_points[s.first as usize + k];
            for (v, a) in [p.x, p.y, p.z].iter().enumerate() {
                assert!(s.bb_min[v] <= *a && *a <= s.bb_max[v]);
            }
        }
    }
}

/// (x, z, query y, face, height): kart position at the time of the
/// snapshot, the y the kart passes to FUN_8003ddb0 ((2y + k[0x438] +
/// k[0x43a]) / 2), and the face pointer / ground height FUN_8003e91c stored
/// in the kart record (+0x2e0 / +0x31c). All read from RAM, DOCK race.
const KART_GROUND: &[(i32, i32, i32, usize, i32)] = &[
    (17_434, 1_236_382, 40_288, 435, 46_400),
    (110_418, 1_305_996, 37_072, 454, 46_400),
    (18_321, 1_170_699, 37_919, 743, 47_567),
    (2_555, 1_169_238, 38_253, 743, 47_581),
    (-6_917, 830_053, 4_702, 661, 13_822),
    (-234_406, 393_683, 10_046, 683, 15_502),
    (557_517, -74_555, 94_568, 575, 105_695),
    (916_802, 439_098, 82_492, 439, 96_754),
    (559_543, -73_910, 94_568, 575, 105_779),
    (538_381, -80_800, 94_568, 566, 104_855),
    (528_938, -83_808, 94_568, 566, 104_517),
    (530_404, -83_344, 94_568, 566, 104_557),
    (535_517, -80_918, 94_568, 566, 105_181),
    (-6_236, 844_014, 8_873, 661, 14_985),
    (1_008_451, 517_783, 65_776, 425, 71_232),
    (427_483, -111_578, 94_568, 567, 105_502),
    (-295_973, 56_039, 51_319, 253, 64_376),
    (440_164, -109_214, 94_568, 549, 103_763),
    (10_839, 1_133_543, 40_924, 737, 50_252),
    (-11_075, 817_908, 6_951, 660, 13_448),
    (513_923, -87_407, 94_568, 566, 104_615),
    (245_265, -310_453, 94_951, 551, 105_382),
    (1_040_017, -50_499, 39_369, 419, 46_615),
    (871_978, 574_427, 56_861, 414, 64_669),
];

#[test]
fn ground_matches_ram() {
    let Some((_, t)) = dock() else { return };
    for &(x, z, qy, face, y) in KART_GROUND {
        let g = t.ground(x, qy, z);
        assert!(g.in_grid);
        let n = g.nearest.expect("a floor");
        assert_eq!((n.face, n.y, g.nearest_y), (face, y, y), "at ({x}, {z})");
    }
}

#[test]
fn ground_water_and_chosen_height_match_ram() {
    let Some((_, t)) = dock() else { return };
    // Last FUN_8003ddb0 call of a snapshot (kart 5): globals 0x800b0d64/6c
    // cell (13, 8), 0x800b0b0c face 543, 0x800b0b24 99669, 0x800b0ae0 543,
    // 0x800b0b04 99669, 0x800b0f8c face 314, 0x800b0a54 96000, 0x800b0c00
    // 96000; kart +0x2e4 (water face) = 314.
    let g = t.ground(241_854, 77_682, -331_395);
    assert_eq!((g.cell_x, g.cell_z), (13, 8));
    assert_eq!(g.nearest.map(|h| (h.face, h.y)), Some((543, 99_669)));
    assert_eq!(g.below.map(|h| (h.face, h.y)), Some((543, 99_669)));
    assert_eq!(g.water.map(|h| (h.face, h.y)), Some((314, 96_000)));
    assert_eq!(g.water_y, 96_000);
    assert_eq!(g.height, 96_000);
    assert!(g.found);
    assert_eq!(t.faces[314].kind, KIND_WATER);
    // Same snapshot, karts 0..2: water faces 806, 797, 797 (+0x2e4).
    for (x, z, qy, w) in [(245_265, -310_453, 94_951, 806), (585_722, -117_995, 82_830, 797), (536_033, -126_526, 82_307, 797)] {
        assert_eq!(t.ground(x, qy, z).water.map(|h| h.face), Some(w));
    }
    // Default water level when no water face: no_water_y << 5 = 678592
    // (0x800b0a54 seen in RAM with no water under the kart).
    let g = t.ground(17_434, 40_288, 1_236_382);
    assert_eq!((g.water, g.water_y), (None, 678_592));
}

#[test]
fn ground_outside_grid_defaults() {
    let Some((_, t)) = dock() else { return };
    let g = t.ground((t.min[0] - 10) << 5, 0, 0);
    assert!(!g.in_grid);
    assert_eq!(g.cell_x, -1);
    assert_eq!(g.height, t.max[1] << 5);
    assert_eq!(g.nearest, None);
}

#[test]
fn face_height_reproduces_vertices_and_flat_faces() {
    let Some((_, t)) = dock() else { return };
    for (i, f) in t.faces.iter().enumerate() {
        let tri = [f.v[0] as usize, f.v[1] as usize, f.v[2] as usize];
        // At a vertex the height is that vertex's y << 5 (exact).
        let v = t.vertices[tri[0]];
        let h = t.face_height(tri[0], tri[1], tri[2], v.x, v.z);
        let flat = tri.iter().all(|&k| t.vertices[k].y == v.y);
        if flat {
            assert_eq!(h, v.y << 5, "face {i}");
        }
    }
}

/// Wall edges the game reported (0x800b0dac -> wall, 0x800b0ed0 = point
/// from the edge's second u16, 0x800b0ed8 = point from its first u16), with
/// the hitting kart's previous position (+0x2e8/+0x2f0): (wall, p1, p0, x0, z0).
const WALL_HITS: &[(usize, u16, u16, i32, i32)] = &[
    (579, 832, 833, -381_747, 27_398),
    (580, 833, 834, -377_673, 37_296),
    (268, 521, 522, 1_072_156, 905_742),
    (269, 522, 523, 1_068_754, 907_926),
];

#[test]
fn wall_decode_and_side_match_ram() {
    let Some((_, t)) = dock() else { return };
    for &(wi, p1, p0, x0, z0) in WALL_HITS {
        let w = t.walls[wi];
        assert_eq!((w.p1, w.p0, w.disabled), (p1, p0, false), "wall {wi}");
        // The kart came from the wall's negative side.
        let (a, b) = (t.wall_points[p1 as usize], t.wall_points[p0 as usize]);
        let (x, z) = (x0 >> 5, z0 >> 5);
        let e = (x - a.x) as i64 * (b.z - a.z) as i64 + (z - a.z) as i64 * (a.x - b.x) as i64;
        assert!(e < 0, "wall {wi}: start side {e}");
        // A straight move through the wall's midpoint, 2 TER units past it,
        // is reported by wall_cross unless an earlier listed wall is crossed.
        let (mx, mz) = ((a.x + b.x) / 2, (a.z + b.z) / 2);
        let (dx, dz) = (mx - x, mz - z);
        let len = ((dx as f64).hypot(dz as f64)).max(1.0);
        let (ex, ez) = (mx + (dx as f64 * 2.0 / len).round() as i32, mz + (dz as f64 * 2.0 / len).round() as i32);
        let hits: Vec<usize> = t.wall_candidates(x0, z0, ex << 5, ez << 5).collect();
        assert!(hits.contains(&wi), "wall {wi} not crossed: {hits:?}");
        assert_eq!(t.wall_cross(x0, z0, ex << 5, ez << 5), hits.first().copied());
        // Moving back out does not cross it (walls are one-sided).
        assert!(!t.wall_candidates(ex << 5, ez << 5, x0, z0).any(|h| h == wi));
    }
}

#[test]
fn groups_toggle_and_move() {
    let Some(mut d) = disc() else { return };
    let b = d.read("COURSE_A/LONDON.TER").unwrap();
    let mut t = Ter::parse(&b).unwrap();
    assert_eq!(t.groups.len(), 13);
    let g = t.groups[0];
    t.set_group_enabled(0, false);
    for k in 0..g.face_count as usize {
        assert!(t.faces[g.face_start as usize + k].flags & FACE_DISABLED != 0);
    }
    for k in 0..g.wall_count as usize {
        assert!(t.walls[g.wall_start as usize + k].disabled);
    }
    t.set_group_enabled(0, true);
    let y0 = t.vertices[g.vert_start as usize].y;
    t.move_group_y(0, -7);
    assert_eq!(t.vertices[g.vert_start as usize].y, y0 - 7);
    t.move_group_y(0, 7);
    assert_eq!(t.to_bytes(), b, "enable + move back restores the file");
}
