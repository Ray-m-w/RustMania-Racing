//! A standalone 6-kart DOCK race: the player idle (or a route's buttons from
//! the oracle grid), five AI opponents. Prints the opponents, every lap end,
//! the finishing order and per-kart lap times; `--csv OUT` writes per-tick
//! AI rows in the format of mr-ai's free run (for tools/oracle/ai_stats.py).
//!
//!   cargo run --release -p mr-sim --example full_race -- [--seed S] [--route R.json] [--ticks N] [--csv OUT]
use mr_sim::session::{Session, SessionConfig};

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

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    let mut cfg = SessionConfig::dock_default();
    if let Some(s) = opt("--seed") {
        cfg.rand_seed = u32::from_str_radix(s.trim_start_matches("0x"), if s.starts_with("0x") { 16 } else { 10 }).unwrap();
    }
    if let Some(s) = opt("--rng") {
        cfg.rng_start = u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
    }
    if a.iter().any(|x| x == "--oracle-lineup") {
        cfg.lineup = Some(mr_sim::session::ORACLE_LINEUP);
    }
    let ticks: u64 = opt("--ticks").map(|s| s.parse().unwrap()).unwrap_or(6000);
    let route = opt("--route").map(|p| parse_route(&std::fs::read_to_string(p).unwrap()));
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mut s = Session::with_config(&mut disc, cfg).unwrap();
    s.autopilot = a.iter().any(|x| x == "--autopilot");
    println!("grid: player slot {} (x {} z {})", s.kart.race_pos, s.kart.x, s.kart.z);
    for (slot, c, v, _) in s.kart_records() {
        let o = if slot == 0 { None } else { Some(&s.others[slot - 1]) };
        println!("  kart {slot}: character {c} vehicle {v} {} {}", o.map(|o| o.character_model.as_str()).unwrap_or("player"), o.map(|o| o.vehicle_model.as_str()).unwrap_or(""));
    }
    let mut csv = String::from("vb,kart,x,z,lap,lateral,seg,pt,buttons,fwd,width\n");
    let mut laps: Vec<Vec<i32>> = vec![Vec::new(); 1 + s.others.len()];
    let mut aligned_at = None;
    for t in 0..ticks {
        let btn = match &route {
            Some(r) => {
                if s.kart.race_time < 1430 {
                    0
                } else {
                    let base = *aligned_at.get_or_insert(t);
                    let vb = 2 * (t - base);
                    r.iter().take_while(|(v, _)| *v <= vb).last().map(|(_, m)| *m).unwrap_or(0)
                }
            }
            None => 0,
        };
        let before: Vec<u8> = std::iter::once(s.kart.lap).chain(s.others.iter().map(|o| o.kart.lap)).collect();
        if let Err(e) = s.tick(btn) {
            println!("stopped at tick {t}: {e}");
            break;
        }
        let ks: Vec<&mr_sim::KartState> = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).collect();
        for (i, k) in ks.iter().enumerate() {
            if k.lap != before[i] {
                laps[i].push(k.lap_time[before[i] as usize]);
            }
            if i > 0 && s.globals.race_clock > 0 && k.finished == 0 {
                csv += &format!(
                    "{},{},{},{},{},{},{},{},{},{},{}\n",
                    2 * t, i, k.x, k.z, k.lap, k.path_lateral, k.path_seg, k.path_pt, k.buttons, k.fwd_speed, k.path_width_r - k.path_width_l
                );
            }
        }
        if ks.iter().all(|k| k.finished != 0) && a.iter().all(|x| x != "--past-finish") {
            println!("all karts finished at tick {t}");
            break;
        }
    }
    println!("phase {:?}, race clock {}", s.phase(), s.race_clock());
    for r in s.results() {
        println!("  place {} kart {} finished {} race time {} laps {:?} best {}", r.place, r.slot, r.finished, r.race_time, laps[r.slot], r.best_lap);
    }
    if let Some(p) = opt("--csv") {
        let mut summ = String::new();
        for (i, l) in laps.iter().enumerate().skip(1) {
            summ += &format!("kart {i}: lap times {:?}
", l);
        }
        std::fs::write(p.replace(".csv", ".txt"), summ).unwrap();
        std::fs::write(p, csv).unwrap();
    }
}
