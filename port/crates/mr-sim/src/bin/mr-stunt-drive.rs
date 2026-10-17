//! mr-stunt-drive: a headless 1 PLAYER STUNT TRIAL with a simple driver
//! (post-release fix, 2026-10-17: FUN's lifts). The driver holds CROSS and
//! steers toward the stunt strip's centre a few quads ahead of the kart
//! (the strip is the course's own centre line, stunt.rs); nothing else.
//! Reports progress, the lifts, the finish; `--route FILE` writes the
//! buttons as an oracle route (tools/oracle/capture_route.py JSON; frame j
//! at VBlank `--vb0` + 2 j, the trial's 30 Hz frames).
//!
//! ```text
//! cargo run --release --offline -p mr-sim --bin mr-stunt-drive -- COURSE [--ticks N] [--look K] [--log N]
//!     [--poke X,Y,Z,HEADING [--heading H]] [--route FILE --vb0 VB] [--strip A-B] [--map X0,Z0,X1,Z1,STEP]
//! ```

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};

const CROSS: u16 = 1 << 14;
const LEFT: u16 = 1 << 7;
const RIGHT: u16 = 1 << 5;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let name = args.first().expect("COURSE").to_ascii_uppercase();
    let ticks: u64 = opt("--ticks").and_then(|v| v.parse().ok()).unwrap_or(9000);
    let look: usize = opt("--look").and_then(|v| v.parse().ok()).unwrap_or(3);
    let log: u64 = opt("--log").and_then(|v| v.parse().ok()).unwrap_or(150);
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let exe_bytes = disc.read("SLUS_012.37").unwrap();
    let exe = mr_sim::tables::ExeImage::parse(&exe_bytes).unwrap();
    let info = CourseInfo::find(&exe, &name).expect("course");
    let mut s = Session::with_config(&mut disc, SessionConfig::course(&info)).expect("session");
    let (a, b) = {
        let (a, b) = mr_sim::stunt::strip(&s.world).expect("a stunt course");
        (a.to_vec(), b.to_vec())
    };
    let quads = a.len() - 1;
    if let Some(r) = opt("--map") {
        // dev: the top surface (TER y / 100; W water, . none) over x0,z0,x1,z1,step
        let v: Vec<i32> = r.split(',').map(|x| x.parse().unwrap()).collect();
        let t = &s.world.ter;
        let mut z = v[1];
        while z <= v[3] {
            let mut line = format!("{z:8} ");
            let mut x = v[0];
            while x <= v[2] {
                let g = t.ground(x << 5, -100_000, z << 5);
                line += &match g.nearest {
                    Some(h) => format!("{:4}", h.y / 100),
                    None if g.water.is_some() => "   W".into(),
                    None => "   .".into(),
                };
                x += v[4];
            }
            println!("{line}");
            z += v[4];
        }
        let mut hdr = String::from("         ");
        let mut x = v[0];
        while x <= v[2] {
            hdr += &format!("{:4}", (x / 1000) % 1000);
            x += v[4];
        }
        println!("{hdr}  (x / 1000)");
    }
    if let Some(r) = opt("--strip") {
        let (x, y) = r.split_once('-').map(|(x, y)| (x.parse::<usize>().unwrap(), y.parse::<usize>().unwrap())).unwrap();
        for i in x..=y.min(quads) {
            println!("strip {i}: A {:?} B {:?}", a[i], b[i]);
        }
        let t = &s.world.ter;
        for (gi, g) in t.groups.iter().enumerate().take(12) {
            let vs = &t.vertices[g.vert_start as usize..g.vert_start as usize + g.vert_count.max(0) as usize];
            if vs.is_empty() {
                continue;
            }
            let lo = [vs.iter().map(|v| v.x).min().unwrap(), vs.iter().map(|v| v.y).min().unwrap(), vs.iter().map(|v| v.z).min().unwrap()];
            let hi = [vs.iter().map(|v| v.x).max().unwrap(), vs.iter().map(|v| v.y).max().unwrap(), vs.iter().map(|v| v.z).max().unwrap()];
            println!("TER group {gi}: {} verts {} faces {} walls, {lo:?}..{hi:?} (strip quad {})", g.vert_count, g.face_count, g.wall_count, mr_sim::stunt::strip_find(&s.world, (lo[0] + hi[0]) / 2, (lo[2] + hi[2]) / 2));
        }
        if let Some(g) = &s.world.course.gates {
            for (i, p) in g.chunks(2).enumerate() {
                let q = mr_sim::stunt::strip_find(&s.world, p[0][0], p[0][2]);
                println!("gate {i}: {:?} {:?} (strip quad {q})", p[0], p.get(1));
            }
        }
    }
    // --poke X,Y,Z,HEADING (TER units, 12-bit heading): the kart placed
    // there on the grid (as tools/oracle/capture_poked.py), the start gate
    // counted; --heading H then steers to that fixed heading instead of the
    // strip.
    if let Some(p) = opt("--poke") {
        let v: Vec<i32> = p.split(',').map(|x| x.parse().unwrap()).collect();
        let k = &mut s.kart;
        (k.x, k.y, k.z) = (v[0] << 5, v[1] << 5, v[2] << 5);
        (k.prev_x, k.prev_y, k.prev_z) = (k.x, k.y, k.z);
        k.heading = v[3] as i16;
        k.strip = mr_sim::stunt::strip_find(&s.world, v[0], v[2]);
        k.gate = 1;
        println!("poked: {:?} heading {} strip {}", &v[..3], v[3], k.strip);
    }
    let fixed: Option<f64> = opt("--heading").and_then(|v| v.parse::<f64>().ok()).map(|h| h * std::f64::consts::TAU / 4096.0);
    let mut route: Vec<(u64, u16)> = Vec::new();
    let mut best = 0i16;
    for t in 0..ticks {
        let k = &s.kart;
        let i = (k.strip.max(0) as usize).min(quads - 1);
        let j = (i + look).min(quads);
        let tx = (a[j][0] + b[j][0]) / 2;
        let tz = (a[j][2] + b[j][2]) / 2;
        let (kx, kz) = (k.x >> 5, k.z >> 5);
        // the game's heading: forward = (sin h, cos h) in (x, z) (kart.rs)
        let want = fixed.unwrap_or_else(|| ((tx - kx) as f64).atan2((tz - kz) as f64));
        let have = k.heading as f64 * std::f64::consts::TAU / 4096.0;
        let mut err = want - have;
        while err > std::f64::consts::PI {
            err -= std::f64::consts::TAU;
        }
        while err < -std::f64::consts::PI {
            err += std::f64::consts::TAU;
        }
        let mut btn = CROSS;
        if err > 0.12 {
            btn |= RIGHT;
        } else if err < -0.12 {
            btn |= LEFT;
        }
        if route.last().map(|r| r.1) != Some(btn) {
            route.push((t, btn));
        }
        if let Err(e) = s.tick(btn) {
            println!("tick {t}: {e}");
            break;
        }
        let k = &s.kart;
        best = best.max(k.strip);
        if log > 0 && t % log == 0 {
            let lifts: Vec<i32> = s.globals.objects.lifts.iter().map(|l| l.cur).collect();
            println!(
                "t{t:5} clock {:6} strip {:4}/{quads} (best {best}) pos ({}, {}, {}) hdg {} v ({}, {}, {}) err {:+.2} lifts {lifts:?}",
                s.globals.race_clock, k.strip, k.x >> 5, k.y >> 5, k.z >> 5, k.heading, k.vx, k.vy, k.vz, err
            );
        }
        if k.finished != 0 {
            println!("t{t}: out / finished: +0x4C7 {} race time {} (clock {}), strip {}", k.finished, k.race_time, s.globals.race_clock, k.strip);
            break;
        }
    }
    println!("furthest strip quad {best} of {quads}");
    if let Some(path) = opt("--route") {
        let vb0: u64 = opt("--vb0").and_then(|v| v.parse().ok()).unwrap_or(157);
        let mut out = String::from("{\"steps\": [");
        let names = [(CROSS, "cross"), (LEFT, "left"), (RIGHT, "right")];
        for (n, (t, btn)) in route.iter().enumerate() {
            let spec: Vec<&str> = names.iter().filter(|(m, _)| btn & m != 0).map(|e| e.1).collect();
            out += &format!("{}[{}, \"{}\"]", if n > 0 { ", " } else { "" }, vb0 + 2 * t, spec.join("+"));
        }
        out += "]}\n";
        std::fs::write(&path, out).expect("write route");
        println!("route: {} steps -> {path}", route.len());
    }
}
