//! mr-verify: check the Rust kart tick against an oracle capture.
//!
//!   mr-verify [CAPTURE_DIR] [--base DIR] [--trace OUT.trace.jsonl]
//!             [--ticks N] [--verbose]
//!
//! CAPTURE_DIR defaults to analysis/oracle/lap1P3 (a --physics capture with
//! ram0.bin); --base takes ram0.bin from another capture of the same
//! savestate (the short routes were captured without one).
//!
//! 1. Teacher forcing: every tick from the oracle's state, compare every
//!    owned kart field with the oracle's next state; per field: ticks
//!    mismatching, first tick, max |error|.
//! 2. Free run (pure): the Rust kart alone from tick 0 (inputs from outside
//!    the kart from the oracle, see mr_sim::harness) -> OUT with
//!    ".pure.trace.jsonl".
//! 3. Free run with the external events found in 1 injected -> OUT.
//! Compare with: cargo run --bin mr-compare -- ORACLE.trace.jsonl OUT

use mr_sim::harness::{self, Run, NOT_COMPARABLE, EXTERNAL, PRE_EXTERNAL};
use mr_sim::oracle::{self, Capture};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let dir = args.get(1).filter(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| "analysis/oracle/lap1P3".into());
    let cap = Capture::open(&dir).expect("open capture");
    let base = match opt("--base") {
        Some(b) => std::fs::read(std::path::Path::new(&b).join("ram0.bin")).expect("read --base ram0.bin"),
        None => cap.ram0.clone().expect("capture needs ram0.bin (or pass --base)"),
    };
    let world = oracle::dock_world(&base);
    let run = Run::new(&cap, &base, &world);
    let n = opt("--ticks").map(|v| v.parse().unwrap()).unwrap_or(run.n()).min(run.n());
    let verbose = args.iter().any(|a| a == "--verbose");
    println!("capture {dir}: {n} ticks");

    let rep = run.teacher_forcing(n);
    println!("\n== teacher forcing: fields with mismatches (ticks / first tick / max |err|) ==");
    let tag = |name: &str| {
        if EXTERNAL.iter().chain(PRE_EXTERNAL).any(|e| e.0 == name) {
            "  [written outside the port: input]"
        } else if NOT_COMPARABLE.contains(&name) {
            "  [changed later in the frame / random bytes; no effect on motion]"
        } else {
            ""
        }
    };
    let mut motion_exact = true;
    for (name, st) in &rep.fields {
        if st.bad > 0 {
            println!("  {name:<20} {:>5}  first {:>5}  max {:>8}{}", st.bad, st.first.unwrap(), st.max, tag(name));
            if tag(name).is_empty() {
                motion_exact = false;
            }
        }
    }
    let exact = rep.fields.values().filter(|s| s.bad == 0).count();
    println!("  {exact} of {} owned fields exact on every tick", rep.fields.len());
    println!("  ticks with a position/angle/velocity mismatch (external events): {:?}", rep.events.keys().collect::<Vec<_>>());
    if verbose {
        for (t, f) in &rep.events {
            println!("    tick {t} (vb {}): fields {:?}", run.ticks[*t], f.iter().map(|x| format!("{:03X}", x.0)).collect::<Vec<_>>());
        }
    }
    println!("  random-pointer skip at kart 0's update that reproduces the random-driven fields: {:?} (skip: ticks; -1 = none)", rep.rng_skips);
    println!("  render-side record ranges (ticks mismatching / first tick):");
    for (name, st) in &rep.render {
        println!("    {name:<32} {:>5}  first {}", st.bad, st.first.map(|f| f.to_string()).unwrap_or_else(|| "-".into()));
    }
    for (t, m) in &rep.panics {
        println!("  tick {t}: panic {m}");
    }
    let _ = motion_exact;

    let mut all_events = rep.events.clone();
    for (t, r) in &rep.render_events {
        all_events.entry(*t).or_default().extend(r.iter().copied());
    }
    println!("  render-side external events (fields injected in the free run): {:?}", rep.render_events.keys().collect::<Vec<_>>());
    let out = opt("--trace").unwrap_or_else(|| "analysis/traces/lap1.rust.trace.jsonl".into());
    let pure = out.replace(".trace.jsonl", ".pure.trace.jsonl");
    for (path, events, note) in [
        (&pure, &Default::default(), "mr-sim free run; no injected events"),
        (&out, &all_events, "mr-sim free run; external events injected from the oracle"),
    ] {
        let fr = run.free_run_rng(n, events, Some(&rep.rng_at_frame));
        std::fs::write(path, harness::trace_jsonl(&fr, "lap1", &format!("{note}; capture {dir}"))).unwrap();
        println!(
            "\n== free run ({note}): {} ticks -> {path}{}; first tick differing from the oracle: {} ==",
            fr.rows.len() - 1,
            fr.stopped.as_ref().map(|(t, m)| format!(" STOPPED at {t}: {m}")).unwrap_or_default(),
            fr.first_diff.map(|t| t.to_string()).unwrap_or_else(|| "none".into())
        );
        println!("   first render-range difference: {}", fr.first_render_diff.as_ref().map(|(t, n)| format!("tick {t} ({n})")).unwrap_or_else(|| "none".into()));
        if verbose {
            if let Some((t, _)) = &fr.first_render_diff {
                for tt in t.saturating_sub(2)..=*t {
                    let mine = &fr.records[tt];
                    let refr = cap.read(run.ticks[tt], mr_sim::kart::KART_ARRAY, mr_sim::kart::RECORD_SIZE).unwrap();
                    let d: Vec<String> = (0..mr_sim::kart::RECORD_SIZE).step_by(2).filter(|&o| mine[o..o + 2] != refr[o..o + 2]).map(|o| format!("{o:03X}")).collect();
                    println!("     tick {tt}: halfwords differing: {}", d.join(" "));
                }
            }
        }
    }
}
