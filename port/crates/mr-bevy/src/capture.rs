//! `--replay-capture DIR`: draw all six karts from an oracle physics capture
//! (tools/oracle/capture.py --physics: DIR/meta.json + DIR/frames.bin, one
//! record of every captured region per VBlank). Regions used: the kart index
//! table + 6 kart records at 0x800C3000 (records at +0xA8 + 0x504 * slot) and
//! the player camera at 0x800C4F40 (view matrix +0, eye +0x28).
//!
//! One logic tick per fixed step: advance to the next VBlank where the race
//! clock (kart 0 +0x358) changes, like tools/oracle/trace.py (at most 3
//! VBlanks, so the countdown still moves).

use std::io::{Read, Seek, SeekFrom};

use bevy::prelude::*;

use crate::karts::KartFeed;
use crate::kartmodel::KART_LEN;

const KARTS_ADDR: u32 = 0x800C_3000;
const KART0: u32 = 0x800C_30A8;
const CAM: u32 = 0x800C_4F40;

#[derive(Resource)]
pub struct CaptureReplay {
    file: std::fs::File,
    regions: Vec<(u32, usize)>,
    pub vbs: Vec<i64>,
    size: usize,
    pub idx: usize,
    pub vb: i64,
}

/// Numbers of the first JSON array after `"key":` (nested arrays flattened).
fn json_numbers(s: &str, key: &str) -> Vec<i64> {
    let Some(i) = s.find(&format!("\"{key}\"")) else { return vec![] };
    let rest = &s[i..];
    let Some(a) = rest.find('[') else { return vec![] };
    let mut depth = 0;
    let mut end = a;
    for (j, ch) in rest[a..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = a + j;
                    break;
                }
            }
            _ => {}
        }
    }
    rest[a..end]
        .split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .filter_map(|t| t.parse().ok())
        .collect()
}

impl CaptureReplay {
    pub fn open(dir: &str) -> std::io::Result<Self> {
        let meta = std::fs::read_to_string(format!("{dir}/meta.json"))?;
        let r = json_numbers(&meta, "regions");
        let regions: Vec<(u32, usize)> = r.chunks_exact(2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        let vbs = json_numbers(&meta, "vb");
        let size = regions.iter().map(|r| r.1).sum();
        let file = std::fs::File::open(format!("{dir}/frames.bin"))?;
        Ok(CaptureReplay { file, regions, vbs, size, idx: 0, vb: 0 })
    }

    fn frame(&mut self, i: usize) -> Option<Vec<u8>> {
        let mut b = vec![0u8; self.size];
        self.file.seek(SeekFrom::Start((i * self.size) as u64)).ok()?;
        self.file.read_exact(&mut b).ok()?;
        Some(b)
    }

    fn get<'a>(&self, f: &'a [u8], addr: u32, n: usize) -> Option<&'a [u8]> {
        let mut o = 0;
        for &(a, len) in &self.regions {
            if a <= addr && addr as usize + n <= a as usize + len {
                let s = o + (addr - a) as usize;
                return f.get(s..s + n);
            }
            o += len;
        }
        None
    }
}

/// Advance one logic tick and publish all karts + the player pose/camera.
pub fn tick(cap: Option<ResMut<CaptureReplay>>, mut feed: ResMut<KartFeed>, mut p: ResMut<crate::Player>) {
    let Some(mut cap) = cap else { return };
    let cap = &mut *cap;
    feed.external = true;
    let clock = |cap: &CaptureReplay, f: &[u8]| cap.get(f, KART0 + 0x358, 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let i0 = cap.idx;
    let Some(cur) = cap.frame(i0) else { return };
    let c0 = clock(cap, &cur);
    let mut next = cap.idx;
    for _ in 0..3 {
        if next + 1 >= cap.vbs.len() {
            break;
        }
        next += 1;
        let f = cap.frame(next).unwrap();
        if clock(cap, &f) != c0 {
            break;
        }
    }
    if p.tick > 0 {
        cap.idx = next;
    }
    let i1 = cap.idx;
    let Some(f) = cap.frame(i1) else { return };
    cap.vb = cap.vbs[cap.idx];
    if crate::settings::args().into_iter().any(|a| a == "--capture-log") {
        eprintln!("capture tick {} vb {}", p.tick, cap.vb);
    }
    let recs: Vec<(u8, Vec<u8>)> = (0..6u8)
        .filter_map(|s| cap.get(&f, KARTS_ADDR + 0xA8 + KART_LEN as u32 * s as u32, KART_LEN).map(|r| (s, r.to_vec())))
        .collect();
    feed.set_records(recs.iter().map(|(s, r)| (*s, &r[..])));
    // player pose + camera from kart 0 and the camera record
    if let (Some(k), Some(c)) = (recs.first().map(|r| r.1.clone()), cap.get(&f, CAM, 0x34).map(|c| c.to_vec())) {
        let i32at = |b: &[u8], o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let i16at = |b: &[u8], o: usize| i16::from_le_bytes([b[o], b[o + 1]]) as i32;
        let mut m = [0; 9];
        let mut cm = [0; 9];
        for i in 0..9 {
            m[i] = i16at(&k, 0x18C + 2 * i);
            cm[i] = i16at(&c, 2 * i);
        }
        p.prev = p.pose;
        p.pose = crate::Pose {
            x: i32at(&k, 0),
            y: i32at(&k, 4),
            z: i32at(&k, 8),
            heading: i16at(&k, 0xE),
            m,
            cam: Some(([i32at(&c, 0x28), i32at(&c, 0x2C), i32at(&c, 0x30)], cm)),
            cam2: None,
        };
        p.prev_kart = p.kart.take();
        p.kart = Some(k);
    }
}
