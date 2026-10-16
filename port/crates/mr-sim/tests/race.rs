//! Race flow against the oracle's 3-lap DOCK race (analysis/oracle/race3P,
//! route tools/oracle/routes/race3.json): the player's kart over all three
//! laps to the finish, the race-progress / position pass, and the
//! standalone session's countdown and phases. Skips when the capture or
//! the disc is absent.

use mr_sim::harness::Run;
use mr_sim::kart::{KartState, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::race::{self, RacePhase};

fn capture() -> Option<(Capture, Vec<u8>)> {
    let root = oracle::repo_root();
    let dir = root.join("analysis/oracle/race3P");
    if !dir.join("ram0.bin").exists() {
        eprintln!("skip: no capture at {}", dir.display());
        return None;
    }
    std::env::set_current_dir(&root).unwrap();
    let cap = Capture::open(&dir).unwrap();
    let base = cap.ram0.clone().unwrap();
    Some((cap, base))
}

/// The tick the player crosses the line for the third time.
fn finish_tick(run: &Run, cap: &Capture) -> usize {
    (0..run.ticks.len()).find(|&i| cap.read(run.ticks[i], KART_ARRAY + 0x4C7, 1).unwrap()[0] != 0).expect("the capture reaches the finish")
}

#[test]
fn three_laps_to_the_finish() {
    let Some((cap, base)) = capture() else { return };
    let world = oracle::dock_world(&base);
    let run = Run::new(&cap, &base, &world);
    let fin = finish_tick(&run, &cap);
    let rep = run.teacher_forcing(fin);
    assert!(rep.panics.is_empty(), "panics: {:?}", rep.panics);
    // Every external event is another kart's doing: projectile hits (806,
    // 829, 1884, 3179) and the spin effect they start and end (1344, 1493;
    // FUN_8006C388 runs the effect timer).
    assert_eq!(rep.events.keys().copied().collect::<Vec<_>>(), vec![806, 829, 1344, 1493, 1884, 3179]);
    let fr = run.free_run_rng(fin, &rep.events, None);
    assert!(fr.stopped.is_none(), "{:?}", fr.stopped);
    assert_eq!(fr.first_diff, None, "motion");
    let last = &fr.rows.last().unwrap().2;
    let o = oracle::load_kart(&cap.mem_at(run.ticks[fin], Some(&base)), 0);
    // Laps, lap times, best lap, race time, finished, the AI hand-over bit.
    assert_eq!((last.lap, last.finished, last.flags & 1), (3, 1, 1));
    assert_eq!(last.lap_time, o.lap_time);
    assert_eq!((last.best_lap, last.race_time), (o.best_lap, o.race_time));
    // Place: 0x800B0CD8 at the finish (two AI karts were in already).
    // (+0x4FF: 0x10 here; the position pass after the kart loop takes dt
    // off in the same frame.)
    assert_eq!((last.race_pos, last.shown_pos, last.pos_flash as i32 - 2), (o.race_pos, o.shown_pos, o.pos_flash as i32));
    assert_eq!(last.race_pos, 3);
    assert_eq!(last.progress, o.progress);
    let r = race::result(0, last);
    assert_eq!(r.lap_times.len(), 3);
    // The race time is the sum of the lap times before the last one was
    // rounded to a multiple of 3 (the finish does not re-add them).
    assert!((r.lap_times.iter().sum::<i32>() - r.race_time).abs() <= 1);
}

/// FUN_800837F8 / FUN_800834C0 on the oracle's six karts: the served
/// kart's progress, then every kart's position and position flash.
#[test]
fn race_progress_matches_oracle() {
    let Some((cap, base)) = capture() else { return };
    let world = oracle::dock_world(&base);
    let run = Run::new(&cap, &base, &world);
    let fin = finish_tick(&run, &cap);
    let fields = |k: &KartState| (k.progress, k.course_point, k.race_pos, k.shown_pos, k.pos_flash);
    let (mut matched, mut served0, mut changed) = (0, 0, 0);
    for i in 0..fin {
        let ma = cap.mem_at(run.ticks[i], Some(&base));
        let mb = cap.mem_at(run.ticks[i + 1], Some(&base));
        let n = mb.i16(0x800B_0DFE) as u32;
        let before: Vec<KartState> = (0..n).map(|s| oracle::load_kart(&ma, s)).collect();
        let after: Vec<KartState> = (0..n).map(|s| oracle::load_kart(&mb, s)).collect();
        let mut g = mr_sim::globals::RaceGlobals::read(&mb);
        g.dt = mb.i32(0x800B_0AEC);
        let want: Vec<_> = after.iter().map(fields).collect();
        if want != before.iter().map(fields).collect::<Vec<_>>() {
            changed += 1;
        }
        let mut ok = None;
        for pick in 0..n as usize {
            let mut ks = after.clone();
            for (s, k) in ks.iter_mut().enumerate() {
                if k.finished != 0 && before[s].finished == 0 {
                    // Finished in this frame's kart loop: place, shown place
                    // and a 0x10 flash were set there (race_finished).
                    k.pos_flash = 0x10;
                    continue;
                }
                k.race_pos = before[s].race_pos;
                k.shown_pos = before[s].shown_pos;
                k.pos_flash = before[s].pos_flash;
                if s == pick {
                    k.progress = before[s].progress;
                    k.course_point = before[s].course_point;
                }
            }
            let mut refs: Vec<&mut KartState> = ks.iter_mut().collect();
            race::race_progress(&g, &world, &mut refs, pick);
            if ks.iter().map(fields).collect::<Vec<_>>() == want {
                ok = Some(pick);
                break;
            }
        }
        match ok {
            Some(p) => {
                matched += 1;
                if p == 0 {
                    served0 += 1;
                }
            }
            None => {
                let b: Vec<_> = before.iter().map(fields).collect();
                panic!("tick {i}: no kart served reproduces the oracle's progress / positions
before {b:?}
want   {want:?}
laps {:?} clock {}", after.iter().map(|k| (k.lap, k.finished)).collect::<Vec<_>>(), g.race_clock)
            }
        }
    }
    eprintln!("race progress: {matched}/{fin} frames reproduced ({changed} with changes, kart 0 served on {served0})");
    assert!(served0 > fin / 8);
    let _ = RECORD_SIZE;
}

/// The standalone session from the true race start: intro, start game, GO,
/// and the oracle savestate's race time: the load frame (countdown held at
/// 480), 240 countdown ticks to GO, then 142 race ticks.
#[test]
fn session_countdown_to_go() {
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    let mut s = mr_sim::session::Session::solo(&mut disc).unwrap();
    assert_eq!(s.phase(), RacePhase::Intro { countdown: 480 });
    let mut seen = Vec::new();
    for _ in 0..241 {
        s.tick(0).unwrap();
        let p = std::mem::discriminant(&s.phase());
        if seen.last() != Some(&p) {
            seen.push(p);
        }
    }
    assert_eq!(s.countdown(), 0);
    assert_eq!(s.race_clock(), 0);
    assert!(matches!(s.phase(), RacePhase::Racing { lap: 1, laps: 3 }));
    assert_eq!(seen.len(), 3, "intro -> start game -> racing");
    s.idle_until_race_time(1430).unwrap();
    assert_eq!(s.ticks, 1 + 240 + 142);
    assert_eq!(s.kart().race_time, 1430);
    assert_eq!(s.position(), 1);
    let r = s.results();
    assert_eq!((r.len(), r[0].slot, r[0].finished), (1, 0, false));
}

/// The standalone session from race load against the oracle's run from the
/// course menu (analysis/oracle/start_game, route
/// tools/oracle/routes/start_game.json): countdown, the "Copy the buttons"
/// start game with face buttons pressed in turn, GO and the start boost.
/// Per frame the oracle state is the frame's last VBlank. The next button
/// to copy is drawn from the random pointer the AI karts share, so it is
/// taken from the oracle; everything else must match through GO. (The
/// frame after, the path position differs: the game serves FUN_8009004C one
/// kart per frame round six karts, the session its one kart every frame.)
#[test]
fn start_game_matches_oracle() {
    let root = oracle::repo_root();
    let dir = root.join("analysis/oracle/start_game");
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    if !dir.join("meta.json").exists() {
        eprintln!("skip: no capture at {}", dir.display());
        return;
    }
    let cap = Capture::open(&dir).unwrap();
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| key(v).0 == 480 && cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0]).unwrap();
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    let mut s = mr_sim::session::Session::solo(&mut disc).unwrap();
    // Random-pointer driven fields and the AI karts' contacts.
    let skip = [
        "copy_button", "aim_x", "aim_y", "aim_z", "aim_dist", "nearest_kart", "race_pos", "shown_pos", "pos_flash", "progress", "course_point",
        "catchup", "blink_timer", "blink", "blink_count", "sway_a", "sway_b", "sway_c", "sway_d", "sway_e", "lean_a", "lean_b", "look_x", "look_y",
        "look_z", "ref_y", "flags", "close_kart", "rumble", "rumble_a", "rumble_b", "event_bits",
        // Model slot numbers (the game's model table order) and the colour
        // of the last token an earlier race left in the record.
        "model_slot", "char_slot", "token_colour", "drv_x", "drv_z",
        // Their copies from the previous render pass (named since Session 6).
        "drv_prev_x", "drv_prev_y", "drv_prev_z",
    ];
    let mut presses = 0;
    let mut go = None;
    for (j, &vb) in ends.iter().enumerate() {
        let m = cap.mem_at(vb, None);
        s.kart.copy_button = cap.read(if j == 0 { load } else { ends[j - 1] }, KART_ARRAY + 0x4DA, 1).unwrap()[0] as i8;
        let btn = m.u16(0x800B_0CE8);
        if btn & 0xF000 != 0 && s.countdown() > 0 && s.countdown() < 300 {
            presses += 1;
        }
        s.tick_dt(btn, m.i32(0x800B_0AEC)).unwrap();
        let o = cap.read(vb, KART_ARRAY, RECORD_SIZE).unwrap();
        let r = s.record();
        let bad: Vec<&str> = mr_sim::kart::FIELD_OFFSETS.iter().filter(|&&(n, off, sz)| r[off..off + sz] != o[off..off + sz] && !skip.contains(&n)).map(|e| e.0).collect();
        assert!(bad.is_empty(), "frame {j}: {bad:?}");
        assert_eq!((s.countdown(), s.race_clock(), s.best_start), (m.i32(0x800B_0C70), m.i32(0x800B_0A94), m.u8(0x800E_02BA)), "frame {j}");
        if s.countdown() == 0 && go.is_none() {
            go = Some(j);
        }
        if go.is_some() {
            break;
        }
    }
    eprintln!("start game: GO at frame {go:?}, {presses} frames with a face button, best start {}", s.best_start);
    // Load frame + 239 frames (one of them dt 3) = 480 VBlanks.
    assert_eq!(go, Some(239));
    assert!(s.best_start > 0);
}

/// The six-kart session from race load against the oracle's load from the
/// course menu (analysis/oracle/start_probe, no input): with the BIOS seed
/// and random pointer the oracle had before the load (0x832B2260,
/// 0x8001FC4E) and its water-animation state, the race load picks the same
/// opponents and grid and builds the same six records, and every kart's
/// motion stays exact through the countdown, GO and the first race ticks
/// (to frame 372; at 373 an oracle AI kart fires a turbo: items are on in
/// that capture, off in the session).
#[test]
fn six_kart_race_load_matches_oracle() {
    let root = oracle::repo_root();
    let dir = root.join("analysis/oracle/start_probe");
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    if !dir.join("meta.json").exists() {
        eprintln!("skip: no capture at {}", dir.display());
        return;
    }
    let cap = Capture::open(&dir).unwrap();
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| key(v).0 == 480 && cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0]).unwrap();
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    let mut cfg = mr_sim::session::SessionConfig::dock_default();
    cfg.rand_seed = 0x832B_2260;
    cfg.rng_start = 0x8001_FC4E;
    let mut s = mr_sim::session::Session::with_config(&mut disc, cfg).unwrap();
    s.water = mr_sim::water::WaterAnim::from_bytes(cap.read(load, 0x800A_FEF0, 0x20).unwrap());
    let m = cap.mem_at(load, None);
    assert_eq!((s.globals.rng, s.rand_seed()), (m.u32(0x800B_0BCC), m.u32(0x8000_9010)), "random state after the load");
    // Load: every named field of every kart (the session's records also
    // carry a render pass for drawing; the game's look point is still 0).
    for (slot, k) in std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).enumerate() {
        let o = KartState::from_record(cap.read(load, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap());
        let d: Vec<_> = k
            .diff(&o)
            .into_iter()
            .filter(|d| !d.starts_with("trail") && !d.starts_with("model_slot") && !d.starts_with("char_slot") && !d.starts_with("token_colour"))
            .collect();
        assert!(d.is_empty(), "kart {slot} at load: {d:?}");
    }
    let motion = ["x", "y", "z", "pitch", "heading", "roll", "vx", "vy", "vz", "fwd_speed", "side_speed", "lap", "buttons", "race_pos"];
    for (j, &vb) in ends.iter().enumerate().take(372) {
        let m = cap.mem_at(vb, None);
        s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)).unwrap();
        assert_eq!((s.countdown(), s.race_clock()), (m.i32(0x800B_0C70), m.i32(0x800B_0A94)), "frame {j}");
        for (slot, _, _, r) in s.kart_records() {
            // Items on (Session 4): the AI's item buttons count too.
            let o = cap.read(vb, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap().to_vec();
            let bad: Vec<&str> = mr_sim::kart::FIELD_OFFSETS.iter().filter(|&&(n, off, sz)| motion.contains(&n) && r[off..off + sz] != o[off..off + sz]).map(|e| e.0).collect();
            assert!(bad.is_empty(), "frame {j} kart {slot}: {bad:?}");
        }
    }
}

/// A standalone race from load to the finish: the five opponents run the
/// three laps (the player idles on the grid), every AI kart finishes, the
/// results rank them by finishing order.
#[test]
fn standalone_six_kart_race_finishes() {
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    let mut s = mr_sim::session::Session::new(&mut disc).unwrap();
    assert_eq!(s.others.len(), 5);
    let mut t = 0;
    while !s.others.iter().all(|o| o.kart.finished != 0) {
        s.tick(0).unwrap();
        t += 1;
        assert!(t < 8000, "AI karts did not finish");
    }
    let r = s.results();
    for (i, x) in r.iter().take(5).enumerate() {
        assert_eq!((x.place as usize, x.finished, x.lap_times.len()), (i + 1, true, 3), "{x:?}");
        assert!(x.slot != 0);
        // Items on: turbo and specials make the AI's laps faster and more
        // spread out (oracle items-on laps ~12,000, sd up to 1,800).
        assert!(x.lap_times.iter().all(|&l| (8_000..17_000).contains(&l)), "{x:?}");
    }
    assert_eq!((r[5].slot, r[5].finished), (0, false));
    eprintln!("standalone race: {t} ticks, {:?}", r.iter().map(|x| (x.slot, x.race_time)).collect::<Vec<_>>());
}

/// The player's kart after the finish is driven by the AI as in the game:
/// on autopilot the player finishes, the race goes to the results (mode 10)
/// and the kart keeps moving under FUN_800912F8.
#[test]
fn finished_player_is_driven_by_the_ai() {
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    let mut s = mr_sim::session::Session::new(&mut disc).unwrap();
    s.autopilot = true;
    let mut t = 0;
    while s.kart.finished == 0 {
        s.tick(0).unwrap();
        t += 1;
        assert!(t < 8000, "the player did not finish");
    }
    assert_eq!(s.kart.flags & 1, 1);
    assert!(matches!(s.phase(), RacePhase::Finished { .. }));
    let p0 = (s.kart.x, s.kart.z);
    for _ in 0..300 {
        s.tick(0).unwrap();
    }
    assert_eq!(s.globals.mode, 10);
    let d = ((s.kart.x - p0.0) as i64).abs() + ((s.kart.z - p0.1) as i64).abs();
    assert!(d > 100_000, "the finished kart did not drive on ({d})");
}

/// The 10-minute limit (0x80067F64: race clock 0x800B0A94 > 179999 with
/// laps to go): every kart still racing is out (+0x4C7 = 2), driven by the
/// AI, and the race goes to the results in the same frame's simulation
/// (oracle: analysis/oracle/tri/DOCK_TIMEUP, `mr-course-check --dir`).
#[test]
fn ten_minute_limit_ends_the_race() {
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    for hz in [30u8, 60] {
        let mut s = mr_sim::session::Session::new(&mut disc).unwrap();
        s.set_logic_hz(hz);
        while s.countdown() != 0 {
            s.tick(0).unwrap();
        }
        s.globals.race_clock = 179_000;
        let mut t = 0;
        while s.globals.mode != 10 {
            s.tick(0).unwrap();
            t += 1;
            assert!(t < 400, "{hz} Hz: no time-up");
        }
        assert!(s.globals.race_clock > 179_999);
        assert_eq!((s.kart.finished, s.kart.flags & 1, s.kart.pause_bits), (2, 1, 0x80));
        assert!(s.others.iter().all(|o| o.kart.finished == 2 && o.kart.flags & 1 == 1));
        let p0 = (s.kart.x, s.kart.z);
        for _ in 0..200 {
            s.tick(0).unwrap();
        }
        assert_ne!((s.kart.x, s.kart.z), p0, "{hz} Hz: the AI drives the kart on");
    }
}
