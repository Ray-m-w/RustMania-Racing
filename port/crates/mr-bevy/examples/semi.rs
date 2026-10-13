//! Semi-transparent DAT polygons of DOCK: per object, layer, flags, ABR, slot.
use mr_assets::dat::Dat;
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let dat = Dat::parse(&d.read("COURSE_A/DOCK.DAT").unwrap()).unwrap();
    let mut tot = std::collections::BTreeMap::new();
    for (i, o) in dat.objects.iter().enumerate() {
        let Some(m) = dat.mesh(o) else { continue };
        for g in &m.groups {
            if g.semi_transparent() {
                let k = (i >= dat.layer0_count, o.loaded_flags() & 0xC0 != 0, g.blend_bits(), if g.is_textured() { g.tex_slot() as i32 } else { -1 });
                *tot.entry(k).or_insert((0, Vec::new())) = { let e = tot.get(&k).cloned().unwrap_or((0, Vec::new())); let mut v = e.1; if !v.contains(&i) { v.push(i); } (e.0 + g.prims.len(), v) };
            }
        }
    }
    let mut cols = std::collections::BTreeMap::new();
    for o in dat.objects.iter().take(dat.layer0_count) {
        let Some(m) = dat.mesh(o) else { continue };
        for g in &m.groups {
            if g.semi_transparent() && g.blend_bits() == 1 {
                for p in &g.prims {
                    let c: Vec<[u8; 3]> = (0..if g.is_triangle() { 3 } else { 4 }).map(|k| m.vertices[p.idx[k] as usize].rgb()).collect();
                    *cols.entry((g.tex_slot(), format!("{:?} uv {:?}", c, p.uv[0]))).or_insert(0) += 1;
                }
            }
        }
    }
    for ((sl, c), n) in cols.iter().take(40) { println!("slot {sl} x{n}: {c}"); }
    for (k, (n, objs)) in tot { println!("far {} ground {} abr {} slot {}: {} prims in objects {:?}", k.0, k.1, k.2, k.3, n, objs); }
}
