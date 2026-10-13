//! Session 5 G: proof that the faithful 30 Hz path is unchanged by the 60 Hz
//! mode work. Runs a few long sessions at 30 Hz (full six-kart races with
//! items, AI-driven and idle players, a recorded player route) and prints an
//! FNV-1a hash of the whole race state every 500 ticks: every kart record,
//! AI fields, the position history, the race globals (items, particles,
//! sparks, sound queue), the camera, the water animation and both random
//! sources. Run it on the old and the new tree and diff the output.
//!
//!   cargo run --release -p mr-sim --example hz30_fingerprint > a.txt
use mr_sim::session::{Session, SessionConfig, ORACLE_LINEUP};

struct Fnv(u64);
impl Fnv {
    fn eat(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= x as u64;
            self.0 = self.0.wrapping_mul(0x100_0000_01B3);
        }
    }
}

fn state_hash(s: &Session) -> u64 {
    let mut h = Fnv(0xCBF2_9CE4_8422_2325);
    let mut rec = s.record().to_vec();
    s.kart().to_record(&mut rec);
    h.eat(&rec);
    h.eat(format!("{:?}{:?}", s.kart().trail, s.player_ai).as_bytes());
    for o in &s.others {
        let mut r = o.record.clone();
        o.kart.to_record(&mut r);
        h.eat(&r);
        h.eat(format!("{:?}{:?}", o.kart.trail, o.ai).as_bytes());
    }
    // The 60 Hz mode's own fields (off at 30 Hz) are left out of the hash so
    // that the old tree and the new one hash the same state the same way.
    let g = format!("{:?}", s.globals);
    let g = match g.find(", hz60: ") {
        Some(i) => g[..i].to_string(),
        None => g.trim_end_matches(" }").to_string(),
    };
    h.eat(g.as_bytes());
    h.eat(format!("{:?}{:?}{:?}{}{}", s.camera, s.water, s.rand_seed(), s.pick, s.best_start).as_bytes());
    h.0
}

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

fn run(name: &str, mut s: Session, ticks: u64, mut btn: impl FnMut(u64) -> u16) {
    for t in 0..ticks {
        if let Err(e) = s.tick(btn(t)) {
            println!("{name}: stopped at tick {t}: {e}");
            break;
        }
        if (t + 1) % 500 == 0 || t + 1 == ticks {
            println!("{name} tick {:5}: {:016x}", t + 1, state_hash(&s));
        }
    }
}

fn main() {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    let mut disc = mr_disc::Disc::open_default().unwrap();
    // 1. The default race, Kermit on autopilot, items on.
    let mut s = Session::new(&mut disc).unwrap();
    s.autopilot = true;
    run("default-autopilot", s, 4500, |_| 0);
    // 2. Other seeds with the oracle line-up, autopilot, items on.
    for seed in [2u32, 3] {
        let cfg = SessionConfig { rand_seed: seed, lineup: Some(ORACLE_LINEUP), ..SessionConfig::dock_default() };
        let mut s = Session::with_config(&mut disc, cfg).unwrap();
        s.autopilot = true;
        run(&format!("seed{seed}-autopilot"), s, 4000, |_| 0);
    }
    // 3. Idle player, items off.
    let cfg = SessionConfig { items: false, ..SessionConfig::dock_default() };
    run("idle-itemsoff", Session::with_config(&mut disc, cfg).unwrap(), 3000, |_| 0);
    // 4. The solo session from the oracle grid driving the recorded routes.
    for r in ["race3", "items1"] {
        let route = parse_route(&std::fs::read_to_string(format!("tools/oracle/routes/{r}.json")).unwrap());
        let mut s = Session::solo(&mut disc).unwrap();
        s.leader_progress_override = Some(1000);
        s.race_pos_override = Some(6);
        s.idle_until_race_time(1430).unwrap();
        s.kart.catchup = 180;
        run(&format!("solo-{r}"), s, 4400, |t| {
            let vb = 2 * t;
            route.iter().take_while(|(v, _)| *v <= vb).last().map(|(_, m)| *m).unwrap_or(0)
        });
    }
    // 5. The six-kart race driving a route (player human, AI around it).
    let route = parse_route(&std::fs::read_to_string("tools/oracle/routes/items2.json").unwrap());
    let s = Session::new(&mut disc).unwrap();
    run("race-items2", s, 4500, |t| {
        let vb = 2 * t.saturating_sub(383);
        if t < 383 { 0 } else { route.iter().take_while(|(v, _)| *v <= vb).last().map(|(_, m)| *m).unwrap_or(0) }
    });
}
