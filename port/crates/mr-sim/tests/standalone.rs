//! Checks for the standalone session's own producers (no oracle data at run
//! time): the water animation against the game's code (fixtures from
//! tools/oracle/fx_water.py), the path position against the oracle lap,
//! and Session::new's kart against the oracle's grid state.

use mr_sim::harness::val;
use mr_sim::kart::{FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::session::Session;
use mr_sim::water::WaterAnim;

fn root() -> std::path::PathBuf {
    let r = oracle::repo_root();
    std::env::set_current_dir(&r).unwrap();
    r
}

#[test]
fn water_animation_matches_game_code() {
    let r = root();
    let Ok(b) = std::fs::read(r.join("analysis/oracle/fx_water.bin")) else {
        eprintln!("skip: no analysis/oracle/fx_water.bin (tools/oracle/fx_water.py)");
        return;
    };
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let w = mr_sim::World::dock(&mut disc).unwrap();
    let rec = 8 + 0x1C + 4 + 0x1C + 12;
    let mut n = 0;
    for c in b.chunks_exact(rec) {
        let u = |o: usize| u32::from_le_bytes(c[o..o + 4].try_into().unwrap());
        let (dt, rng_in) = (u(0) as i32, u(4));
        let mut st = WaterAnim::from_bytes(&c[8..8 + 0x1C]);
        let rng_out = u(8 + 0x1C);
        let want = WaterAnim::from_bytes(&c[12 + 0x1C..12 + 0x38]);
        let nrm: Vec<i16> = (0..6).map(|i| i16::from_le_bytes([c[12 + 0x38 + 2 * i], c[13 + 0x38 + 2 * i]])).collect();
        let mut rng = rng_in;
        let (wn, an) = st.step(&w.tables, dt, &mut rng, &|a| w.rng_byte(a));
        assert_eq!(rng, rng_out, "record {n}: random pointer");
        assert_eq!(st.alt, want.alt, "record {n}: alt angles");
        assert_eq!(st.water, want.water, "record {n}: water angles");
        assert_eq!(&wn[..], &nrm[..3], "record {n}: water normal");
        assert_eq!(&an[..], &nrm[3..], "record {n}: alt normal");
        n += 1;
    }
    eprintln!("water: {n} records exact");
    assert!(n > 100);
}

#[test]
fn path_position_matches_oracle() {
    let r = root();
    let dir = r.join("analysis/oracle/lap1P3");
    if !dir.join("ram0.bin").exists() {
        eprintln!("skip: no capture");
        return;
    }
    let cap = Capture::open(&dir).unwrap();
    let base = cap.ram0.clone().unwrap();
    let w = oracle::dock_world(&base);
    let run = mr_sim::harness::Run::new(&cap, &base, &w);
    let (mut checked, mut ok) = (0, 0);
    for i in 0..run.n() {
        let (va, vb) = (run.ticks[i], run.ticks[i + 1]);
        // 0x800C3058[n] picks the kart FUN_8009004C updates this frame.
        let pick = cap.read(vb, 0x800C_3058 + 6, 1).unwrap()[0];
        if pick != 0 {
            continue;
        }
        let m = cap.mem_at(va, Some(&base));
        let mut k = oracle::load_kart(&m, 0);
        mr_sim::progress::path_position(&mut k, &w);
        let refr = cap.read(vb, KART_ARRAY, RECORD_SIZE).unwrap();
        let mut rec = refr.to_vec();
        k.to_record(&mut rec);
        checked += 1;
        let same = [(0x498, 2), (0x49A, 2), (0x3F4, 4), (0x3F8, 4), (0x3FC, 4), (0x500, 1), (0x48C, 2)]
            .iter()
            .all(|&(o, n)| rec[o..o + n] == refr[o..o + n]);
        if same {
            ok += 1;
        } else if checked - ok < 5 {
            eprintln!("tick {i}: path differs");
        }
    }
    eprintln!("path_position: {ok}/{checked} frames exact");
    assert!(checked > 100);
    assert_eq!(ok, checked);
}

#[test]
fn session_grid_state_matches_oracle() {
    let r = root();
    let dir = r.join("analysis/oracle/lap1P3");
    if !dir.join("meta.json").exists() {
        eprintln!("skip: no capture");
        return;
    }
    let cap = Capture::open(&dir).unwrap();
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mut s = Session::solo(&mut disc).unwrap();
    let vb0 = cap.vbs[0];
    let orec = cap.read(vb0, KART_ARRAY, RECORD_SIZE).unwrap();
    // The savestate was taken 143 ticks into the race; idle to the same clock.
    let target = cap.i32(vb0, KART_ARRAY + 0x358).unwrap();
    while s.kart.race_time < target {
        s.tick(0).unwrap();
    }
    // Fields that differ, with the reason (docs/spec/kart-physics-ledger.md).
    let explained: &[(&str, &str)] = &[
        ("aim_x", "look-at target: other karts' look points in the oracle race"),
        ("aim_y", "look-at target: other karts"),
        ("aim_z", "look-at target: other karts"),
        ("aim_dist", "look-at target: other karts"),
        ("look_x", "look point follows the body sway"),
        ("look_y", "look point follows the body sway"),
        ("look_z", "look point follows the body sway"),
        ("ref_y", "driver position follows the body lean"),
        ("lean_a", "body sway (random kicks, look-at target)"),
        ("lean_b", "body sway follows the look-at target"),
        ("sway_a", "body sway"),
        ("sway_b", "body sway"),
        ("sway_c", "body sway"),
        ("sway_d", "body sway"),
        ("sway_e", "body sway"),
        ("blink_timer", "random draws (other karts share the stream in the oracle)"),
        ("blink_count", "random draws"),
        ("blink", "random draws"),
        ("copy_button", "random draws"),
        ("flags", "bit 1 set by code outside the port"),
        ("start_hold", "start-light minigame (countdown) not run"),
        ("copy_hold", "start-light minigame (countdown) not run"),
        ("catchup", "player leads (no AI): catch-up 0 instead of 180"),
        ("nearest_kart", "no other karts"),
        ("race_pos", "no AI karts: the player ranks 1st of 1 (oracle: 6th of 6)"),
        ("shown_pos", "no AI karts: the player ranks 1st of 1 (oracle: 6th of 6)"),
    ];
    let mut bad = Vec::new();
    for &(name, off, sz) in FIELD_OFFSETS {
        if s.record[off..off + sz] != orec[off..off + sz] && !explained.iter().any(|e| e.0 == name) {
            bad.push(format!("{name}@{off:03X} {} vs {}", val(&s.record[off..off + sz]), val(&orec[off..off + sz])));
        }
    }
    assert!(bad.is_empty(), "unexplained differences: {bad:?}");
    assert_eq!(s.record[0x18C..0x19E], orec[0x18C..0x19E], "body matrix");
}

/// The standalone session with the oracle race's catch-up (player 6th), its
/// dt per tick, its random pointer per frame and its outside events
/// (weapon pickups, the AI hit) supplied: every motion field, the body
/// matrix and every render-side range part placement reads match the oracle
/// until an AI kart becomes the look-at target (tick 69; no AI here).
#[test]
fn standalone_render_fields_match_oracle() {
    let r = root();
    let dir = r.join("analysis/oracle/lap1P3");
    if !dir.join("ram0.bin").exists() {
        eprintln!("skip: no capture");
        return;
    }
    let cap = Capture::open(&dir).unwrap();
    let base = cap.ram0.clone().unwrap();
    let w = oracle::dock_world(&base);
    let run = mr_sim::harness::Run::new(&cap, &base, &w);
    let n = 200;
    let rep = run.teacher_forcing(n);
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let mut s = Session::solo(&mut disc).unwrap();
    s.leader_progress_override = Some(1000);
    let r0 = cap.read(run.ticks[0], KART_ARRAY, RECORD_SIZE).unwrap();
    while s.kart.race_time < cap.i32(run.ticks[0], KART_ARRAY + 0x358).unwrap() {
        s.tick(0).unwrap();
    }
    // The oracle's player runs 6th of 6 (catch-up 180 by now).
    s.kart.catchup = 180;
    s.inject(r0, &[(0x388, 4), (0x488, 2), (0x4BE, 1), (0x4CA, 1), (0x4FF, 1)]);
    s.inject(r0, &[(0x14, 0x2C * 4), (0x11C, 0x30), (0x2C0, 0x10), (0x324, 0x24), (0x3A0, 0x48), (0x44C, 2), (0x450, 0xE), (0x4B0, 1), (0x4D0, 6), (0x4DD, 1), (0x4F0, 1)]);
    let mut first = None;
    for i in 0..n {
        let vb = run.ticks[i + 1];
        let m = cap.mem_at(vb, Some(&base));
        s.rng_override = rep.rng_at_frame.get(&i).copied();
        s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)).unwrap();
        let refr = cap.read(vb, KART_ARRAY, RECORD_SIZE).unwrap();
        // Race progress and positions: in the oracle's six-kart race
        // FUN_800837F8 serves the player every sixth frame only.
        s.inject(refr, &[(0x388, 4), (0x488, 2), (0x4BE, 1), (0x4CA, 1), (0x4FF, 1)]);
        if let Some(f) = rep.render_events.get(&i) {
            s.inject(refr, f);
        }
        if std::env::var("MR_DIFF").is_ok() {
            let d: Vec<String> = (0..RECORD_SIZE).filter(|&o| s.record()[o] != refr[o]).map(|o| format!("{o:03X}:{}/{}", s.record()[o], refr[o])).collect();
            eprintln!("tick {}: {}", i + 1, d.join(" "));
        }
        let motion = s.record()[..0x12] == refr[..0x12] && s.record()[0x2F4..0x300] == refr[0x2F4..0x300];
        let render = mr_sim::harness::RENDER_RANGES.iter().all(|&(_, o, k)| s.record()[o..o + k] == refr[o..o + k]);
        if !(motion && render) {
            for &(name, o, k) in mr_sim::harness::RENDER_RANGES {
                if s.record()[o..o + k] != refr[o..o + k] {
                    eprintln!("  {name}: {:?} vs {:?}", &s.record()[o..o + k], &refr[o..o + k]);
                }
            }
            eprintln!("  motion {:?} vs {:?}", (&s.record()[..0x12], &s.record()[0x2F4..0x300]), (&refr[..0x12], &refr[0x2F4..0x300]));
            first = Some(i + 1);
            break;
        }
    }
    eprintln!("standalone render: first difference at tick {first:?}");
    assert!(first.map_or(true, |t| t >= 69), "{first:?}");
}
