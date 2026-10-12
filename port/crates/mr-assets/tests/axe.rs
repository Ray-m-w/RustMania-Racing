//! AXE (sky dome) decoder tests. They read the owner's disc (skipped with a
//! message when absent). The RAM facts below were read from the running
//! recomp with `mr-axe oracle` (DOCK race; see docs/spec/formats/AXE.md).

use mr_assets::axe::{Axe, SEGMENT_LEN};

fn disc() -> Option<mr_disc::Disc> {
    match mr_disc::Disc::open_default() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("SKIP: disc not available ({e:?})");
            None
        }
    }
}

#[test]
fn dock_axe_layout() {
    let Some(mut d) = disc() else { return };
    let b = d.read("COURSE_A/DOCK.AXE").unwrap();
    assert_eq!(b.len(), 20_924);
    let a = Axe::parse(&b).unwrap();
    assert_eq!(a.segments.len(), 16);
    assert_eq!(a.tail_offset, 0x5140);
    assert_eq!(a.n_face_ids, 120);
    assert_eq!(a.segments[0].vert_offset as usize, 8 + 16 * SEGMENT_LEN);
    let nv: usize = a.segments.iter().map(|s| s.vertices.len()).sum();
    let nf: usize = a.segments.iter().map(|s| s.faces.len()).sum();
    assert_eq!((nv, nf), (1292, 323));
    // Each segment has 4 vertices per face (quads never share vertices).
    for s in &a.segments {
        assert_eq!(s.vertices.len(), 4 * s.faces.len());
        assert_eq!((s.unk40, s.unk42), (0, 123));
    }
    // Every face id is drawn exactly once after dedupe.
    assert_eq!(a.drawn_quads().len(), a.n_face_ids as usize);
    // The one unreferenced block is the spare table every segment's third
    // pointer names: a verbatim copy of the count word + segment table.
    assert_eq!(a.unreferenced.len(), 1);
    let (off, ref bytes) = a.unreferenced[0];
    assert!(a.segments.iter().all(|s| s.spare_offset as usize == off));
    assert_eq!(bytes[..], b[4..4 + 4 + 16 * SEGMENT_LEN]);
    assert!(a.tail_rest.iter().all(|&x| x == 0));
    assert_eq!(a.to_bytes(), b);
}

#[test]
fn dock_axe_is_a_dome_around_the_origin() {
    let Some(mut d) = disc() else { return };
    let a = Axe::parse(&d.read("COURSE_A/DOCK.AXE").unwrap()).unwrap();
    for s in &a.segments {
        for v in &s.vertices {
            let r2 = (v.x as i64).pow(2) + (v.y as i64).pow(2) + (v.z as i64).pow(2);
            assert!(r2 > 200 * 200 && r2 < 600 * 600, "vertex {v:?}");
            assert_eq!(v.code, 0);
        }
    }
}

#[test]
fn every_axe_on_disc_round_trips() {
    let Some(mut d) = disc() else { return };
    let mut paths: Vec<String> = d.paths().filter(|p| p.ends_with(".AXE")).map(String::from).collect();
    paths.sort();
    assert_eq!(paths.len(), 12);
    for p in paths {
        let b = d.read(&p).unwrap();
        let a = Axe::parse(&b).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert_eq!(a.to_bytes(), b, "{p}");
        assert!(a.drawn_quads().len() <= a.n_face_ids as usize, "{p}");
    }
}

#[test]
fn ram_image_transform() {
    // Proven against RAM: on DOCK, `to_ram_image(0x801a5970)` equals the
    // 20,924 bytes at *0x800b0c4c byte for byte (mr-axe oracle); 15,173
    // bytes are unchanged from the file. Here: the transform's shape.
    let Some(mut d) = disc() else { return };
    let b = d.read("COURSE_A/DOCK.AXE").unwrap();
    let a = Axe::parse(&b).unwrap();
    let base = 0x801a_5970u32;
    let r = a.to_ram_image(base);
    let same = b.iter().zip(&r).filter(|(x, y)| x == y).count();
    assert_eq!(same, 15_173);
    let w = |o: usize| u32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    assert_eq!(w(0), 0x5140, "word 0 is not relocated");
    assert_eq!(w(8), base + a.segments[0].vert_offset);
    let f = a.segments[0].face_offset as usize;
    assert_eq!(w(f + 8), 0x1F80_0020 + a.segments[0].faces[0].face_id);
    let v = a.segments[0].vert_offset as usize;
    assert_eq!([r[v + 8], r[v + 9], r[v + 10], r[v + 11]], [b[v + 10], b[v + 9], b[v + 8], 0]);
}
