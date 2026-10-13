//! Session 5 G: the 60 Hz logic mode and the snapshot API (no oracle data).
//! The 30 Hz path itself is proven unchanged by the oracle tests in this
//! directory and by examples/hz30_fingerprint.rs (whole-state hashes of
//! long races before / after); the feel tolerance is examples/feel60.rs.

use mr_sim::session::{Session, SessionConfig};

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

fn records(s: &Session) -> Vec<u8> {
    s.kart_records().flat_map(|(_, _, _, r)| r.to_vec()).collect()
}

/// A tick run on a snapshot and thrown away leaves no trace: the next real
/// tick is the same as without the look-ahead (mr-bevy's display relies on
/// this to keep the faithful simulation untouched).
#[test]
fn snapshot_restore_leaves_the_session_unchanged() {
    let mut d = disc();
    let mut a = Session::new(&mut d).unwrap();
    let mut b = Session::new(&mut d).unwrap();
    a.autopilot = true;
    b.autopilot = true;
    for t in 0..900u32 {
        let snap = b.snapshot();
        b.tick(0x4080).unwrap();
        b.restore(&snap);
        a.tick(0).unwrap();
        b.tick(0).unwrap();
        assert_eq!(records(&a), records(&b), "tick {t}");
        assert_eq!(a.rand_seed(), b.rand_seed(), "tick {t}");
        assert_eq!(a.globals, b.globals, "tick {t}");
    }
}

/// Two 60 Hz ticks are one 30 Hz tick of game time: the countdown, GO and
/// the race clock (VBlank units) land on the same game time; dt is 1.
#[test]
fn hz60_keeps_game_time() {
    let mut d = disc();
    let mut s30 = Session::with_config(&mut d, SessionConfig::dock_solo()).unwrap();
    let mut s60 = Session::with_config(&mut d, SessionConfig { logic_hz: 60, ..SessionConfig::dock_solo() }).unwrap();
    assert_eq!((s30.logic_hz(), s60.logic_hz()), (30, 60));
    assert_eq!((s30.vblanks_per_tick(), s60.vblanks_per_tick()), (2, 1));
    for k in 0..400 {
        s30.tick(0x4000).unwrap();
        s60.tick(0x4000).unwrap();
        s60.tick(0x4000).unwrap();
        assert_eq!(s60.globals.dt, 1);
        assert_eq!(s30.countdown(), s60.countdown(), "pair {k}: countdown");
        assert_eq!(s30.race_clock(), s60.race_clock(), "pair {k}: race clock");
        assert_eq!(s30.globals.frame, s60.globals.frame, "pair {k}: VBlank counter");
    }
}

/// The 60 Hz mode drives whole races: the game's AI on the player's kart
/// (items off), 3 laps, 4 seeds; the mean race time within 3 % of the same
/// races at 30 Hz (one race alone varies by +-4 % with a wall hit or not;
/// feel60 measures 40 seeds: +0.1 %).
#[test]
fn hz60_ai_races_finish_near_the_30hz_time() {
    let mut d = disc();
    let mut t = [0f64; 2];
    for seed in 1..=4u32 {
        for (i, hz) in [30u8, 60].into_iter().enumerate() {
            let mut s = Session::with_config(&mut d, SessionConfig { logic_hz: hz, items: false, rand_seed: seed, ..SessionConfig::dock_solo() }).unwrap();
            s.autopilot = true;
            while s.kart.finished == 0 && s.ticks < 20_000 {
                s.tick(0).unwrap();
            }
            assert_ne!(s.kart.finished, 0, "seed {seed}: {hz} Hz race did not finish");
            t[i] += s.kart.lap_time[..3].iter().sum::<i32>() as f64 / 300.0 / 4.0;
        }
    }
    let d = (t[1] / t[0] - 1.0) * 100.0;
    eprintln!("mean race time 30 Hz {:.2} s, 60 Hz {:.2} s ({d:+.2} %)", t[0], t[1]);
    assert!(d.abs() < 3.0, "race time {d:+.2} %");
}

/// Switching the rate waits for a pair boundary.
#[test]
fn logic_hz_switches_on_a_pair_boundary() {
    let mut d = disc();
    let mut s = Session::with_config(&mut d, SessionConfig { logic_hz: 60, ..SessionConfig::dock_solo() }).unwrap();
    s.tick(0).unwrap();
    assert!(!s.set_logic_hz(30), "mid-pair");
    s.tick(0).unwrap();
    assert!(s.set_logic_hz(30));
    assert_eq!(s.logic_hz(), 30);
}
