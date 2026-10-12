//! The whole DOCK lap against the oracle (analysis/oracle/lap1P3, route
//! tools/oracle/routes/lap1.json): teacher forcing on every tick and the
//! free run. Skips when the capture or the disc is absent.

use mr_sim::harness::{Run, EXTERNAL, NOT_COMPARABLE, PRE_EXTERNAL};
use mr_sim::oracle::{self, Capture};

#[test]
fn lap1_teacher_forcing_and_free_run() {
    let root = oracle::repo_root();
    let dir = root.join("analysis/oracle/lap1P3");
    if !dir.join("ram0.bin").exists() {
        eprintln!("skip: no capture at {}", dir.display());
        return;
    }
    std::env::set_current_dir(&root).unwrap();
    let cap = Capture::open(&dir).unwrap();
    let base = cap.ram0.clone().unwrap();
    let world = oracle::dock_world(&base);
    let run = Run::new(&cap, &base, &world);
    let n = run.n();
    assert!(n > 1500, "expected a full lap, got {n} ticks");

    let rep = run.teacher_forcing(n);
    assert!(rep.panics.is_empty(), "panics: {:?}", rep.panics);
    // The only tick the port cannot explain is the AI projectile hit
    // (FUN_80069648, out of scope) at tick 821.
    assert_eq!(rep.events.keys().copied().collect::<Vec<_>>(), vec![821]);
    for (name, st) in &rep.fields {
        let excused = EXTERNAL.iter().chain(PRE_EXTERNAL).any(|e| e.0 == name) || NOT_COMPARABLE.contains(&name.as_str());
        if !excused && st.bad > 0 {
            assert_eq!(st.first, Some(821), "{name} mismatches outside the hit tick: {st:?}");
            assert_eq!(st.bad, 1, "{name}: {st:?}");
        }
    }

    // Pure free run: exact until the hit.
    let pure = run.free_run(n, &Default::default());
    assert!(pure.stopped.is_none(), "{:?}", pure.stopped);
    assert_eq!(pure.first_diff, Some(822));
    // With the hit injected: exact on every tick of the lap.
    let fr = run.free_run(n, &rep.events);
    assert!(fr.stopped.is_none(), "{:?}", fr.stopped);
    assert_eq!(fr.first_diff, None);
}
