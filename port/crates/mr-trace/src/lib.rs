//! mr-trace: one trace format for the oracle (psxrecomp) and the Rust
//! simulation, a comparator with pass/fail tolerances, and SVG plots.
//! std-only (ADR-0002 boundary).
//!
//! File format (".trace.jsonl"), one JSON value per line:
//!   line 1: {"mrm_trace":1,"source":"oracle","route":"accel-straight",
//!            "fields":["x","y","z",...],"note":"..."}
//!   then:   [frame, input, v0, v1, ...]   integers only, one per sim tick.
//! `frame` counts sim ticks from the route's anchor (frame 0 = the first
//! tick the route's input is visible to the game). `input` is the active-high
//! PS1 button mask the game saw that tick. Values are raw game integers
//! (16.16 positions, 12-bit angles, ...); interpretation lives in `Schema`.

pub mod plot;

use std::fmt::Write as _;
use std::io::{BufRead, Write};
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub source: String,
    pub route: String,
    pub note: String,
    pub fields: Vec<String>,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub frame: i64,
    pub input: u32,
    pub v: Vec<i64>,
}

impl Trace {
    pub fn new(source: &str, route: &str, fields: &[&str]) -> Self {
        Trace {
            source: source.into(),
            route: route.into(),
            note: String::new(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn col(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f == name)
    }

    pub fn series(&self, name: &str) -> Vec<i64> {
        let c = self.col(name).unwrap_or_else(|| panic!("no field {name}"));
        self.rows.iter().map(|r| r.v[c]).collect()
    }

    pub fn push(&mut self, frame: i64, input: u32, v: Vec<i64>) {
        assert_eq!(v.len(), self.fields.len());
        self.rows.push(Row { frame, input, v });
    }

    pub fn write(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        let fields: Vec<String> = self.fields.iter().map(|s| json_str(s)).collect();
        writeln!(
            f,
            "{{\"mrm_trace\":1,\"source\":{},\"route\":{},\"note\":{},\"fields\":[{}]}}",
            json_str(&self.source),
            json_str(&self.route),
            json_str(&self.note),
            fields.join(",")
        )?;
        for r in &self.rows {
            let mut s = format!("[{},{}", r.frame, r.input);
            for x in &r.v {
                let _ = write!(s, ",{x}");
            }
            s.push(']');
            writeln!(f, "{s}")?;
        }
        Ok(())
    }

    pub fn read(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let f = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut t = Trace::default();
        for (i, line) in f.lines().enumerate() {
            let line = line?;
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if i == 0 {
                t.source = json_field(l, "source").unwrap_or_default();
                t.route = json_field(l, "route").unwrap_or_default();
                t.note = json_field(l, "note").unwrap_or_default();
                t.fields = json_array_strs(l, "fields");
                continue;
            }
            let nums: Vec<i64> = l
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().parse().expect("trace row integer"))
                .collect();
            t.rows.push(Row {
                frame: nums[0],
                input: nums[1] as u32,
                v: nums[2..].to_vec(),
            });
        }
        Ok(t)
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Minimal extraction of a top-level string field from the header line.
/// Byte index just after `"key":` (whitespace around ':' allowed).
fn after_key(l: &str, key: &str) -> Option<usize> {
    let pat = format!("\"{key}\"");
    let mut i = l.find(&pat)? + pat.len();
    let b = l.as_bytes();
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if i >= b.len() || b[i] != b':' {
        return None;
    }
    i += 1;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    Some(i)
}

fn json_field(l: &str, key: &str) -> Option<String> {
    let i = after_key(l, key)?;
    if l.as_bytes().get(i) != Some(&b'"') {
        return None;
    }
    let i = i + 1;
    let mut out = String::new();
    let mut esc = false;
    for c in l[i..].chars() {
        if esc {
            out.push(match c {
                'n' => '\n',
                c => c,
            });
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

fn json_array_strs(l: &str, key: &str) -> Vec<String> {
    let Some(i) = after_key(l, key) else { return vec![] };
    if l.as_bytes().get(i) != Some(&b'[') {
        return vec![];
    }
    let rest = &l[i + 1..];
    let end = rest.find(']').unwrap_or(rest.len());
    rest[..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// How to read a kart trace: which fields are the planar position, the
/// heading, and their units. Kept separate from the format so the format
/// never has to change when the schema grows.
#[derive(Clone, Debug)]
pub struct Schema {
    pub x: String,
    pub z: String,
    pub y: Option<String>,
    /// Heading field and its full-circle value (4096 for the PS1 GTE angle).
    pub heading: Option<(String, i64)>,
    /// Raw position units per world unit (65536 for 16.16).
    pub pos_scale: f64,
}

#[derive(Clone, Debug)]
pub struct Tolerance {
    /// Max planar position error, in world units (same units as pos/pos_scale).
    pub pos: f64,
    /// Max heading error, degrees.
    pub heading_deg: f64,
    /// Window in ticks over which pos/heading must hold (from frame 0).
    pub window: i64,
}

#[derive(Clone, Debug, Default)]
pub struct FieldStat {
    pub name: String,
    pub max_abs: i64,
    pub first_diff_frame: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub frames: usize,
    pub pos_err: Vec<(i64, f64)>,
    pub heading_err: Vec<(i64, f64)>,
    pub max_pos_in_window: f64,
    pub max_heading_in_window: f64,
    pub first_pos_fail: Option<i64>,
    pub first_heading_fail: Option<i64>,
    pub fields: Vec<FieldStat>,
    pub input_mismatch: Option<i64>,
    pub pass: bool,
}

pub fn wrap_deg(d: f64) -> f64 {
    let mut d = d % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

/// Compare `cand` against `refr` frame by frame (matched on `frame`).
pub fn compare(refr: &Trace, cand: &Trace, s: &Schema, tol: &Tolerance) -> Report {
    use std::collections::HashMap;
    let idx: HashMap<i64, &Row> = cand.rows.iter().map(|r| (r.frame, r)).collect();
    let (rx, rz) = (refr.col(&s.x).unwrap(), refr.col(&s.z).unwrap());
    let (cx, cz) = (cand.col(&s.x).unwrap(), cand.col(&s.z).unwrap());
    let ry = s.y.as_ref().and_then(|y| refr.col(y));
    let cy = s.y.as_ref().and_then(|y| cand.col(y));
    let rh = s.heading.as_ref().and_then(|(h, _)| refr.col(h));
    let ch = s.heading.as_ref().and_then(|(h, _)| cand.col(h));
    let mut rep = Report::default();
    let common: Vec<(String, usize, usize)> = refr
        .fields
        .iter()
        .enumerate()
        .filter_map(|(i, f)| cand.col(f).map(|j| (f.clone(), i, j)))
        .collect();
    rep.fields = common
        .iter()
        .map(|(n, _, _)| FieldStat {
            name: n.clone(),
            ..Default::default()
        })
        .collect();
    for r in &refr.rows {
        let Some(c) = idx.get(&r.frame) else { continue };
        rep.frames += 1;
        if r.input != c.input && rep.input_mismatch.is_none() {
            rep.input_mismatch = Some(r.frame);
        }
        for (k, (_, i, j)) in common.iter().enumerate() {
            let d = (r.v[*i] - c.v[*j]).abs();
            let st = &mut rep.fields[k];
            if d > st.max_abs {
                st.max_abs = d;
            }
            if d != 0 && st.first_diff_frame.is_none() {
                st.first_diff_frame = Some(r.frame);
            }
        }
        let dx = (r.v[rx] - c.v[cx]) as f64 / s.pos_scale;
        let dz = (r.v[rz] - c.v[cz]) as f64 / s.pos_scale;
        let dy = match (ry, cy) {
            (Some(a), Some(b)) => (r.v[a] - c.v[b]) as f64 / s.pos_scale,
            _ => 0.0,
        };
        let pe = (dx * dx + dz * dz + dy * dy).sqrt();
        rep.pos_err.push((r.frame, pe));
        let in_win = r.frame >= 0 && r.frame < tol.window;
        if in_win {
            rep.max_pos_in_window = rep.max_pos_in_window.max(pe);
            if pe > tol.pos && rep.first_pos_fail.is_none() {
                rep.first_pos_fail = Some(r.frame);
            }
        }
        if let (Some(a), Some(b), Some((_, full))) = (rh, ch, s.heading.as_ref()) {
            let he = wrap_deg((r.v[a] - c.v[b]) as f64 * 360.0 / *full as f64).abs();
            rep.heading_err.push((r.frame, he));
            if in_win {
                rep.max_heading_in_window = rep.max_heading_in_window.max(he);
                if he > tol.heading_deg && rep.first_heading_fail.is_none() {
                    rep.first_heading_fail = Some(r.frame);
                }
            }
        }
    }
    rep.pass = rep.frames > 0 && rep.first_pos_fail.is_none() && rep.first_heading_fail.is_none();
    rep
}

impl Report {
    pub fn summary(&self, tol: &Tolerance) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "{}  frames compared {}  window {} ticks",
            if self.pass { "PASS" } else { "FAIL" },
            self.frames,
            tol.window
        );
        let _ = writeln!(
            s,
            "  max position error in window {:.4} (tol {:.4}){}",
            self.max_pos_in_window,
            tol.pos,
            self.first_pos_fail
                .map(|f| format!("  first fail at tick {f}"))
                .unwrap_or_default()
        );
        if !self.heading_err.is_empty() {
            let _ = writeln!(
                s,
                "  max heading error in window {:.3} deg (tol {:.3}){}",
                self.max_heading_in_window,
                tol.heading_deg,
                self.first_heading_fail
                    .map(|f| format!("  first fail at tick {f}"))
                    .unwrap_or_default()
            );
        }
        if let Some(f) = self.input_mismatch {
            let _ = writeln!(s, "  WARNING: input differs from tick {f}");
        }
        for st in &self.fields {
            let _ = writeln!(
                s,
                "  {:<14} max |diff| {:>12}  first diff {}",
                st.name,
                st.max_abs,
                st.first_diff_frame
                    .map(|f| f.to_string())
                    .unwrap_or_else(|| "-".into())
            );
        }
        s
    }
}
