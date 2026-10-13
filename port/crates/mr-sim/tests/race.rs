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
    let Ok(mut disc) = mr_disc::Disc::open_default() else {
        eprintln!("skip: no disc");
        return;
    };
    let mut s = mr_sim::session::Session::new(&mut disc).unwrap();
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
    let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
    if !dir.join("meta.json").exists() {
        eprintln!("skip: no capture at {}", dir.display());
        return;
    }
    let cap = Capture::open(&dir).unwrap();
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| key(v).0 == 480 && cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0]).unwrap();
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    let mut s = mr_sim::session::Session::new(&mut disc).unwrap();
    // Random-pointer driven fields and the AI karts' contacts.
    let skip = [
        "copy_button", "aim_x", "aim_y", "aim_z", "aim_dist", "nearest_kart", "race_pos", "shown_pos", "pos_flash", "progress", "course_point",
        "catchup", "blink_timer", "blink", "blink_count", "sway_a", "sway_b", "sway_c", "sway_d", "sway_e", "lean_a", "lean_b", "look_x", "look_y",
        "look_z", "ref_y", "flags", "close_kart", "rumble", "rumble_a", "rumble_b", "event_bits",
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
