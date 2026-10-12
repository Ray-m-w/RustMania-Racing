//! mr-compare REF.trace.jsonl CAND.trace.jsonl [--out DIR] [--x F --z F --y F
//!   --heading F --full 4096 --scale 65536 --pos-tol T --deg-tol D --window N]
//! Prints PASS/FAIL + per-field stats; writes error/trajectory SVGs to DIR.
//! Exit code 0 = PASS, 1 = FAIL, 2 = usage.
use mr_trace::plot::{chart, Series};
use mr_trace::{compare, Schema, Tolerance, Trace};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: mr-compare REF CAND [options]");
        std::process::exit(2);
    }
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let refr = Trace::read(&a[1]).expect("read REF");
    let cand = Trace::read(&a[2]).expect("read CAND");
    let s = Schema {
        x: opt("--x").unwrap_or("x".into()),
        z: opt("--z").unwrap_or("z".into()),
        y: Some(opt("--y").unwrap_or("y".into())).filter(|y| refr.col(y).is_some()),
        heading: opt("--heading")
            .or_else(|| refr.col("heading").map(|_| "heading".into()))
            .map(|h| (h, opt("--full").map(|v| v.parse().unwrap()).unwrap_or(4096))),
        pos_scale: opt("--scale").map(|v| v.parse().unwrap()).unwrap_or(65536.0),
    };
    let tol = Tolerance {
        pos: opt("--pos-tol").map(|v| v.parse().unwrap()).unwrap_or(1.0),
        heading_deg: opt("--deg-tol").map(|v| v.parse().unwrap()).unwrap_or(2.0),
        window: opt("--window").map(|v| v.parse().unwrap()).unwrap_or(300),
    };
    let rep = compare(&refr, &cand, &s, &tol);
    print!("{}", rep.summary(&tol));
    if let Some(dir) = opt("--out") {
        std::fs::create_dir_all(&dir).unwrap();
        let name = |t: &Trace| format!("{} ({})", t.source, t.route);
        let pe: Vec<(f64, f64)> = rep.pos_err.iter().map(|&(f, e)| (f as f64, e)).collect();
        std::fs::write(
            format!("{dir}/pos_err.svg"),
            chart("Position error vs tick", "tick", "world units", &[Series { label: "|ref - cand|", color: "#1f6fb4", pts: pe }], Some((tol.pos, "tolerance")), false),
        )
        .unwrap();
        if !rep.heading_err.is_empty() {
            let he: Vec<(f64, f64)> = rep.heading_err.iter().map(|&(f, e)| (f as f64, e)).collect();
            std::fs::write(
                format!("{dir}/heading_err.svg"),
                chart("Heading error vs tick", "tick", "degrees", &[Series { label: "|ref - cand|", color: "#b4471f", pts: he }], Some((tol.heading_deg, "tolerance")), false),
            )
            .unwrap();
        }
        let traj = |t: &Trace| -> Vec<(f64, f64)> {
            let (x, z) = (t.series(&s.x), t.series(&s.z));
            x.iter().zip(z.iter()).map(|(&x, &z)| (x as f64 / s.pos_scale, -(z as f64) / s.pos_scale)).collect()
        };
        std::fs::write(
            format!("{dir}/trajectory.svg"),
            chart("Trajectory, top down (x, -z)", "x", "-z", &[
                Series { label: &name(&refr), color: "#1f6fb4", pts: traj(&refr) },
                Series { label: &name(&cand), color: "#d62728", pts: traj(&cand) },
            ], None, true),
        )
        .unwrap();
    }
    std::process::exit(if rep.pass { 0 } else { 1 });
}
