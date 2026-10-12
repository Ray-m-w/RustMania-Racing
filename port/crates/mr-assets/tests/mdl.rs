//! MDL decoder tests against the owner's disc (skipped when it is absent).
//! Values marked RAM/GPU/GTE were read from the running recomp on DOCK
//! (see docs/spec/formats/MDL.md, Verification).

use mr_assets::mdl::{self, Mdl, PartData, SlotAttr};

fn disc() -> Option<mr_disc::Disc> {
    match mr_disc::Disc::open_default() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("SKIP: no disc ({e:?})");
            None
        }
    }
}

fn load(d: &mut mr_disc::Disc, p: &str) -> (Vec<u8>, Mdl) {
    let b = d.read(p).unwrap();
    let m = Mdl::parse(&b).unwrap_or_else(|e| panic!("{p}: {e}"));
    (b, m)
}

#[test]
fn every_mdl_on_disc_parses_fully() {
    let Some(mut d) = disc() else { return };
    let mut paths: Vec<String> = d.paths().filter(|p| p.ends_with(".MDL")).map(|s| s.to_string()).collect();
    paths.sort();
    assert_eq!(paths.len(), 65);
    let mut polys = 0;
    for p in &paths {
        let (b, m) = load(&mut d, p);
        assert_eq!(m.table_hw as usize * 2 + 4 + m.parts.len() * mdl::RECORD_LEN, b.len(), "{p}");
        let mut at = 4;
        for (i, part) in m.parts.iter().enumerate() {
            assert_eq!(part.offset, at, "{p} part {i}");
            at += part.record.size_hw as usize * 2;
            let r = &part.record;
            assert_eq!(r.zero_14, [0; 6]);
            assert_eq!(r.zero_1c, [0; 12]);
            assert_eq!((r.tail_mode, r.flags_46, r.unk_44), (0, 0, 0), "{p} part {i}");
            match &part.data {
                PartData::Mesh { polys: ps, tail } => {
                    assert_eq!(r.kind, mdl::KIND_MESH, "{p} part {i}");
                    assert!(tail.is_empty());
                    assert_eq!(r.data_offset_hw as usize * 2 + 4, part.offset, "{p} part {i}");
                    assert_eq!(r.unk_28, [255, 0]);
                    let slot = ps[0].tex_slot();
                    for q in ps {
                        // one texture slot per part (the emitter reads it once)
                        assert_eq!(q.tex_slot(), slot, "{p} part {i} @0x{:X}", q.offset);
                        if q.is_quad() {
                            assert_eq!(q.flags[0], 0, "{p} quad +0x27");
                        }
                        assert_eq!(q.code & 0xFD, if q.is_quad() { 0x3C } else { 0x34 });
                    }
                    polys += ps.len();
                }
                PartData::Ring(rg) => {
                    assert_eq!(r.kind, mdl::KIND_SEAM_RING);
                    assert_eq!(rg.verts.first().map(|v| v.0), rg.verts.last().map(|v| v.0), "closed ring");
                    assert!(rg.owner_part() < m.parts.len());
                }
                PartData::Other(_) => panic!("{p}: unexpected non-ring block"),
            }
        }
        // every neck part names two rings that exist
        for (_, part) in m.meshes() {
            if part.record.point_w[1] != 0 {
                let (a, b) = part.seam_rings();
                assert!(m.rings().any(|r| r.ring_number() == a), "{p} ring {a}");
                assert!(m.rings().any(|r| r.ring_number() == b), "{p} ring {b}");
            }
        }
    }
    assert_eq!(polys, 8079);
}

#[test]
fn kermit_matches_ram_and_gpu() {
    let Some(mut d) = disc() else { return };
    let (_, m) = load(&mut d, "CHARS_A/KERMIT.MDL");
    assert_eq!(m.table_hw, 0x96E);
    assert_eq!(m.parts.len(), 5);
    let counts: Vec<usize> = m.meshes().map(|(_, p)| p.polys().len()).collect();
    assert_eq!(counts, [50, 55, 7]); // RAM slots 20..22 +0x24: 50, 55, 7
    let piv: Vec<[i32; 3]> = m.meshes().map(|(_, p)| p.record.pivot).collect();
    assert_eq!(piv, [[0, -145, -28], [0, 0, 0], [0, 0, 0]]); // RAM slots 20..22
    assert_eq!(m.slot_count(), 3);
    let head = &m.parts[0];
    let q = &head.polys()[0];
    assert_eq!(q.code, 0x34);
    assert_eq!(q.colors[0], [0x7D, 0x86, 0x71]); // GPU GT3 word0 0x3471867D
    let uv: Vec<[u8; 2]> = q.verts.iter().map(|v| v.uv).collect();
    assert_eq!(uv, [[16, 70], [21, 64], [22, 81]]);
    let pos: Vec<[i16; 3]> = q.verts.iter().map(|v| v.pos).collect();
    assert_eq!(pos, [[23, -94, -56], [5, -89, -35], [28, -88, -21]]); // GTE RTPT V0..V2
    assert_eq!(head.tex_flag(), Some(0x10));
    // RAM 0x800dff00 slot 16 = tpage 0x0004, clut 0x3FF0; GPU packet tpage 0x0004 clut 0x3FF4
    assert_eq!(mdl::tpage_clut(0x10, SlotAttr { tpage: 0x0004, clut: 0x3FF0 }, q), (0x0004, 0x3FF4));
    // neck: rings 101 (head, ring 1) and 200 (body, ring 0); RAM slot 22 +0x2C = 0x0C0D
    let neck = &m.parts[2];
    assert_eq!(neck.record.point_w[1], 1);
    assert_eq!(neck.seam_rings(), (1, 0));
    let ids: Vec<u16> = m.rings().map(|r| r.id).collect();
    assert_eq!(ids, [101, 200]);
    // rest pose: ring vertex + owner pivot is a neck vertex (seam closes)
    let neck_pts: Vec<[i32; 3]> =
        neck.polys().iter().flat_map(|q| q.verts.iter().map(|v| v.pos.map(|c| c as i32))).collect();
    for rg in m.rings() {
        assert_eq!(rg.segments, 7);
        let pv = m.parts[rg.owner_part()].record.pivot;
        for (p, _) in &rg.verts {
            let w = [p[0] as i32 + pv[0], p[1] as i32 + pv[1], p[2] as i32 + pv[2]];
            assert!(neck_pts.contains(&w), "ring {} vertex {:?} -> {:?}", rg.id, p, w);
        }
    }
    // GTE: seam renderer RTPS inputs (frame 38262) start (0,-114,-45) / (0,22,-28)
    assert_eq!(m.rings().map(|r| r.verts[0].0).collect::<Vec<_>>(), [[0, 22, -28], [0, -114, -45]]);
}

#[test]
fn racer_matches_ram_and_gpu() {
    let Some(mut d) = disc() else { return };
    let (_, m) = load(&mut d, "VEHIC_A/RACER.MDL");
    let counts: Vec<usize> = m.meshes().map(|(_, p)| p.polys().len()).collect();
    assert_eq!(counts, [13, 13, 13, 13, 78]); // RAM slots 108..112
    let piv: Vec<[i32; 3]> = m.meshes().map(|(_, p)| p.record.pivot).collect();
    assert_eq!(piv, [[154, -62, -199], [-152, -62, -199], [-151, -71, 106], [155, -71, 106], [0, 0, 0]]);
    assert_eq!(m.rings().count(), 0);
    let flags: Vec<u8> = m.meshes().map(|(_, p)| p.tex_flag().unwrap()).collect();
    assert_eq!(flags, [0x10, 0x10, 0x10, 0x10, 0x12]);
    // wheels: slot 16 (0x0004/0x3FF0), u 160..191 v 0..59 -> cell 2 -> GPU clut 0x3FF2
    let w = &m.parts[0].polys()[0];
    assert_eq!(mdl::tpage_clut(0x10, SlotAttr { tpage: 4, clut: 0x3FF0 }, w), (0x0004, 0x3FF2));
    // body: slot 18 (0x0006/0x3F70); GPU shows clut 0x3F70 and 0x3F7E
    let body = &m.parts[4];
    let cl: std::collections::BTreeSet<u16> = body
        .polys()
        .iter()
        .map(|q| mdl::tpage_clut(0x12, SlotAttr { tpage: 6, clut: 0x3F70 }, q).1)
        .collect();
    assert!(cl.contains(&0x3F70) && cl.contains(&0x3F7E), "{cl:X?}");
}

#[test]
fn dock_raw_slots_feed_the_models() {
    use mr_assets::raw::{self, LoadEnv};
    let Some(mut d) = disc() else { return };
    let exe = d.read("SLUS_012.37").unwrap();
    let env = LoadEnv::from_exe(&exe, raw::DOCK_COURSE).unwrap();
    let dec = raw::decode(&d.read("COURSE_A/DOCK.RAW").unwrap(), &env).unwrap();
    // RAM 0x800dff00: slot 16 = 0004/3FF0, slot 18 = 0006/3F70
    let s16 = dec.slots[16].unwrap();
    let s18 = dec.slots[18].unwrap();
    assert_eq!((s16.tpage, s16.clut), (Some(0x0004), 0x3FF0));
    assert_eq!((s18.tpage, s18.clut), (Some(0x0006), 0x3F70));
}
