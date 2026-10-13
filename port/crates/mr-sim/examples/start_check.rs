//! The standalone Session from race load against an oracle capture that
//! starts in the menus (slot 1): countdown, start game, GO and the first
//! race ticks. Per Session tick j the oracle state is the last VBlank of
//! that frame (the last VBlank before the countdown / race clock moves on).
//!
//!   cargo run --release -p mr-sim --example start_check -- analysis/oracle/start_probe [TICKS]
use mr_sim::kart::{FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::session::Session;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cap = Capture::open(&a[1]).unwrap();
    let max: usize = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(400);
    let base = cap.ram0.clone();
    // Frame ends: (countdown, race clock) changes at the start of a frame.
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| key(v).0 == 480 && cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0]).expect("race load in the capture");
    let mut ends = Vec::new();
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    for w in vbs.windows(2) {
        if key(w[0]) != key(w[1]) {
            ends.push(w[0]);
        }
    }
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mut s = Session::new(&mut disc).unwrap();
    let skip: &[&str] = &["copy_button", "aim_x", "aim_y", "aim_z", "aim_dist", "nearest_kart", "race_pos", "shown_pos", "pos_flash", "progress", "course_point", "catchup",
        // Random-pointer driven (the AI karts' inits and updates share the
        // pointer) and AI contacts:
        "blink_timer", "blink", "blink_count", "sway_a", "sway_b", "sway_c", "sway_d", "sway_e", "lean_a", "lean_b", "look_x", "look_y", "look_z", "ref_y",
        "flags", "close_kart", "rumble", "rumble_a", "rumble_b", "event_bits"];
    let random_only = std::env::args().any(|a| a == "--strict");
    let show = |label: &str, s: &Session, vb: u32| -> usize {
        let o = cap.read(vb, KART_ARRAY, RECORD_SIZE).unwrap();
        let r = s.record();
        let bad: Vec<String> = FIELD_OFFSETS
            .iter()
            .filter(|&&(n, off, sz)| r[off..off + sz] != o[off..off + sz] && !skip.contains(&n))
            .map(|&(n, off, sz)| format!("{n} {} vs {}", mr_sim::harness::val(&r[off..off + sz]), mr_sim::harness::val(&o[off..off + sz])))
            .collect();
        let m = cap.mem_at(vb, base.as_deref());
        let g = (s.globals.countdown, s.globals.race_clock, s.best_start);
        let og = (m.i32(0x800B_0C70), m.i32(0x800B_0A94), m.u8(0x800E_02BA));
        if !bad.is_empty() || g != og {
            println!("{label} vb {vb}: globals {g:?} vs {og:?}; {} fields differ: {}", bad.len(), bad.join(", "));
        }
        bad.len() + (g != og) as usize
    };
    // The start game's first button is drawn at kart init from the random
    // pointer, which the other karts' inits advance first: take the
    // oracle's.
    s.kart.copy_button = cap.read(load, KART_ARRAY + 0x4DA, 1).unwrap()[0] as i8;
    s.kart.to_record(&mut s.record);
    show("load", &s, load);
    let mut first = None;
    for (j, &vb) in ends.iter().enumerate().take(max) {
        let m = cap.mem_at(vb, base.as_deref());
        let btn = m.u16(0x800B_0CE8);
        // 0x800B0AEC: this frame's dt (measured after FUN_8005C2EC).
        let dt = m.i32(0x800B_0AEC);
        // The next button is drawn from the random pointer the AI karts
        // share: take the oracle's button shown before this frame.
        s.kart.copy_button = cap.read(if j == 0 { load } else { ends[j - 1] }, KART_ARRAY + 0x4DA, 1).unwrap()[0] as i8;
        if let Err(e) = s.tick_dt(btn, dt) {
            println!("tick {j}: {e}");
            break;
        }
        let n = show(&format!("tick {j} btn {btn:#06x} dt {dt}"), &s, vb);
        if n > 0 && first.is_none() {
            first = Some(j);
        }
        if first.is_some_and(|f| j > f + 3) && !random_only {
            break;
        }
    }
    println!("first differing tick: {first:?} (of {} frame ends)", ends.len());
}
