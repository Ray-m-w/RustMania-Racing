//! 60 Hz logic experiment, mr-sim side (same comparison as
//! mr-bevy/examples/dt1.rs, aligned for the session that starts at race
//! load): both sessions idle to the oracle grid (race time 1430: the
//! countdown, GO, 142 / 285 race ticks), then replay the route's buttons at
//! dt = 2 and dt = 1 and compare at equal game time (tick k at dt 2 = ticks
//! 2k, 2k+1 at dt 1). Catch-up as the oracle race (6th, leader far ahead).
//!   cargo run --release -p mr-sim --example dt_compare -- tools/oracle/routes/lap1.json [ticks] [--every N]
use mr_sim::session::Session;

fn parse_route(text: &str) -> Vec<(u64, u16)> {
    let names = ["select", "", "", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"];
    let body = &text[text.find("\"steps\"").unwrap()..];
    let mut out = Vec::new();
    let mut rest = &body[body.find('[').unwrap() + 1..];
    while let Some(i) = rest.find('[') {
        let end = rest[i..].find(']').unwrap() + i;
        let item = &rest[i + 1..end];
        let mut parts = item.splitn(2, ',');
        let vb: u64 = parts.next().unwrap().trim().parse().unwrap();
        let spec = parts.next().unwrap_or("").trim().trim_matches('"');
        let mut m = 0u16;
        for b in spec.split('+').filter(|x| !x.is_empty()) {
            m |= 1 << names.iter().position(|n| *n == b).unwrap();
        }
        out.push((vb, m));
        rest = &rest[end + 1..];
    }
    out
}

fn at(route: &[(u64, u16)], vb: u64) -> u16 {
    route.iter().take_while(|(v, _)| *v <= vb).last().map(|(_, m)| *m).unwrap_or(0)
}

fn pos(s: &Session) -> (i64, i64, i64, i64) {
    let r = s.record();
    let i = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]) as i64;
    (i(0), i(4), i(8), i16::from_le_bytes([r[0xE], r[0xF]]) as i64)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    let route = parse_route(&std::fs::read_to_string(&a[1]).unwrap());
    let ticks: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1500);
    let every: u64 = a.iter().position(|x| x == "--every").and_then(|i| a.get(i + 1)).map(|s| s.parse().unwrap()).unwrap_or(100);
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mk = |d: &mut mr_disc::Disc, dt: i32| {
        let mut s = Session::solo(d).unwrap();
        s.leader_progress_override = Some(1000);
        s.race_pos_override = Some(6);
        while s.kart.race_time < 1430 {
            s.tick_dt(0, dt).unwrap();
        }
        s.kart.catchup = 180;
        s
    };
    let (mut s2, mut s1) = (mk(&mut disc, 2), mk(&mut disc, 1));
    println!("aligned: dt2 {} ticks, dt1 {} ticks, race time {} / {}", s2.ticks, s1.ticks, s2.kart.race_time, s1.kart.race_time);
    let mut worst = (0f64, 0u64);
    let mut worst_h = (0f64, 0u64);
    let mut first_far = None;
    let mut sum = 0f64;
    let mut n = 0u64;
    for k in 0..ticks {
        let vb = 2 * k;
        if let Err(e) = s2.tick_dt(at(&route, vb), 2) {
            println!("dt2 stopped at tick {k}: {e:?}");
            break;
        }
        let mut ok = true;
        for h in 0..2 {
            if let Err(e) = s1.tick_dt(at(&route, vb + h), 1) {
                println!("dt1 stopped at tick {k}: {e:?}");
                ok = false;
                break;
            }
        }
        if !ok {
            break;
        }
        let (x2, y2, z2, h2) = pos(&s2);
        let (x1, y1, z1, h1) = pos(&s1);
        let d = (((x2 - x1).pow(2) + (y2 - y1).pow(2) + (z2 - z1).pow(2)) as f64).sqrt();
        let dh = (((h2 - h1) % 4096 + 6144) % 4096 - 2048).abs() as f64 * 360.0 / 4096.0;
        sum += d;
        n += 1;
        if d > worst.0 {
            worst = (d, k);
        }
        if dh > worst_h.0 {
            worst_h = (dh, k);
        }
        if first_far.is_none() && (d > 1169.0 || dh > 2.0) {
            first_far = Some(k);
        }
        if std::env::var("DT_FIELDS").is_ok() && k < 40 {
            let (r2, r1) = (s2.record(), s1.record());
            let d: Vec<String> = mr_sim::kart::FIELD_OFFSETS
                .iter()
                .filter(|&&(_, o, z)| r2[o..o + z] != r1[o..o + z])
                .map(|&(nm, o, z)| format!("{nm} {}/{}", mr_sim::harness::val(&r2[o..o + z]), mr_sim::harness::val(&r1[o..o + z])))
                .collect();
            println!("  tick {k} btn {:#x}: {}", at(&route, vb), d.join(", "));
        }
        if k % every == 0 {
            let (a2, a1) = (s2.kart(), s1.kart());
            println!(
                "tick {k:5}: |dpos| {d:9.0}  dheading {dh:6.2} deg  speed {} / {}  catchup {} / {}  steer {} / {}",
                a2.fwd_speed, a1.fwd_speed, a2.catchup, a1.catchup, a2.steer, a1.steer
            );
        }
    }
    println!(
        "max |dpos| {:.0} at tick {}, mean {:.0}; max dheading {:.2} deg at tick {}; first beyond tolerance (1169 units / 2 deg): {:?}",
        worst.0,
        worst.1,
        sum / n.max(1) as f64,
        worst_h.0,
        worst_h.1,
        first_far
    );
}
