//! Debug: an AI-driven race on one course, printing a kart's state every
//! N ticks. `cargo run --release --offline -p mr-sim --example course_trace -- COURSE [slot] [every] [from] [to] [hz]`
use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, &a[0]).expect("course");
    let slot: usize = a.get(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    let every: u64 = a.get(2).and_then(|x| x.parse().ok()).unwrap_or(30);
    let from: u64 = a.get(3).and_then(|x| x.parse().ok()).unwrap_or(0);
    let to: u64 = a.get(4).and_then(|x| x.parse().ok()).unwrap_or(3000);
    let hz: u8 = a.get(5).and_then(|x| x.parse().ok()).unwrap_or(30);
    let mut s = Session::with_config(&mut disc, SessionConfig { logic_hz: hz, ..SessionConfig::course(&info) }).unwrap();
    s.autopilot = true;
    while s.ticks < to {
        if let Err(e) = s.tick(0) { println!("ERR {e}"); break; }
        if s.ticks >= from && s.ticks % every == 0 {
            let k = if slot == 0 { &s.kart } else { &s.others[slot - 1].kart };
            println!("t {:5} pos ({:8},{:8},{:8}) v ({:6},{:6},{:6}) hd {:4} gc {} e3 {} e4 {} car {} lap {} prog {} path {}/{} fwd {} eng {} jets {:?}",
                s.ticks, k.x >> 5, k.y >> 5, k.z >> 5, k.vx, k.vy, k.vz, k.heading, k.ground_contact, k.object_e3, k.object_e4, k.carried, k.lap, k.progress, k.path_seg, k.path_pt, k.fwd_speed, k.engine,
                s.globals.objects.jets.iter().map(|j| j.active).collect::<Vec<_>>());
        }
    }
}
