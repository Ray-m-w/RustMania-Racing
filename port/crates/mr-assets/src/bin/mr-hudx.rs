//! mr-hudx: lay out the full 1-player race HUD (minimal HUD + item HUD,
//! mr_assets::hud_items::race_hud) from a tools/oracle/fx_capture.py
//! directory's RAM rows, and print the primitives for the checker
//! (tools/oracle/hudx_check.py).
//!
//! usage: mr-hudx CAPDIR VB [VB ...]
//! Per VB: a line "# vb N", then one line per primitive in add order:
//!   S x y w h u v tpage clut raw semi r g b
//!   Q x0 y0 x1 y1 x2 y2 x3 y3 u0 v0 u1 v1 u2 v2 u3 v3 tpage clut semi r g b
//!   T x0 y0 x1 y1 x2 y2 r g b
//!   L x0 y0 x1 y1 r g b
//! tpage / clut use the captured slot table (0x800DFF00, stride 16).

use mr_assets::hud::{HudState, HudTables};
use mr_assets::hud_items::{self, HudPulse, ItemHud, ItemTables, MapKart, Prim};
use mr_assets::le;

/// A capture: regions + rows (fx_capture.py layout).
pub struct Cap {
    regions: Vec<(u32, usize)>,
    vbs: Vec<u32>,
    size: usize,
    data: Vec<u8>,
}

fn numbers(s: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() || (ch == '-' && cur.is_empty()) {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse() {
                out.push(v);
            }
            cur.clear();
        }
    }
    if let Ok(v) = cur.parse() {
        out.push(v);
    }
    out
}

impl Cap {
    pub fn open(dir: &str) -> Cap {
        let meta = std::fs::read_to_string(format!("{dir}/meta.json")).expect("meta.json");
        let sect = |key: &str| -> &str {
            let i = meta.find(&format!("\"{key}\"")).unwrap_or_else(|| panic!("{key}"));
            let rest = &meta[i..];
            let a = rest.find('[').unwrap();
            // matching bracket
            let mut depth = 0;
            for (k, ch) in rest[a..].char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            return &rest[a..a + k + 1];
                        }
                    }
                    _ => {}
                }
            }
            panic!("unterminated {key}");
        };
        let r = numbers(sect("regions"));
        let regions: Vec<(u32, usize)> = r.chunks_exact(2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        let vbs = numbers(sect("vb")).into_iter().map(|v| v as u32).collect();
        let size = regions.iter().map(|r| r.1).sum();
        let data = std::fs::read(format!("{dir}/frames.bin")).expect("frames.bin");
        Cap { regions, vbs, size, data }
    }
    pub fn read(&self, vb: u32, addr: u32, n: usize) -> Vec<u8> {
        let i = self.vbs.iter().position(|&v| v == vb).expect("vb captured");
        let row = &self.data[i * self.size..(i + 1) * self.size];
        let mut o = 0;
        for &(a, len) in &self.regions {
            if addr >= a && addr + n as u32 <= a + len as u32 {
                let s = o + (addr - a) as usize;
                return row[s..s + n].to_vec();
            }
            o += len;
        }
        panic!("{addr:#x} not captured")
    }
}

pub const KART0: u32 = 0x800C_30A8;

/// The minimal HUD state and the item HUD state at a captured VBlank.
pub fn states(c: &Cap, vb: u32) -> (HudState, ItemHud, [(u16, u16); 32]) {
    let i32at = |a: u32| le::i32(&c.read(vb, a, 4), 0);
    let i16at = |a: u32| le::i16(&c.read(vb, a, 2), 0);
    let rec = c.read(vb, KART0, 0x504);
    // The RAM row at VBlank t holds the state frame t was built from, except
    // what FUN_8005c2ec advances after the draw at the end of the frame: the
    // frame counter, the race clock and the HUD pulse. Take those from the
    // last row before the frame counter's latest step (the dt global already
    // holds the next frame's dt, so it cannot be subtracted).
    let d90 = |v: u32| le::i32(&c.read(v, 0x800B_0D90, 4), 0);
    let mut prev = vb.saturating_sub(1).max(c.vbs[0]);
    while prev > c.vbs[0] && d90(prev) == d90(vb) {
        prev -= 1;
    }
    let mut st = HudState {
        countdown: i32at(0x800B_0C70),
        countdown_start: i32at(0x800B_07F8),
        mode: c.read(vb, 0x800B_0D46, 1)[0],
        laps: c.read(vb, 0x800B_0DA6, 1)[0],
        race_time: le::i32(&c.read(prev, 0x800B_0A94, 4), 0),
        ..HudState::default()
    };
    st = st.with_kart(&rec);
    let n = (i16at(0x800B_0DFE) + i16at(0x800B_0C98)).clamp(0, 6) as u32;
    let karts = (0..n)
        .map(|k| {
            let r = c.read(vb, KART0 + k * 0x504, 0x504);
            MapKart { x: le::i32(&r, 0), z: le::i32(&r, 8), heading: le::i16(&r, 0xE), flags: r[0x496], character: r[0x4B2], damage: le::i16(&r, 0x440) }
        })
        .collect();
    let cm = c.read(vb, 0x800C_4F40, 0x12);
    let mut cam = [[0i16; 3]; 3];
    for (k, v) in cam.iter_mut().flatten().enumerate() {
        *v = le::i16(&cm, 2 * k);
    }
    let it = ItemHud {
        frame: d90(prev),
        strip: le::i32(&c.read(prev, 0x800B_07D4, 4), 0),
        pulse: HudPulse([le::i16(&c.read(prev, 0x800B_0E10, 2), 0), le::i16(&c.read(prev, 0x800B_0E12, 2), 0)]),
        race_time: st.race_time,
        countdown: st.countdown,
        mode: st.mode,
        course: c.read(vb, 0x800B_0FAC, 1)[0],
        e38: i16at(0x800B_0E38),
        special: (c.read(vb, 0x800B_07F4, 1)[0] as i8 as i16, i16at(0x800B_07F6), c.read(vb, 0x800B_0E95, 1)[0] as i8),
        cam,
        kart: rec,
        karts,
        players: i16at(0x800B_0E28).max(0) as usize,
        race_flags: 0,
        gauge_rgb: None,
    };
    // The stunt trial (race type 3): its HUD state (the course record from
    // MR_HUDX_RECORD: the captures do not hold the save block).
    let mut st = st;
    if c.read(vb, 0x800B_0B9A, 1)[0] == 3 {
        let r = &it.kart;
        st.stunt = Some(mr_assets::hud::StuntHud {
            progress: le::i16(r, 0x490) as i32 * 0x80 + r[0x4A8] as i32,
            quads: i16at(0x800B_0CC8),
            marks: [i32at(0x800B_0C90), i32at(0x800B_0C94)],
            n_marks: c.read(vb, 0x800B_0E94, 1)[0],
            race_flags: le::u16(&c.read(vb, 0x800B_07E2, 2), 0),
            character: r[0x4B2],
            kart_flags: le::u16(r, 0x496),
            time: le::i32(r, 0x358),
            flash: le::i32(r, 0x38C),
            place: r[0x4BE],
            shown_place: r[0x4CA],
            record: std::env::var("MR_HUDX_RECORD").ok().and_then(|v| v.parse().ok()).unwrap_or(i32::MIN),
            first_time: i32at(0x800B_0C20),
        });
        if st.mode == 10 {
            let f88 = c.read(vb, 0x800B_0F88, 1)[0] != 0 || c.read(vb, 0x800B_0EBE, 1)[0] != 0;
            // the prompt blinks with 0x800B0AF0 (VBlanks since mode 10)
            st.results = Some(mr_assets::hud::Results { press_start: f88, frame: i32at(0x800B_0AF0), ..Default::default() });
        }
    }
    // A battle (race type 2, game modes 2 / 10; phase 4): its HUD state
    // (the course record from MR_HUDX_RECORD, as the stunt trial's).
    if c.read(vb, 0x800B_0B9A, 1)[0] == 2 && matches!(st.mode, 2 | 10) {
        let r = &it.kart;
        st.battle = Some(mr_assets::hud::BattleHud {
            dealt: le::i16(r, 0x43C),
            taken: le::i16(r, 0x43E),
            best_crash: le::i16(r, 0x478),
            knockouts: r[0x4FB],
            counter: le::i32(r, 0x38C),
            shown_place: r[0x4CA],
            place: r[0x4BE],
            time: le::i32(r, 0x358),
            record: std::env::var("MR_HUDX_RECORD").ok().and_then(|v| v.parse().ok()).unwrap_or(i32::MIN),
            race_flags: le::u16(&c.read(vb, 0x800B_07E2, 2), 0),
        });
        if st.mode == 10 {
            let f88 = c.read(vb, 0x800B_0F88, 1)[0] != 0 || c.read(vb, 0x800B_0EBE, 1)[0] != 0;
            st.results = Some(mr_assets::hud::Results { press_start: f88, frame: i32at(0x800B_0AF0), ..Default::default() });
        }
    }
    let tab = c.read(vb, 0x800D_FF00, 0x160);
    let mut slots = [(0u16, 0u16); 32];
    for (i, s) in slots.iter_mut().enumerate().take(0x16) {
        *s = (le::u16(&tab, 16 * i), le::u16(&tab, 16 * i + 2));
    }
    (st, it, slots)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let cap = Cap::open(&a[0]);
    let mut disc = mr_disc::Disc::open_default().expect("open disc");
    let exe = disc.read("SLUS_012.37").expect("exe");
    let ht = HudTables::from_exe(&exe).expect("hud tables");
    let t = ItemTables::from_exe(&exe).expect("item tables");
    for v in &a[1..] {
        let vb: u32 = v.parse().expect("vb");
        let (st, it, slots) = states(&cap, vb);
        println!("# vb {vb}");
        for p in hud_items::race_hud(&ht, &t, &st, &it) {
            match p {
                Prim::Sprt(s) => {
                    let (tp, cl) = s.attrs(&slots);
                    println!("S {} {} {} {} {} {} {:#06x} {:#06x} {} {} {} {} {}", s.x, s.y, s.w, s.h, s.u, s.v, tp, cl, s.raw as u8, s.semi as u8, s.rgb[0], s.rgb[1], s.rgb[2]);
                }
                Prim::Ft4(q) => {
                    let (tp, cl) = slots[q.slot as usize & 31];
                    let tp = (tp & !0x60) | ((q.abr as u16) << 5);
                    let cl = cl.wrapping_add(q.clut_off);
                    let v: Vec<String> = q.v.iter().flat_map(|p| [p[0].to_string(), p[1].to_string()]).collect();
                    let uv: Vec<String> = q.uv.iter().flat_map(|p| [p[0].to_string(), p[1].to_string()]).collect();
                    println!("Q {} {} {:#06x} {:#06x} {} {} {} {}", v.join(" "), uv.join(" "), tp, cl, q.semi as u8, q.rgb[0], q.rgb[1], q.rgb[2]);
                }
                Prim::F3 { v, rgb } => println!("T {} {} {} {} {} {} {} {} {}", v[0][0], v[0][1], v[1][0], v[1][1], v[2][0], v[2][1], rgb[0], rgb[1], rgb[2]),
                Prim::Line { v, rgb } => println!("L {} {} {} {} {} {} {}", v[0][0], v[0][1], v[1][0], v[1][1], rgb[0], rgb[1], rgb[2]),
                Prim::Tile { x, y, w, h, rgb } => println!("R {x} {y} {w} {h} {} {} {}", rgb[0], rgb[1], rgb[2]),
            }
        }
    }
}
