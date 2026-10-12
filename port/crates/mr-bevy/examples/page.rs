use mr_assets::{raw, vram::{self, Vram}};
fn main() {
    let mut d = mr_disc::Disc::open_default().unwrap();
    let exe = d.read("SLUS_012.37").unwrap();
    let env = raw::LoadEnv::from_exe(&exe, 3).unwrap();
    let dec = raw::decode(&d.read("COURSE_A/DOCK.RAW").unwrap(), &env).unwrap();
    let mut v = Vram::default(); dec.apply(&mut v);
    let (px, py, dep) = vram::tpage(0);
    let mut img = vec![0u8; 256 * 256 * 4];
    for t in 0..16u16 {
        let (cx, cy) = vram::clut(12400 + t);
        let (tu, tv) = ((t % 4) as usize * 64, (t / 4) as usize * 64);
        for y in tv..tv + 64 { for x in tu..tu + 64 {
            let c = vram::rgba(v.texel(dep, px, py, cx, cy, x as u8, y as u8));
            img[(y * 256 + x) * 4..][..4].copy_from_slice(&c);
        }}
    }
    std::fs::write("../analysis/bevy/page0.png", mr_assets::img::png_rgba(256, 256, &img)).unwrap();
    println!("{:?} {:?}", vram::tpage(0), vram::clut(12400));
}
