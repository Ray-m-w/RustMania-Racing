use mr_assets::dat::{Dat, DrawParams};
fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let mut d = mr_disc::Disc::open_default().unwrap();
    let dat = Dat::parse(&d.read("COURSE_A/DOCK.DAT").unwrap()).unwrap();
    let eye = [(a[0] / 32) as i32, (a[1] / 32) as i32, (a[2] / 32) as i32];
    let c = |i: usize| a[3 + i] as i16;
    let cam = [[c(0), c(1), c(2)], [c(3), c(4), c(5)], [c(6), c(7), c(8)]];
    let zone = dat.vis_zone(eye, 0);
    let node = dat.vis_node(zone, eye);
    let mask = node.map(|n| dat.vis[zone].mask_at(n)).unwrap_or(vec![0xff; 256]);
    let all = dat.drawn(eye, cam, &mask, DrawParams::DOCK_1P);
    let nomask = dat.drawn(eye, cam, &vec![0xff; 256], DrawParams::DOCK_1P);
    println!("eye {:?} zone {} node {:?} drawn {} (no mask {})", eye, zone, node, all.len(), nomask.len());
    for (i, o) in dat.objects.iter().enumerate() {
        let dx = (o.centre[0] - eye[0]) as i64; let dz = (o.centre[2] - eye[2]) as i64;
        let s = if i < dat.layer0_count { 1 } else { 4 };
        let dist = (((dx * s) * (dx * s) + (dz * s) * (dz * s)) as f64).sqrt();
        if i < dat.layer0_count && dist < 3000.0 {
            let m = dat.mesh(o).unwrap();
            let tex = m.groups.iter().filter(|g| g.is_textured()).map(|g| g.prims.len()).sum::<usize>();
            let un = m.groups.iter().filter(|g| !g.is_textured()).map(|g| g.prims.len()).sum::<usize>();
            println!("obj {i} dist {dist:.0} r {} flags {:#x} tex {tex} untex {un} drawn {} nomask {}", o.radius, o.loaded_flags(),
                all.iter().any(|x| x.object == i), nomask.iter().any(|x| x.object == i));
        }
    }
}
