use mr_assets::dat::Dat;
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let dat = Dat::parse(&d.read("COURSE_A/DOCK.DAT").unwrap()).unwrap();
    let mut h = [0usize; 32];
    for m in dat.meshes.values() { for g in &m.groups { if g.is_textured() && g.tex_slot() == 0 { for p in &g.prims { for k in 0..4 { let c = m.vertices[p.idx[k] as usize].rgb(); h[(c[0] / 8) as usize] += 1; } } } } }
    println!("red channel histogram (bins of 8): {:?}", h);
}
