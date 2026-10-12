use mr_assets::ter::Ter;
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let t = Ter::parse(&d.read("COURSE_A/DOCK.TER").unwrap()).unwrap();
    // For each racing-line point: distance to the nearest wall segment on each side (2D, TER units).
    let segs: Vec<_> = t.walls.iter().filter(|w| !w.disabled).map(|w| {
        let a = t.wall_points[w.p0 as usize]; let b = t.wall_points[w.p1 as usize];
        ((a.x as f64, a.z as f64), (b.x as f64, b.z as f64))
    }).collect();
    let mut widths = Vec::new();
    for p in &t.path_points {
        let h = p.heading as f64 / 4096.0 * std::f64::consts::TAU;
        let (fx, fz) = (h.sin(), h.cos());
        let (rx, rz) = (fz, -fx);
        let mut best = [f64::MAX; 2];
        for &((ax, az), (bx, bz)) in &segs {
            // ray from p along +right and -right
            for (k, s) in [1.0f64, -1.0].iter().enumerate() {
                let (dx, dz) = (rx * s, rz * s);
                let (ex, ez) = (bx - ax, bz - az);
                let den = dx * ez - dz * ex;
                if den.abs() < 1e-9 { continue; }
                let (wx, wz) = (ax - p.x as f64, az - p.z as f64);
                let tt = (wx * ez - wz * ex) / den;
                let u = (wx * dz - wz * dx) / den;
                if tt > 0.0 && (0.0..=1.0).contains(&u) && tt < best[k] { best[k] = tt; }
            }
        }
        if best[0] < 1e6 && best[1] < 1e6 { widths.push(best[0] + best[1]); }
        let _ = p.w;
    }
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = widths.len();
    println!("racing-line points {} with walls both sides {}; width TER units: min {:.0} p25 {:.0} median {:.0} p75 {:.0}; median x32 = {:.0} kart units",
        t.path_points.len(), n, widths[0], widths[n/4], widths[n/2], widths[3*n/4], widths[n/2]*32.0);
}
