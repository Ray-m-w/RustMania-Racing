use mr_assets::{dat::Dat, raw};
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let exe = d.read("SLUS_012.37").unwrap();
    let env = raw::LoadEnv::from_exe(&exe, 3).unwrap();
    let dec = raw::decode(&d.read("COURSE_A/DOCK.RAW").unwrap(), &env).unwrap();
    for (k, s) in dec.slots.iter().enumerate() { if let Some(s) = s { println!("slot {k}: {:?}", s); } }
    let dat = Dat::parse(&d.read("COURSE_A/DOCK.DAT").unwrap()).unwrap();
    let mut cnt = std::collections::BTreeMap::new();
    for m in dat.meshes.values() { for g in &m.groups { if g.is_textured() { *cnt.entry(g.tex_slot()).or_insert(0) += g.prims.len(); } else { *cnt.entry(255).or_insert(0) += g.prims.len(); } } }
    println!("prims by slot (255 = untextured): {:?}", cnt);
}
