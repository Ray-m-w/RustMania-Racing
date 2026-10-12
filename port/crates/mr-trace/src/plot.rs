//! Hand-written SVG plots (no dependencies): line charts of a series against
//! the tick, and a top-down trajectory overlay.

use std::fmt::Write as _;

pub struct Series<'a> {
    pub label: &'a str,
    pub color: &'a str,
    pub pts: Vec<(f64, f64)>,
}

const W: f64 = 900.0;
const H: f64 = 420.0;
const ML: f64 = 70.0;
const MR: f64 = 20.0;
const MT: f64 = 36.0;
const MB: f64 = 44.0;

fn bounds(series: &[Series], equal: bool) -> (f64, f64, f64, f64) {
    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y0 = f64::INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for s in series {
        for &(x, y) in &s.pts {
            if x.is_finite() && y.is_finite() {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if !x0.is_finite() {
        return (0.0, 1.0, 0.0, 1.0);
    }
    if x1 - x0 < 1e-9 {
        x1 = x0 + 1.0;
    }
    if y1 - y0 < 1e-9 {
        y1 = y0 + 1.0;
    }
    if equal {
        let pw = W - ML - MR;
        let ph = H - MT - MB;
        let sx = (x1 - x0) / pw;
        let sy = (y1 - y0) / ph;
        let s = sx.max(sy);
        let cx = (x0 + x1) / 2.0;
        let cy = (y0 + y1) / 2.0;
        x0 = cx - s * pw / 2.0;
        x1 = cx + s * pw / 2.0;
        y0 = cy - s * ph / 2.0;
        y1 = cy + s * ph / 2.0;
    }
    (x0, x1, y0, y1)
}

fn nice_ticks(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let span = hi - lo;
    let raw = span / n as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| span / s <= n as f64)
        .unwrap_or(10.0 * mag);
    let mut t = (lo / step).ceil() * step;
    let mut v = Vec::new();
    while t <= hi + 1e-9 {
        v.push(t);
        t += step;
    }
    v
}

/// Line chart. `hline`: optional horizontal threshold (value, label).
pub fn chart(
    title: &str,
    xlabel: &str,
    ylabel: &str,
    series: &[Series],
    hline: Option<(f64, &str)>,
    equal_axes: bool,
) -> String {
    let (x0, x1, mut y0, mut y1) = bounds(series, equal_axes);
    if let Some((h, _)) = hline {
        y0 = y0.min(h);
        y1 = y1.max(h * 1.1);
    }
    let px = |x: f64| ML + (x - x0) / (x1 - x0) * (W - ML - MR);
    let py = |y: f64| H - MB - (y - y0) / (y1 - y0) * (H - MT - MB);
    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {W} {H}' font-family='sans-serif' font-size='12'>\
<rect width='100%' height='100%' fill='white'/>\
<text x='{}' y='20' font-size='14' font-weight='bold'>{}</text>",
        ML,
        esc(title)
    );
    for t in nice_ticks(x0, x1, 8) {
        let x = px(t);
        let _ = write!(s, "<line x1='{x:.1}' y1='{MT}' x2='{x:.1}' y2='{}' stroke='#eee'/><text x='{x:.1}' y='{}' text-anchor='middle' fill='#555'>{}</text>", H - MB, H - MB + 16.0, fmt_num(t));
    }
    for t in nice_ticks(y0, y1, 6) {
        let y = py(t);
        let _ = write!(s, "<line x1='{ML}' y1='{y:.1}' x2='{}' y2='{y:.1}' stroke='#eee'/><text x='{}' y='{:.1}' text-anchor='end' fill='#555'>{}</text>", W - MR, ML - 6.0, y + 4.0, fmt_num(t));
    }
    let _ = write!(s, "<rect x='{ML}' y='{MT}' width='{}' height='{}' fill='none' stroke='#999'/>", W - ML - MR, H - MT - MB);
    let _ = write!(s, "<text x='{}' y='{}' text-anchor='middle' fill='#333'>{}</text>", (ML + W - MR) / 2.0, H - 8.0, esc(xlabel));
    let _ = write!(s, "<text x='14' y='{}' transform='rotate(-90 14 {})' text-anchor='middle' fill='#333'>{}</text>", (MT + H - MB) / 2.0, (MT + H - MB) / 2.0, esc(ylabel));
    if let Some((h, lab)) = hline {
        let y = py(h);
        let _ = write!(s, "<line x1='{ML}' y1='{y:.1}' x2='{}' y2='{y:.1}' stroke='#c00' stroke-dasharray='6 4'/><text x='{}' y='{:.1}' text-anchor='end' fill='#c00'>{}</text>", W - MR, W - MR - 4.0, y - 4.0, esc(lab));
    }
    for (i, se) in series.iter().enumerate() {
        let mut d = String::new();
        for (k, &(x, y)) in se.pts.iter().enumerate() {
            let _ = write!(d, "{}{:.1},{:.1} ", if k == 0 { "M" } else { "L" }, px(x), py(y));
        }
        let _ = write!(s, "<path d='{d}' fill='none' stroke='{}' stroke-width='1.6'/>", se.color);
        let ly = MT + 16.0 + 16.0 * i as f64;
        let _ = write!(s, "<rect x='{}' y='{}' width='14' height='3' fill='{}'/><text x='{}' y='{}'>{}</text>", ML + 10.0, ly - 4.0, se.color, ML + 30.0, ly, esc(se.label));
    }
    s.push_str("</svg>\n");
    s
}

fn fmt_num(v: f64) -> String {
    if v.abs() >= 1000.0 || v == v.trunc() {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
