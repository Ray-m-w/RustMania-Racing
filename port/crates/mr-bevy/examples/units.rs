use mr_assets::{dat::{Dat, SinTable}, ter::Ter};
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let exe = d.read("SLUS_012.37").unwrap();
    let ter = Ter::parse(&d.read("COURSE_A/DOCK.TER").unwrap()).unwrap();
    let dat = Dat::parse(&d.read("COURSE_A/DOCK.DAT").unwrap()).unwrap();
    let sin = SinTable::from_exe(&exe).unwrap();
    let mut lo = [i32::MAX; 3]; let mut hi = [i32::MIN; 3];
    for v in &ter.vertices { for (k, c) in [v.x, v.y, v.z].iter().enumerate() { lo[k] = lo[k].min(*c); hi[k] = hi[k].max(*c); } }
    println!("TER verts bbox {:?}..{:?} (hdr min {:?} max {:?})", lo, hi, ter.min, ter.max);
    for layer in [0usize, 1] {
        let mut lo = [i32::MAX; 3]; let mut hi = [i32::MIN; 3]; let mut n = 0;
        for (i, o) in dat.objects.iter().enumerate() {
            if (i >= dat.layer0_count) as usize != layer { continue; }
            let Some(m) = dat.mesh(o) else { continue };
            let t = dat.transform(o, &sin);
            for v in &m.vertices { let w = t.apply(v.pos); n += 1; for k in 0..3 { lo[k] = lo[k].min(w[k]); hi[k] = hi[k].max(w[k]); } }
        }
        println!("DAT layer {layer}: {n} verts bbox {:?}..{:?}", lo, hi);
    }
}
