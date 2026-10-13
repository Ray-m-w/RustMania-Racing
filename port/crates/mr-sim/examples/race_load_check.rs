//! The standalone 6-kart session from race load against an oracle capture
//! that starts in the menus (slot 1): the race load (opponents, grids,
//! records, random pointer, rand() seed) and then every frame, every kart.
//! The session gets the BIOS seed and random pointer the oracle had before
//! the load; buttons and dt per frame from the capture.
//!
//!   cargo run --release -p mr-sim --example race_load_check -- analysis/oracle/start_probe SEED RNG [FRAMES]
use mr_sim::kart::{FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::session::{Session, SessionConfig};

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cap = Capture::open(&a[1]).unwrap();
    let mut cfg = SessionConfig::dock_default();
    cfg.rand_seed = hex(&a[2]);
    cfg.rng_start = hex(&a[3]);
    if let Some(i) = a.iter().position(|x| x == "--rr") {
        cfg.round_robin_start = a[i + 1].parse().unwrap();
    }
    let max: usize = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(300);
    let verbose = std::env::args().any(|x| x == "-v");
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| key(v).0 == 480 && cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0]).unwrap();
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mut s = Session::with_config(&mut disc, cfg).unwrap();
    // Fields other code writes that the session does not model.
    let motion_only = std::env::args().any(|x| x == "--motion");
    let motion: &[&str] = &["x", "y", "z", "pitch", "heading", "roll", "vx", "vy", "vz", "fwd_speed", "side_speed", "lap", "buttons", "race_pos"];
    let compare = |label: &str, s: &Session, vb: u32| -> usize {
        let m = cap.mem_at(vb, None);
        let mut bad = 0;
        let mut lines = Vec::new();
        for (slot, _, _, r) in s.kart_records() {
            let mut o = cap.read(vb, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap().to_vec();
            // Items are off in the session: the oracle's item buttons
            // (L1 / L2 / R1 / R2) are masked for the comparison.
            let b = u16::from_le_bytes([o[0x44E], o[0x44F]]) & !mr_sim::ai::btn::ITEMS;
            o[0x44E..0x450].copy_from_slice(&b.to_le_bytes());
            let d: Vec<String> = FIELD_OFFSETS
                .iter()
                .filter(|&&(n, off, sz)| r[off..off + sz] != o[off..off + sz] && (!motion_only || motion.contains(&n)))
                .map(|&(n, off, sz)| format!("{n} {}/{}", mr_sim::harness::val(&r[off..off + sz]), mr_sim::harness::val(&o[off..off + sz])))
                .collect();
            bad += d.len();
            if !d.is_empty() {
                lines.push(format!("    kart {slot}: {}", d.join(", ")));
            }
        }
        let g = (s.globals.countdown, s.globals.race_clock, if motion_only { 0 } else { s.globals.rng }, if motion_only { 0 } else { s.rand_seed() });
        let og = (m.i32(0x800B_0C70), m.i32(0x800B_0A94), if motion_only { 0 } else { m.u32(0x800B_0BCC) }, if motion_only { 0 } else { m.u32(0x8000_9010) });
        if g != og {
            bad += 1;
        }
        if bad > 0 {
            println!("{label} vb {vb}: (countdown, clock, rng, seed) {g:X?} vs {og:X?}; {bad} differences");
            if verbose {
                for l in lines {
                    println!("{l}");
                }
            }
        }
        bad
    };
    if std::env::args().any(|x| x == "--oracle-water") {
        // The water animation runs on between races; take the oracle's state.
        s.water = mr_sim::water::WaterAnim::from_bytes(cap.read(load, 0x800A_FEF0, 0x20).unwrap());
    }
    compare("load", &s, load);
    let mut first = None;
    for (j, &vb) in ends.iter().enumerate().take(max) {
        let m = cap.mem_at(vb, None);
        if let Err(e) = s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)) {
            println!("frame {j}: {e}");
            break;
        }
        let n = compare(&format!("frame {j}"), &s, vb);
        if n > 0 && first.is_none() {
            first = Some(j);
        }
        if first.is_some_and(|f| j > f + 2) {
            break;
        }
    }
    println!("first differing frame: {first:?}");
}
