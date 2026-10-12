//! mr-standalone: run the standalone Session (no oracle data at run time)
//! and compare it with the oracle afterwards.
//!
//!   mr-standalone [--capture analysis/oracle/lap1P3]
//!                 [--oracle-trace analysis/traces/lap1.oracle.trace.jsonl]
//!                 [--out analysis/traces/lap1.standalone.trace.jsonl]
//!
//! 1. Session::new's kart record vs the oracle's at the first captured
//!    VBlank, field by field; then again after idling until the race clock
//!    matches the oracle's (the savestate was taken 143 ticks into the race).
//! 2. Free run with the buttons the oracle trace recorded per tick (the lap1
//!    route), dt = 2 throughout; trace written to --out. Compare with
//!    `mr-compare ORACLE OUT`.

use mr_sim::harness::val;
use mr_sim::kart::{diff_records, FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::session::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |k: &str, d: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| d.to_string());
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cap = Capture::open(opt("--capture", "analysis/oracle/lap1P3")).expect("capture");
    let otrace = opt("--oracle-trace", "analysis/traces/lap1.oracle.trace.jsonl");
    let out = opt("--out", "analysis/traces/lap1.standalone.trace.jsonl");
    // Analysis switches (not used for the default trace):
    //   --oracle-dt        use the dt the oracle recorded per tick (3 in heavy frames)
    //   --catchup-as-6th   catch-up as the oracle's race (player 6th, leader far ahead)
    let oracle_dt = args.iter().any(|a| a == "--oracle-dt");
    let catchup6 = args.iter().any(|a| a == "--catchup-as-6th");

    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let mut s = Session::new(&mut disc).expect("session");

    // ---- 1. initial state ----
    let vb0 = cap.vbs[0];
    let orec = cap.read(vb0, KART_ARRAY, RECORD_SIZE).unwrap().to_vec();
    let report = |label: &str, rec: &[u8]| {
        let mut eq = 0;
        let mut diffs = Vec::new();
        for &(name, off, sz) in FIELD_OFFSETS {
            if rec[off..off + sz] == orec[off..off + sz] {
                eq += 1;
            } else {
                diffs.push(format!("{name}@{off:03X} {} vs {}", val(&rec[off..off + sz]), val(&orec[off..off + sz])));
            }
        }
        let body_eq = rec[0x18C..0x19E] == orec[0x18C..0x19E];
        let bytes_eq = (0..RECORD_SIZE).filter(|&i| rec[i] == orec[i]).count();
        println!("== {label}: {eq}/{} named fields equal, body matrix {}, {bytes_eq}/{RECORD_SIZE} record bytes equal", FIELD_OFFSETS.len(), if body_eq { "equal" } else { "differs" });
        for d in &diffs {
            println!("   {d}");
        }
        let _ = diff_records;
    };
    report("Session::new vs oracle VBlank 1", &s.record);
    let target = cap.i32(vb0, KART_ARRAY + 0x358).unwrap();
    let mut idle = 0;
    if catchup6 {
        s.leader_progress_override = Some(1000);
        s.kart.catchup = 180;
    }
    while s.kart.race_time < target {
        s.tick(0).expect("idle tick");
        idle += 1;
    }
    println!("\n(idled {idle} ticks with no buttons: race clock {} = oracle {target})", s.kart.race_time);
    report("after idling", &s.record);

    // ---- 2. free run on the oracle's per-tick buttons ----
    let text = std::fs::read_to_string(&otrace).expect("oracle trace");
    let mut lines = text.lines();
    let header = lines.next().unwrap();
    let fields: Vec<String> = mr_sim::oracle::json_ints(header, "fields").iter().map(|_| String::new()).collect();
    let _ = fields;
    let rows: Vec<Vec<i64>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_matches(|c| c == '[' || c == ']').split(',').map(|x| x.trim().parse().unwrap()).collect())
        .collect();
    let mut o = String::from(
        "{\"mrm_trace\":1,\"source\":\"rust-standalone\",\"route\":\"lap1\",\"note\":\"mr_sim::session::Session, no oracle data at run time; buttons per tick from the oracle trace; dt 2 unless --oracle-dt\",\"fields\":[\"x\",\"y\",\"z\",\"pitch\",\"heading\",\"roll\",\"vx\",\"vy\",\"vz\",\"speed\",\"yaw_rate\",\"m0\",\"m1\",\"m2\",\"m3\",\"m4\",\"m5\",\"m6\",\"m7\",\"m8\",\"clock\",\"dt\",\"cam_x\",\"cam_y\",\"cam_z\",\"c0\",\"c1\",\"c2\",\"c3\",\"c4\",\"c5\",\"c6\",\"c7\",\"c8\"]}\n",
    );
    let mut push = |t: i64, btn: u16, s: &Session| {
        let k = &s.kart;
        let m = k.body;
        o += &format!(
            "[{t},{btn},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},2]\n",
            k.x, k.y, k.z, k.pitch, k.heading, k.roll, k.vx, k.vy, k.vz, k.fwd_speed, k.side_speed,
            m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2], k.race_time
        )
        .replace(",2]
", &format!(",{}]
", s.globals.dt));
        let c = &s.camera;
        o.truncate(o.len() - 2);
        o += &format!(
            ",{},{},{},{},{},{},{},{},{},{},{},{}]
",
            c.pos[0], c.pos[1], c.pos[2],
            c.matrix[0][0], c.matrix[0][1], c.matrix[0][2], c.matrix[1][0], c.matrix[1][1], c.matrix[1][2], c.matrix[2][0], c.matrix[2][1], c.matrix[2][2]
        );
    };
    push(rows[0][0], rows[0][1] as u16, &s);
    let mut stopped = None;
    // dt is field "dt" of the oracle trace: index of "dt" in its fields + 2.
    let names: Vec<&str> = {
        let i = header.find("\"fields\"").unwrap();
        let rest = &header[i..];
        let a = rest.find('[').unwrap();
        let b = rest.find(']').unwrap();
        rest[a + 1..b].split(',').map(|x| x.trim().trim_matches('"')).collect()
    };
    let dt_col = names.iter().position(|&n| n == "dt").map(|i| i + 2);
    for r in &rows[1..] {
        let btn = r[1] as u16;
        let dt = if oracle_dt { dt_col.map(|c| r[c] as i32).unwrap_or(2) } else { 2 };
        if let Err(e) = s.tick_dt(btn, dt) {
            stopped = Some((r[0], e.to_string()));
            break;
        }
        push(r[0], btn, &s);
    }
    std::fs::write(&out, o).unwrap();
    println!("\n== free run: {} ticks -> {out}{} ==", s.ticks - idle as u64, stopped.map(|(t, m)| format!(" (stopped at tick {t}: {m})")).unwrap_or_default());
}
