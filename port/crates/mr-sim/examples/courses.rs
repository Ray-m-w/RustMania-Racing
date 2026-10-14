//! Course table + per-course data census (Session 6, track P): the game's
//! course rows (course.rs) and each course's DAT chunk ids / counts, the
//! special (moving object) slots, and the TER's surface types in use.
//! `cargo run --release --offline -p mr-sim --example courses [-- --all]`
use mr_sim::course::CourseInfo;

fn main() {
    let all = std::env::args().any(|a| a == "--all");
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").unwrap()).unwrap();
    let list = if all { CourseInfo::all(&exe) } else { CourseInfo::races(&exe) };
    for c in list {
        println!("{:>2} {:<20} cup {:>2}/{:>2} type {} mode {} karts {} music {:>3} intro {:>4} flags {:04X} fog {:02X}{:02X}{:02X} boss {:>3} r9 {:>3} lod {:>3} rec {}",
            c.id, c.path, c.cup, c.slot, c.race_type, c.mode, c.karts, c.music, c.intro_clip, c.flags, c.fog[0], c.fog[1], c.fog[2], c.boss, c.row9, c.lod_bias, c.lap_record);
        let Ok(b) = disc.read(&c.file("DAT")) else { println!("   no DAT"); continue };
        let d = mr_assets::dat::Dat::parse(&b).unwrap();
        let ch: Vec<String> = d.chunks.iter().map(|x| format!("{:X}:{}", x.id, x.count)).collect();
        println!("   chunks {}", ch.join(" "));
        let sp = d.special.iter().filter(|x| x.is_some()).count();
        println!("   objects {} (layer0 {}), special slots {} used / {}, block128 {}", d.objects.len(), d.layer0_count, sp, d.special.len(), d.block128.len());
        if let Ok(t) = disc.read(&c.file("TER")) {
            let t = mr_assets::ter::Ter::parse(&t).unwrap();
            let mut ty = std::collections::BTreeMap::<u16, usize>::new();
            for f in &t.faces {
                *ty.entry(f.kind as u16).or_default() += 1;
            }
            for (gi, g) in t.groups.iter().enumerate() {
                let dis = (0..g.face_count.max(0) as usize).filter(|&k| t.faces[g.face_start as usize + k].disabled()).count();
                let wdis = (0..g.wall_count.max(0) as usize).filter(|&k| t.walls[g.wall_start as usize + k].disabled).count();
                println!("     group {gi}: verts {} faces {} ({} disabled) walls {} ({} disabled) unk0 {:x} unk16 {:x}", g.vert_count, g.face_count, dis, g.wall_count, wdis, g.unk0, g.unk16);
            }
            println!("   ter faces {} walls {} path segs {} pts {} no_water_y {} surface kinds {:?}", t.faces.len(), t.walls.len(), t.path_segments.len(), t.path_points.len(), t.no_water_y, ty);
        }
    }
}
