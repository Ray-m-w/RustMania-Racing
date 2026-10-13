//! Session 5 G: feel tolerance of the 60 Hz logic mode against the faithful
//! 30 Hz mode (the 30 Hz port is exact against the oracle, so it is the
//! reference; no oracle run needed). Same inputs per 1/30 s in both modes
//! (at 60 Hz each 30 Hz input is held for both ticks of the pair).
//!
//!   cargo run --release -p mr-sim --example feel60 -- [--races N] [--json OUT]
//!
//! Tests (docs/spec/kart-physics-ledger.md "60 Hz mode", feel table):
//! 1. scripted drives from the oracle grid (solo, catch-up as the oracle
//!    race): acceleration curve and top speed (CROSS held), jump (SQUARE),
//!    turn at full lock (LEFT held);
//! 2. the recorded player routes (tools/oracle/routes): lap times, racing
//!    line (distance of the 60 Hz path from the 30 Hz path; lateral offset
//!    from the TER line as a fraction of the track width);
//! 3. six-kart races with items, player idle, N seeds (the item-statistics
//!    races of Session 4): AI lap means per kart, finishing places, item use
//!    per race (tolerance 2 * sqrt(sd_a^2/n + sd_b^2/n), docs/spec/items.md).
use mr_sim::items::{ItemEvent, PickupKind};
use mr_sim::session::{Session, SessionConfig};
use mr_sim::KartState;

/// One sample per 1/30 s of game time (after the pair at 60 Hz).
#[derive(Clone, Copy, Debug)]
struct Sample {
    x: i64,
    y: i64,
    z: i64,
    heading: i32,
    fwd: i32,
    ground: bool,
    lateral: i32,
    width: i32,
}

fn sample(k: &KartState) -> Sample {
    Sample {
        x: k.x as i64,
        y: k.y as i64,
        z: k.z as i64,
        heading: k.heading as i32,
        fwd: k.fwd_speed,
        ground: k.ground_contact != 0,
        lateral: k.path_lateral as i32,
        width: (k.path_width_r as i32 - k.path_width_l as i32),
    }
}

fn session(disc: &mut mr_disc::Disc, cfg: SessionConfig) -> Session {
    Session::with_config(disc, cfg).unwrap()
}

/// The solo session at the oracle grid (race time 1430), catch-up as the
/// oracle race (mr-standalone --catchup-as-6th).
fn grid(disc: &mut mr_disc::Disc, hz: u8) -> Session {
    let mut s = session(disc, SessionConfig { logic_hz: hz, ..SessionConfig::dock_solo() });
    s.leader_progress_override = Some(1000);
    s.race_pos_override = Some(6);
    s.idle_until_race_time(1430).unwrap();
    s.kart.catchup = 180;
    s
}

/// Advance one 1/30 s step with `b` (two ticks at 60 Hz). False if the
/// session stopped.
fn step(s: &mut Session, b: u16) -> bool {
    let n = if s.logic_hz() == 60 { 2 } else { 1 };
    for _ in 0..n {
        if let Err(e) = s.tick(b) {
            eprintln!("{} Hz session stopped at tick {}: {e}", s.logic_hz(), s.ticks);
            return false;
        }
    }
    true
}

struct Drive {
    samples: Vec<Sample>,
    /// Lap times (race clock units, 300 = 1 s) of completed laps.
    laps: Vec<i32>,
    /// Samples taken at every tick (for jump peaks at 60 Hz resolution).
    fine: Vec<Sample>,
}

fn drive(disc: &mut mr_disc::Disc, hz: u8, steps: u64, input: &dyn Fn(u64) -> u16) -> Drive {
    let mut s = grid(disc, hz);
    let mut d = Drive { samples: Vec::new(), laps: Vec::new(), fine: Vec::new() };
    for t in 0..steps {
        let b = input(2 * t);
        let n = if hz == 60 { 2 } else { 1 };
        let lap0 = s.kart.lap;
        let mut ok = true;
        for _ in 0..n {
            if s.tick(b).is_err() {
                ok = false;
                break;
            }
            d.fine.push(sample(&s.kart));
        }
        if !ok {
            break;
        }
        if s.kart.lap != lap0 {
            d.laps.push(s.kart.lap_time[lap0 as usize]);
        }
        d.samples.push(sample(&s.kart));
        if s.kart.finished != 0 && s.kart.lap as i16 >= 3 {
            break;
        }
    }
    d
}

fn pct(a: f64, b: f64) -> f64 {
    if a == 0.0 { 0.0 } else { (b - a) / a * 100.0 }
}

struct Row {
    test: String,
    a: String,
    b: String,
    diff: String,
    limit: String,
    ok: bool,
}

fn row(rows: &mut Vec<Row>, test: &str, a: f64, b: f64, rel_limit_pct: f64) {
    let d = pct(a, b);
    rows.push(Row {
        test: test.into(),
        a: format!("{a:.1}"),
        b: format!("{b:.1}"),
        diff: format!("{d:+.2} %"),
        limit: format!("{rel_limit_pct} %"),
        ok: d.abs() <= rel_limit_pct,
    });
}

fn row_abs(rows: &mut Vec<Row>, test: &str, a: f64, b: f64, limit: f64, unit: &str) {
    let d = b - a;
    rows.push(Row {
        test: test.into(),
        a: format!("{a:.3}"),
        b: format!("{b:.3}"),
        diff: format!("{d:+.3}{unit}"),
        limit: format!("{limit}{unit}"),
        ok: d.abs() <= limit,
    });
}

/// Acceleration: CROSS from VBlank 60 (the cross route), 8 s.
fn accel(disc: &mut mr_disc::Disc, rows: &mut Vec<Row>) {
    let inp = |vb: u64| if vb >= 60 { 0x4000 } else { 0 };
    let a = drive(disc, 30, 270, &inp);
    let b = drive(disc, 60, 270, &inp);
    let top = |d: &Drive| d.samples.iter().map(|s| s.fwd).max().unwrap_or(0) as f64;
    // Speed at 0.5 / 1 / 2 s after the press (sample index = 30 + 15 * k).
    let sp = |d: &Drive, t: f64| d.samples.get(30 + (t * 30.0) as usize).map(|s| s.fwd as f64).unwrap_or(0.0);
    let reach = |d: &Drive, f: f64| {
        let tp = top(d);
        d.samples.iter().skip(30).position(|s| s.fwd as f64 >= f * tp).map(|i| i as f64 / 30.0).unwrap_or(f64::NAN)
    };
    let dist = |d: &Drive, t: f64| {
        let (s0, s1) = (d.samples[30], d.samples[30 + (t * 30.0) as usize]);
        (((s1.x - s0.x).pow(2) + (s1.z - s0.z).pow(2)) as f64).sqrt()
    };
    row(rows, "accel: top forward speed", top(&a), top(&b), 1.0);
    for t in [0.5, 1.0, 2.0] {
        row(rows, &format!("accel: speed {t} s after CROSS"), sp(&a, t), sp(&b, t), 2.0);
    }
    row(rows, "accel: time to 90 % of top (s)", reach(&a, 0.9), reach(&b, 0.9), 3.0);
    row(rows, "accel: distance in 3 s", dist(&a, 3.0), dist(&b, 3.0), 1.0);
}

/// Jump: CROSS from 60, SQUARE held 240..300.
fn jump(disc: &mut mr_disc::Disc, rows: &mut Vec<Row>) {
    let inp = |vb: u64| {
        let mut m = if vb >= 60 { 0x4000 } else { 0 };
        if (240..300).contains(&vb) {
            m |= 0x8000;
        }
        m
    };
    let a = drive(disc, 30, 200, &inp);
    let b = drive(disc, 60, 200, &inp);
    // Takeoff: first tick off the ground at/after VBlank 240; height = takeoff
    // y - min y (y grows downward); airtime until the ground again (s).
    // Height and airtime from every tick; the length between the samples
    // (1/30 s) before take-off and after landing in both modes, so the
    // 60 Hz mode's finer ticks do not shorten it.
    let meas = |d: &Drive, per_s: f64, first: usize| {
        let i0 = d.fine.iter().enumerate().skip(first).find(|(_, s)| !s.ground).map(|(i, _)| i).unwrap();
        let y0 = d.fine[i0 - 1].y;
        let land = d.fine.iter().enumerate().skip(i0).find(|(_, s)| s.ground).map(|(i, _)| i).unwrap_or(d.fine.len());
        let ymin = d.fine[i0..land].iter().map(|s| s.y).min().unwrap();
        let j0 = d.samples.iter().enumerate().skip(120).find(|(_, s)| !s.ground).map(|(j, _)| j).unwrap();
        let j1 = d.samples.iter().enumerate().skip(j0).find(|(_, s)| s.ground).map(|(j, _)| j).unwrap_or(d.samples.len() - 1);
        ((y0 - ymin) as f64, (land - i0) as f64 / per_s, {
            let (p, q) = (d.samples[j0 - 1], d.samples[j1]);
            (((q.x - p.x).pow(2) + (q.z - p.z).pow(2)) as f64).sqrt()
        })
    };
    let (ha, ta, la) = meas(&a, 30.0, 120);
    let (hb, tb, lb) = meas(&b, 60.0, 240);
    row(rows, "jump: height", ha, hb, 3.0);
    row_abs(rows, "jump: airtime (s)", ta, tb, 0.034, " s");
    row(rows, "jump: length", la, lb, 3.0);
}

/// Turn at full lock: CROSS from 60, LEFT held 150..270; measured over
/// 180..270 (steering saturated): radius = speed / yaw rate.
fn turn(disc: &mut mr_disc::Disc, rows: &mut Vec<Row>) {
    let inp = |vb: u64| {
        let mut m = if vb >= 60 { 0x4000 } else { 0 };
        if (150..270).contains(&vb) {
            m |= 0x80;
        }
        m
    };
    let a = drive(disc, 30, 150, &inp);
    let b = drive(disc, 60, 150, &inp);
    let meas = |d: &Drive| {
        let (i0, i1) = (90usize, 134usize);
        let mut turn = 0i64;
        for i in i0..i1 {
            let dh = (((d.samples[i + 1].heading - d.samples[i].heading) % 4096 + 6144) % 4096 - 2048) as i64;
            turn += dh;
        }
        let secs = (i1 - i0) as f64 / 30.0;
        let mut path = 0f64;
        for i in i0..i1 {
            let (p, q) = (d.samples[i], d.samples[i + 1]);
            path += (((q.x - p.x).pow(2) + (q.z - p.z).pow(2)) as f64).sqrt();
        }
        let rate = turn as f64 * 360.0 / 4096.0 / secs;
        let radius = path / (turn.abs() as f64 * std::f64::consts::TAU / 4096.0);
        (rate.abs(), radius)
    };
    let (ra, rada) = meas(&a);
    let (rb, radb) = meas(&b);
    row(rows, "turn: yaw rate at full lock (deg/s)", ra, rb, 3.0);
    row(rows, "turn: radius at full lock", rada, radb, 3.0);
}

/// Distance of each 60 Hz sample from the 30 Hz path (polyline), units.
fn line_dev(a: &[Sample], b: &[Sample]) -> Vec<f64> {
    let mut out = Vec::new();
    for (i, p) in b.iter().enumerate() {
        let lo = i.saturating_sub(300);
        let hi = (i + 300).min(a.len().saturating_sub(1));
        let mut best = f64::MAX;
        for j in lo..hi {
            let (u, v) = (a[j], a[j + 1]);
            let (ux, uz, vx, vz, px, pz) = (u.x as f64, u.z as f64, v.x as f64, v.z as f64, p.x as f64, p.z as f64);
            let (dx, dz) = (vx - ux, vz - uz);
            let l2 = dx * dx + dz * dz;
            let t = if l2 > 0.0 { (((px - ux) * dx + (pz - uz) * dz) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let (cx, cz) = (ux + t * dx, uz + t * dz);
            best = best.min(((px - cx).powi(2) + (pz - cz).powi(2)).sqrt());
        }
        out.push(best);
    }
    out
}

fn quantile(v: &[f64], q: f64) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return f64::NAN;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * q) as usize]
}

/// The player's kart driven by the game's AI (autopilot) in the solo
/// session from race load, items off (driving only), 3 laps: samples per
/// 1/30 s, lap times; with `rec`, a snapshot and the buttons at every 1/30 s.
fn autopilot(disc: &mut mr_disc::Disc, hz: u8, seed: u32, mut rec: Option<&mut Vec<(mr_sim::session::Snapshot, u16)>>) -> Drive {
    let mut s = session(disc, SessionConfig { logic_hz: hz, rand_seed: seed, items: false, ..SessionConfig::dock_solo() });
    s.autopilot = true;
    let mut d = Drive { samples: Vec::new(), laps: Vec::new(), fine: Vec::new() };
    for _ in 0..6000 {
        let lap0 = s.kart.lap;
        let snap = rec.as_ref().map(|_| s.snapshot());
        if !step(&mut s, 0) {
            break;
        }
        if let (Some(r), Some(sn)) = (rec.as_mut(), snap) {
            r.push((sn, s.kart.buttons));
        }
        if s.kart.lap != lap0 {
            d.laps.push(s.kart.lap_time[lap0 as usize]);
        }
        d.samples.push(sample(&s.kart));
        if s.kart.finished != 0 {
            break;
        }
    }
    d
}

fn line_rows(rows: &mut Vec<Row>, name: &str, a: &[Drive], b: &[Drive]) {
    let mut dev = Vec::new();
    let mut widths = Vec::new();
    for (da, db) in a.iter().zip(b) {
        let m = da.samples.len().min(db.samples.len());
        dev.extend(line_dev(&da.samples[..m], &db.samples[..m]));
        widths.extend(da.samples.iter().filter(|s| s.width > 0).map(|s| s.width as f64 * 32.0));
    }
    let w = quantile(&widths, 0.5);
    rows.push(Row {
        test: format!("{name}: distance of the 60 Hz path from the 30 Hz path, median (p90) x track width"),
        a: "-".into(),
        b: format!("{:.3} ({:.3})", quantile(&dev, 0.5) / w, quantile(&dev, 0.9) / w),
        diff: format!("{:.0} ({:.0}) units", quantile(&dev, 0.5), quantile(&dev, 0.9)),
        limit: "median 0.05".into(),
        ok: quantile(&dev, 0.5) / w <= 0.05,
    });
    let lat = |d: &[Drive], q: f64| quantile(&d.iter().flat_map(|d| d.samples.iter()).filter(|s| s.width > 0).map(|s| s.lateral as f64 / s.width as f64).collect::<Vec<_>>(), q);
    for q in [0.1, 0.5, 0.9] {
        row_abs(rows, &format!("{name}: lateral offset from the TER line / width, p{}", (q * 100.0) as i32), lat(a, q), lat(b, q), 0.10, "");
    }
}

fn player(disc: &mut mr_disc::Disc, rows: &mut Vec<Row>, runs: u32) {
    // 1. Closed loop: the game's AI drives Kermit (items off) at either
    //    rate; `runs` seeds (the AI's path choices).
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut rec = Vec::new();
    for r in 0..runs {
        a.push(autopilot(disc, 30, 1 + r, if r == 0 { Some(&mut rec) } else { None }));
        b.push(autopilot(disc, 60, 1 + r, None));
    }
    if std::env::var("FEEL_VERBOSE").is_ok() {
        for (r, (da, db)) in a.iter().zip(&b).enumerate() {
            eprintln!("run {r}: laps 30 Hz {:?} 60 Hz {:?}", da.laps, db.laps);
        }
    }
    for l in 0..3 {
        let la: Vec<f64> = a.iter().filter_map(|d| d.laps.get(l)).map(|&t| t as f64 / 300.0).collect();
        let lb: Vec<f64> = b.iter().filter_map(|d| d.laps.get(l)).map(|&t| t as f64 / 300.0).collect();
        row(rows, &format!("player (AI-driven, {runs} runs): lap {} mean (s)", l + 1), mean_sd(&la).0, mean_sd(&lb).0, 2.0);
    }
    let ra: Vec<f64> = a.iter().map(|d| d.laps.iter().sum::<i32>() as f64 / 300.0).collect();
    let rb: Vec<f64> = b.iter().map(|d| d.laps.iter().sum::<i32>() as f64 / 300.0).collect();
    row(rows, &format!("player (AI-driven, {runs} runs): race time mean (s)"), mean_sd(&ra).0, mean_sd(&rb).0, 1.0);
    line_rows(rows, "player (AI-driven)", &a, &b);

    // 2. Recorded inputs: the first run's 30 Hz drive. From its state at
    //    the start of every 1 s window, its buttons are replayed open loop
    //    at 30 Hz and at 60 Hz (each word held 1/30 s); the two end states
    //    are compared (an open-loop replay over a whole lap only measures
    //    chaos: one unit of difference grows into a different wall hit).
    let mut s30 = session(disc, SessionConfig { items: false, ..SessionConfig::dock_solo() });
    let mut s60 = session(disc, SessionConfig { items: false, ..SessionConfig::dock_solo() });
    let mut perr = Vec::new();
    let mut herr = Vec::new();
    let mut verr = Vec::new();
    let win = 30usize;
    let mut i = 300; // after GO
    while i + win < rec.len() {
        s30.restore(&rec[i].0);
        s60.restore(&rec[i].0);
        s60.set_logic_hz(60);
        // The recording's buttons, not the AI's.
        s30.autopilot = false;
        s60.autopilot = false;
        let p0 = (s30.kart.x as f64, s30.kart.z as f64);
        let mut ok = true;
        for (_, btn) in &rec[i..i + win] {
            ok &= step(&mut s30, *btn) && step(&mut s60, *btn);
        }
        if ok {
            let (ka, kb) = (&s30.kart, &s60.kart);
            let travel = ((ka.x as f64 - p0.0).powi(2) + (ka.z as f64 - p0.1).powi(2)).sqrt();
            let d = (((ka.x - kb.x) as f64).powi(2) + ((ka.y - kb.y) as f64).powi(2) + ((ka.z - kb.z) as f64).powi(2)).sqrt();
            perr.push(d);
            herr.push(((((ka.heading - kb.heading) as i32 % 4096) + 6144) % 4096 - 2048).abs() as f64 * 360.0 / 4096.0);
            if travel > 0.0 {
                verr.push(d / travel * 100.0);
            }
        }
        i += win;
    }
    let n = perr.len();
    rows.push(Row {
        test: format!("player, recorded inputs ({n} windows of 1 s): position difference median (p90), units"),
        a: "-".into(),
        b: format!("{:.0} ({:.0})", quantile(&perr, 0.5), quantile(&perr, 0.9)),
        diff: format!("{:.2} % ({:.2} %) of the distance driven", quantile(&verr, 0.5), quantile(&verr, 0.9)),
        limit: "median 1169".into(),
        ok: quantile(&perr, 0.5) <= 1169.0,
    });
    rows.push(Row {
        test: format!("player, recorded inputs ({n} windows of 1 s): heading difference median (p90), deg"),
        a: "-".into(),
        b: format!("{:.2} ({:.2})", quantile(&herr, 0.5), quantile(&herr, 0.9)),
        diff: "-".into(),
        limit: "median 2".into(),
        ok: quantile(&herr, 0.5) <= 2.0,
    });
}

#[derive(Default, Clone)]
struct RaceStats {
    /// Per kart slot: lap times (s) of completed laps.
    laps: Vec<Vec<f64>>,
    place: Vec<u8>,
    fires: f64,
    boxes: f64,
    tokens: f64,
    turbo_s: f64,
    special_moves: f64,
    specials: f64,
    hits: f64,
}

fn ai_race(disc: &mut mr_disc::Disc, hz: u8, seed: u32) -> RaceStats {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995;
    x ^= x >> 13;
    let cfg = SessionConfig {
        logic_hz: hz,
        rand_seed: x,
        rng_start: mr_sim::session::RNG_START + (x % 0x5DB),
        ..SessionConfig::dock_default()
    };
    let mut s = session(disc, cfg);
    let n = 1 + s.others.len();
    let mut st = RaceStats { laps: vec![Vec::new(); n], ..Default::default() };
    let per_tick_s = s.tick_seconds();
    let limit = (6000.0 / 30.0 / per_tick_s) as u64;
    for _ in 0..limit {
        let before: Vec<(u8, u8)> = std::iter::once((s.kart.lap, s.kart.finished)).chain(s.others.iter().map(|o| (o.kart.lap, o.kart.finished))).collect();
        if s.tick(0).is_err() {
            break;
        }
        let ks: Vec<&KartState> = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).collect();
        for (i, k) in ks.iter().enumerate() {
            if k.lap != before[i].0 {
                st.laps[i].push(k.lap_time[before[i].0 as usize] as f64 / 300.0);
            }
        }
        // Race window: AI karts until each finishes.
        let racing = |slot: usize| slot > 0 && before[slot].1 == 0;
        for e in &s.items().events {
            match *e {
                ItemEvent::Fired { kart, .. } if racing(kart as usize) => st.fires += 1.0,
                ItemEvent::Pickup { kart, kind: PickupKind::Box, .. } if racing(kart as usize) => st.boxes += 1.0,
                ItemEvent::Pickup { kart, kind: PickupKind::Token, .. } if racing(kart as usize) => st.tokens += 1.0,
                ItemEvent::SpecialMove { kart } if racing(kart as usize) => st.special_moves += 1.0,
                ItemEvent::Special { kart, .. } if racing(kart as usize) => st.specials += 1.0,
                ItemEvent::Hit { shooter, .. } if racing(shooter as usize) => st.hits += 1.0,
                _ => {}
            }
        }
        for (i, k) in ks.iter().enumerate() {
            if racing(i) && k.boosting != 0 && k.special == 0 {
                st.turbo_s += per_tick_s;
            }
        }
        if ks.iter().skip(1).all(|k| k.finished != 0) {
            break;
        }
    }
    st.place = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).map(|k| k.race_pos).collect();
    st
}

fn mean_sd(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let var = if v.len() > 1 { v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0) } else { 0.0 };
    (m, var.sqrt())
}

fn ai(disc: &mut mr_disc::Disc, rows: &mut Vec<Row>, races: u32) {
    let mut a = Vec::new();
    let mut b = Vec::new();
    for r in 0..races {
        a.push(ai_race(disc, 30, 100_001 + r));
        b.push(ai_race(disc, 60, 100_001 + r));
    }
    let n = a[0].laps.len();
    let mut pooled = (0.0, 0.0, 0usize);
    for slot in 1..n {
        let la: Vec<f64> = a.iter().flat_map(|r| r.laps[slot].iter().take(3).copied()).collect();
        let lb: Vec<f64> = b.iter().flat_map(|r| r.laps[slot].iter().take(3).copied()).collect();
        let (ma, _) = mean_sd(&la);
        let (mb, _) = mean_sd(&lb);
        pooled.0 += la.iter().sum::<f64>();
        pooled.1 += lb.iter().sum::<f64>();
        pooled.2 += 1;
        row(rows, &format!("AI kart {slot}: lap mean (s), {} / {} laps", la.len(), lb.len()), ma, mb, 3.0);
        let pa = mean_sd(&a.iter().map(|r| r.place[slot] as f64).collect::<Vec<_>>()).0;
        let pb = mean_sd(&b.iter().map(|r| r.place[slot] as f64).collect::<Vec<_>>()).0;
        row_abs(rows, &format!("AI kart {slot}: mean finishing place"), pa, pb, 1.5, "");
    }
    let la: usize = a.iter().map(|r| r.laps[1..].iter().map(|l| l.len().min(3)).sum::<usize>()).sum();
    let lb: usize = b.iter().map(|r| r.laps[1..].iter().map(|l| l.len().min(3)).sum::<usize>()).sum();
    row(rows, "AI pooled lap mean (s)", pooled.0 / la as f64, pooled.1 / lb as f64, 1.0);
    type Get = fn(&RaceStats) -> f64;
    let stats: [(&str, Get); 7] = [
        ("fires", |r| r.fires),
        ("item boxes", |r| r.boxes),
        ("turbo tokens", |r| r.tokens),
        ("turbo (s)", |r| r.turbo_s),
        ("special moves", |r| r.special_moves),
        ("character specials", |r| r.specials),
        ("hits dealt", |r| r.hits),
    ];
    for (name, f) in stats {
        let va: Vec<f64> = a.iter().map(f).collect();
        let vb: Vec<f64> = b.iter().map(f).collect();
        let (ma, sa) = mean_sd(&va);
        let (mb, sb) = mean_sd(&vb);
        let tol = 2.0 * (sa * sa / va.len() as f64 + sb * sb / vb.len() as f64).sqrt();
        rows.push(Row {
            test: format!("AI items per race: {name}"),
            a: format!("{ma:.1} ({sa:.1})"),
            b: format!("{mb:.1} ({sb:.1})"),
            diff: format!("{:+.2}", mb - ma),
            limit: format!("{tol:.2}"),
            ok: (mb - ma).abs() <= tol.max(1e-9) || (ma == mb),
        });
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let races: u32 = opt("--races").map(|s| s.parse().unwrap()).unwrap_or(6);
    let runs: u32 = opt("--runs").map(|s| s.parse().unwrap()).unwrap_or(6);
    let only = opt("--only");
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mut rows = Vec::new();
    let want = |k: &str| only.as_deref().map(|o| o.split(',').any(|x| x == k)).unwrap_or(true);
    if want("accel") {
        accel(&mut disc, &mut rows);
    }
    if want("jump") {
        jump(&mut disc, &mut rows);
    }
    if want("turn") {
        turn(&mut disc, &mut rows);
    }
    if want("player") {
        player(&mut disc, &mut rows, runs);
    }
    if want("ai") {
        ai(&mut disc, &mut rows, races);
    }
    println!("| test | 30 Hz | 60 Hz | diff | limit | ok |");
    println!("|---|---|---|---|---|---|");
    for r in &rows {
        println!("| {} | {} | {} | {} | {} | {} |", r.test, r.a, r.b, r.diff, r.limit, if r.ok { "yes" } else { "**NO**" });
    }
    let bad = rows.iter().filter(|r| !r.ok).count();
    println!("\n{} of {} within tolerance", rows.len() - bad, rows.len());
}
