//! 60 Hz logic experiment (SESSION-3 item 4 stretch): run the same inputs
//! through the standalone session at dt = 2 (the game's 30 Hz) and dt = 1
//! (60 Hz), compare at equal game time (tick k at dt 2 = tick 2k at dt 1).
//!   cargo run --release --example dt1 -- ROUTE.json [ticks]
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
    let route = parse_route(&std::fs::read_to_string(&a[1]).unwrap());
    let ticks: u64 = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(1500);
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mk = |d: &mut mr_disc::Disc| {
        let mut s = Session::new(d).unwrap();
        s.leader_progress_override = Some(1000);
        s.kart.catchup = 180;
        s
    };
    let (mut s2, mut s1) = (mk(&mut disc), mk(&mut disc));
    // align both to the oracle grid (143 idle ticks of dt 2 = 286 VBlanks)
    for _ in 0..143 { s2.tick_dt(0, 2).unwrap(); }
    for _ in 0..286 { s1.tick_dt(0, 1).unwrap(); }
    let mut worst = (0f64, 0u64);
    let mut worst_h = (0f64, 0u64);
    let mut sum = 0f64;
    let mut n = 0u64;
    for k in 0..ticks {
        let vb = 2 * k;
        if let Err(e) = s2.tick_dt(at(&route, vb), 2) { println!("dt2 stopped at tick {k}: {e:?}"); break; }
        let mut ok = true;
        for h in 0..2 {
            if let Err(e) = s1.tick_dt(at(&route, vb + h), 1) { println!("dt1 stopped at tick {k}: {e:?}"); ok = false; break; }
        }
        if !ok { break; }
        let (x2, y2, z2, h2) = pos(&s2);
        let (x1, y1, z1, h1) = pos(&s1);
        let d = (((x2 - x1).pow(2) + (y2 - y1).pow(2) + (z2 - z1).pow(2)) as f64).sqrt();
        let dh = (((h2 - h1) % 4096 + 6144) % 4096 - 2048).abs() as f64 * 360.0 / 4096.0;
        sum += d; n += 1;
        if d > worst.0 { worst = (d, k); }
        if dh > worst_h.0 { worst_h = (dh, k); }
        if k % 100 == 0 || k == 299 {
            println!("tick {k:5}: |dpos| {d:9.0}  dheading {dh:6.2} deg");
        }
    }
    println!("max |dpos| {:.0} at tick {}, mean {:.0}; max dheading {:.2} deg at tick {} (tolerance: 1169 units, 2 deg)", worst.0, worst.1, sum / n.max(1) as f64, worst_h.0, worst_h.1);
}
