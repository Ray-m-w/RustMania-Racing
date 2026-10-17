//! Phase 11: MEET THE MUPPETS (docs/spec/frontend.md "MEET THE MUPPETS").
//! Against the oracle's gallery (tools/oracle/xa_probe.py, title state,
//! START, CROSS on MEET THE MUPPETS; RAM at VB 300: analysis/s8/meet/p1,
//! local; skipped without it).

use mr_sim::gallery::{self, Gallery, GalleryChar};
use mr_sim::globals::RaceGlobals;
use mr_sim::world::World;

fn world() -> World {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    let mut d = mr_disc::Disc::open_default().unwrap();
    World::course(&mut d, 0x1E).unwrap()
}

fn dump(name: &str) -> Option<Vec<u8>> {
    std::fs::read(mr_sim::oracle::repo_root().join("analysis/s8/meet/p1").join(name)).ok()
}

fn i32at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn i16at(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([b[o], b[o + 1]])
}

#[test]
fn tables_and_lists() {
    let w = world();
    // the grid (oracle RAM = exe): Kermit, Piggy, Fozzie on row 0
    assert_eq!(gallery::character_at(&w, 0, 0), Some(0));
    assert_eq!(gallery::character_at(&w, 0, 1), Some(2));
    assert_eq!(gallery::character_at(&w, 4, 4), Some(24));
    // the heights (oracle RAM 0x800AC038: Kermit 145, 92; the first from
    // the model)
    let mut d = mr_disc::Disc::open_default().unwrap();
    let k = mr_assets::mdl::Mdl::parse(&d.read("CHARS_A/KERMIT.MDL").unwrap()).unwrap();
    assert_eq!(k.parts[0].record.pivot[1].abs(), 145);
    assert_eq!(gallery::height(&w, 0, k.parts[0].record.pivot[1]), (145 + 92) / 2);
    // Kermit with nothing won: his RACER only
    assert_eq!(gallery::vehicle_list(&w, 0, &|_| false), vec![0]);
}

#[test]
fn records_match_the_oracle() {
    let (Some(tab), Some(scratch)) = (dump("ram_00300_800B3B78.bin"), dump("ram_00300_800C49BC.bin")) else {
        eprintln!("SKIP: no gallery capture");
        return;
    };
    let w = world();
    let mut g = RaceGlobals { dt: 2, rng: 0x8001_F94C, ..Default::default() };
    let gal = Gallery::new(&mut g, &w, (0, 0));
    for (i, c) in gal.chars.iter().enumerate() {
        let o = i * 0x64;
        assert_eq!(c.pos, [i32at(&tab, o), i32at(&tab, o + 4), i32at(&tab, o + 8)], "character record {i}");
    }
    // The scratch record holds the last character drawn (row 1, column 1:
    // grid 6 = character 7); rebuilt from its own outputs, the driver
    // matrices are the game's.
    let ch = GalleryChar {
        pos: [i32at(&scratch, 0), i32at(&scratch, 4), i32at(&scratch, 8)],
        out: [0, 1, 2, 3, 4].map(|k| i16at(&scratch, 0x454 + 2 * k)),
        ..Default::default()
    };
    assert_eq!(ch.pos, gal.chars[6].pos);
    let r = gallery::record(&w, &ch, 7, true);
    for (o, n) in [(0x1CC, 18), (0x29C, 18), (0x2AE, 18), (0x330, 4), (0x338, 4), (0x340, 4), (0x414, 12), (0x454, 10), (0x4A4, 1)] {
        assert_eq!(&r[o..o + n], &scratch[o..o + n], "scratch record +{o:#x}");
    }
    assert_eq!(scratch[0x4A5], 19, "model slot 0x800C3028[7]");
}
