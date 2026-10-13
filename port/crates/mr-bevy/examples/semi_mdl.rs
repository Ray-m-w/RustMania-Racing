//! Semi-transparent polygons in every MDL on the disc.
use mr_assets::mdl::Mdl;
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let paths: Vec<String> = d.paths().filter(|p| p.to_uppercase().contains(".MDL")).map(|s| s.to_string()).collect();
    for path in paths {
        let Ok(b) = d.read(&path) else { continue };
        let Ok(m) = Mdl::parse(&b) else { continue };
        let (mut n, mut s) = (0, 0);
        for (_, p) in m.meshes() { for poly in p.polys() { n += 1; if poly.semi_transparent() { s += 1; } } }
        if s > 0 { println!("{path}: {s} semi of {n}"); }
    }
}
