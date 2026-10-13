//! mr-ai: check the AI drivers (src/ai.rs) against oracle captures.
//!
//!   mr-ai tf   CAPTURE_DIR [--ticks N] [--verbose]
//!       Teacher forcing of the AI's decision: every tick, every AI kart, from
//!       the oracle's record: the button word (+0x44E), the steering target
//!       (+0x3E8..+0x3F0) and the curvature (+0x4A0) the game produced.
//!   mr-ai tfk  CAPTURE_DIR [--ticks N] [--verbose]
//!       Teacher forcing of the kart update on the AI karts: the oracle's
//!       buttons through mr-sim's FUN_80064B5C port; motion fields compared.
//!   mr-ai vehicles CAPTURE_DIR
//!       The 48 vehicle templates (ai::vehicle_templates + place_wheels with
//!       the models from the disc) against the template table in ram0.bin.
//!   mr-ai free CAPTURE_DIR [--ticks N] [--seed S] [--out FILE.csv]
//!       Free run: all AI karts from the oracle's state at the first tick,
//!       driven by src/ai.rs through mr-sim's kart update, kart-kart contact
//!       among them; the player kart replayed from the oracle. Items off.
//!       Writes per tick and kart: x, z, lap, lateral offset, ... (CSV).
//!
//! ai.rs is included with #[path] until lib.rs declares `pub mod ai;`
//! (another agent owns lib.rs this session); the shim modules below give it
//! the `crate::` paths it uses.

mod fixed {
    pub use mr_sim::fixed::*;
}
mod globals {
    pub use mr_sim::globals::*;
}
mod kart {
    pub use mr_sim::kart::*;
}
mod tables {
    pub use mr_sim::tables::*;
}
mod world {
    pub use mr_sim::world::*;
}
mod init {
    pub use mr_sim::init::*;
}
#[path = "../../ai.rs"]
#[allow(dead_code)]
mod ai;
#[allow(dead_code)]
mod harness;
use harness as ai_harness;

use mr_sim::oracle::{self, Capture};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cmd = args.get(1).cloned().unwrap_or_default();
    let dir = args.get(2).cloned().expect("usage: mr-ai tf|free CAPTURE_DIR");
    let cap = Capture::open(&dir).expect("open capture");
    let base = cap.ram0.clone().expect("capture needs ram0.bin");
    let world = oracle::dock_world(&base);
    let ticks = ai_harness::ticks(&cap);
    let n = opt("--ticks").map(|v| v.parse().unwrap()).unwrap_or(ticks.len() - 1).min(ticks.len() - 1);
    let verbose = args.iter().any(|a| a == "--verbose");
    match cmd.as_str() {
        "tf" => {
            let rep = ai_harness::teacher_force(&cap, &base, &world, &ticks[..=n], verbose);
            print!("{}", rep.text());
        }
        "tfk" => {
            let rep = ai_harness::teacher_force_update(&cap, &base, &world, &ticks[..=n], verbose);
            print!("{}", rep.text());
        }
        "vehicles" => {
            print!("{}", ai_harness::check_vehicles(&base, &world));
        }
        "free" => {
            let seed = opt("--seed").map(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap());
            let fr = ai_harness::free_run(&cap, &base, &world, &ticks[..=n], seed);
            if let Some(out) = opt("--out") {
                std::fs::write(&out, fr.csv()).unwrap();
                std::fs::write(out.replace(".csv", ".txt"), fr.summary()).unwrap();
                println!("wrote {out}");
            }
            print!("{}", fr.summary());
        }
        _ => panic!("usage: mr-ai tf|free CAPTURE_DIR"),
    }
}
