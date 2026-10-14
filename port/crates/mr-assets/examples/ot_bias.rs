//! Layer-0 ordering-table bias per object (course byte 0x800ACB3F + 12*course
//! + 16 * b_lo; FUN_80012390 sorts each quad at T[OTZ + bias]).
//! usage: cargo run --release -p mr-assets --example ot_bias -- COURSE_B/GHOST ID [OBJ...]
use mr_assets::dat::{self, Dat};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let exe = disc.read("SLUS_012.37").unwrap();
    let d = Dat::parse(&disc.read(&format!("{}.DAT", a[1])).unwrap()).unwrap();
    let cb = dat::course_depth_bias(&exe, a[2].parse().unwrap()).unwrap();
    let mut hist = std::collections::BTreeMap::new();
    for o in &d.objects[..d.layer0_count] {
        let b = dat::lod_bias(cb, d.desc(o).b_lo);
        *hist.entry((o.loaded_flags(), b)).or_insert(0) += 1;
    }
    let cf = dat::DrawParams::course_flags(&exe, a[2].parse().unwrap()).unwrap();
    println!("course bias {cb}; course flags {cf:#x} -> {:?}; (flags, bias) -> objects: {hist:?}", dat::DrawParams::race_1p(cf));
    for s in &a[3..] {
        let i: usize = s.parse().unwrap();
        let o = &d.objects[i];
        println!("obj {i} flags {:#x} b_lo {} bias {}", o.loaded_flags(), d.desc(o).b_lo, dat::lod_bias(cb, d.desc(o).b_lo));
    }
}
