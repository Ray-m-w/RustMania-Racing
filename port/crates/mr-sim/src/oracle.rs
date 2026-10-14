//! Reading oracle captures (tools/oracle/capture.py output) for the
//! teacher-forcing harness. File I/O lives here, outside the simulation.
//!
//! A capture directory holds `meta.json` ("regions": [[addr, len], ...],
//! "vb": [...]), `frames.bin` (per VBlank, the regions concatenated) and,
//! for --physics captures, `ram0.bin` (2 MB RAM + 1 KB scratchpad at the
//! savestate restore).

use std::path::{Path, PathBuf};

use crate::mem::Mem;

pub struct Capture {
    pub dir: PathBuf,
    pub regions: Vec<(u32, usize)>,
    pub vbs: Vec<u32>,
    frame_size: usize,
    frames: Vec<u8>,
    pub ram0: Option<Vec<u8>>,
}

/// Integers of the JSON array that follows `"key":` (nested arrays flattened).
pub fn json_ints(s: &str, key: &str) -> Vec<i64> {
    let pat = format!("\"{key}\"");
    let Some(i) = s.find(&pat) else { return vec![] };
    let rest = &s[i + pat.len()..];
    let Some(open) = rest.find('[') else { return vec![] };
    let mut depth = 0;
    let mut out = Vec::new();
    let mut num = String::new();
    for ch in rest[open..].chars() {
        match ch {
            '[' => depth += 1,
            ']' => {
                if !num.is_empty() {
                    out.push(num.parse().unwrap());
                    num.clear();
                }
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            '0'..='9' | '-' => num.push(ch),
            _ => {
                if !num.is_empty() {
                    out.push(num.parse().unwrap());
                    num.clear();
                }
            }
        }
    }
    out
}

impl Capture {
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Capture> {
        let dir = dir.as_ref().to_path_buf();
        let meta = std::fs::read_to_string(dir.join("meta.json"))?;
        let r = json_ints(&meta, "regions");
        let regions: Vec<(u32, usize)> = r.chunks(2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        let vbs: Vec<u32> = json_ints(&meta, "vb").into_iter().map(|v| v as u32).collect();
        let frames = std::fs::read(dir.join("frames.bin"))?;
        let frame_size = regions.iter().map(|r| r.1).sum();
        let ram0 = std::fs::read(dir.join("ram0.bin")).ok();
        assert_eq!(frames.len(), frame_size * vbs.len(), "frames.bin size");
        Ok(Capture { dir, regions, vbs, frame_size, frames, ram0 })
    }

    fn frame(&self, vb: u32) -> Option<&[u8]> {
        let i = self.vbs.binary_search(&vb).ok()?;
        Some(&self.frames[i * self.frame_size..(i + 1) * self.frame_size])
    }

    pub fn has(&self, vb: u32) -> bool {
        self.vbs.binary_search(&vb).is_ok()
    }

    /// Bytes at `addr` for VBlank `vb`, if inside a captured region.
    pub fn read(&self, vb: u32, addr: u32, n: usize) -> Option<&[u8]> {
        let f = self.frame(vb)?;
        let mut o = 0;
        for &(a, len) in &self.regions {
            if a <= addr && addr as usize + n <= a as usize + len {
                let s = o + (addr - a) as usize;
                return Some(&f[s..s + n]);
            }
            o += len;
        }
        None
    }

    pub fn i32(&self, vb: u32, addr: u32) -> Option<i32> {
        self.read(vb, addr, 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u16(&self, vb: u32, addr: u32) -> Option<u16> {
        self.read(vb, addr, 2).map(|b| u16::from_le_bytes(b.try_into().unwrap()))
    }

    /// Memory at VBlank `vb`: `base` (ram0.bin by default) overlaid with
    /// the captured regions.
    pub fn mem_at(&self, vb: u32, base: Option<&[u8]>) -> Mem {
        let b = base.or(self.ram0.as_deref()).expect("capture has no ram0.bin; pass a base image");
        let mut m = Mem::new(b[..0x20_0000].to_vec(), b[0x20_0000..0x20_0400].to_vec());
        self.overlay(vb, &mut m);
        m
    }

    pub fn overlay(&self, vb: u32, m: &mut Mem) {
        let f = self.frame(vb).expect("no such vblank");
        let mut o = 0;
        for &(a, len) in &self.regions {
            m.write(a, &f[o..o + len]);
            o += len;
        }
    }
}

/// Default location of the oracle captures, relative to the repo root.
pub fn repo_root() -> PathBuf {
    let mut d = std::env::current_dir().unwrap();
    loop {
        if d.join("CLAUDE.md").exists() && d.join("port").exists() {
            return d;
        }
        if !d.pop() {
            panic!("repo root not found");
        }
    }
}

/// One record of tools/oracle/fixtures.py output.
pub struct Fixture {
    pub func: u32,
    pub vb: u32,
    pub args: [u32; 4],
    pub ret: u32,
    pub sp: u32,
    pub entry: Snap,
    pub exit: Snap,
}

pub struct Snap {
    pub kart: Vec<u8>,
    pub globals: Vec<u8>,
    pub scratch: Vec<u8>,
    pub stack: Vec<u8>,
}

pub const FX_KART: usize = 0x504;
pub const FX_GLOBALS: usize = 0x900;
pub const FX_GLOBALS_BASE: u32 = 0x800B_0700;

impl Snap {
    fn parse(b: &[u8]) -> Snap {
        let (k, r) = b.split_at(FX_KART);
        let (g, r) = r.split_at(FX_GLOBALS);
        let (s, st) = r.split_at(0x400);
        Snap { kart: k.to_vec(), globals: g.to_vec(), scratch: s.to_vec(), stack: st[..0x200].to_vec() }
    }

    /// Apply this snapshot onto a memory image (kart 0, globals, scratch).
    pub fn apply(&self, m: &mut Mem) {
        m.write(crate::kart::KART_ARRAY, &self.kart);
        m.write(FX_GLOBALS_BASE, &self.globals);
        m.scratch.copy_from_slice(&self.scratch);
    }
}

pub fn read_fixtures(path: impl AsRef<Path>) -> std::io::Result<Vec<Fixture>> {
    let b = std::fs::read(path)?;
    let snap = FX_KART + FX_GLOBALS + 0x400 + 0x200;
    let rec = 32 + 2 * snap;
    let mut out = Vec::new();
    for c in b.chunks_exact(rec) {
        let u = |i: usize| u32::from_le_bytes(c[4 * i..4 * i + 4].try_into().unwrap());
        out.push(Fixture {
            func: u(0),
            vb: u(1),
            args: [u(2), u(3), u(4), u(5)],
            ret: u(6),
            sp: u(7),
            entry: Snap::parse(&c[32..32 + snap]),
            exit: Snap::parse(&c[32 + snap..]),
        });
    }
    Ok(out)
}

use crate::camera::TrailEntry;
use crate::kart::KartState;
use crate::world::{HeapLayout, TickInputs, World};

/// Course-data addresses in RAM (pointers the .DAT loader FUN_8002E3DC sets).
pub mod course_addr {
    /// Heap address the .DAT file was loaded at.
    pub const DAT_BASE_PTR: u32 = 0x800B_0EE8;
    pub const GRID_PTR: u32 = 0x800D_1C4C;
    pub const FINISH_PTR: u32 = 0x800D_1C54;
    pub const LENGTH_PTR: u32 = 0x800D_1C5C;
    pub const TRIGGER_PTR: u32 = 0x800D_1C9C;
    pub const TRIGGER_COUNT: u32 = 0x800B_0DC6;
    pub const FACES_PTR: u32 = 0x800B_0FA4;
    /// Lists that are null / zero on DOCK (see world::Course).
    pub const OTHER_LISTS: [u32; 4] = [0x800D_1C68, 0x800D_1C8C, 0x800D_1C98, 0x800D_1CA4];
    pub const OTHER_BYTES: [u32; 3] = [0x800B_0C58, 0x800B_0E00, 0x800B_0C68];
    pub const LEADER: u32 = 0x800D_8F94;
}

/// Build the DOCK world: tables and terrain from the disc; course records
/// read from the disc's DOCK.DAT at the file offsets the game's RAM
/// pointers give (pointer - DAT load address), each checked against RAM.
pub fn dock_world(ram0: &[u8]) -> World {
    let mut disc = mr_disc::Disc::open_default().expect("open the owner's disc");
    let mut w = World::dock(&mut disc).expect("DOCK world from the disc");
    let m = Mem::new(ram0[..0x20_0000].to_vec(), ram0[0x20_0000..0x20_0400].to_vec());
    w.heap = HeapLayout { faces: m.u32(course_addr::FACES_PTR) };
    // The course records must be what the game registered (pointers into
    // the loaded DAT image).
    let base = m.u32(course_addr::DAT_BASE_PTR);
    let dat = disc.read("COURSE_A/DOCK.DAT").expect("read DOCK.DAT");
    for (ptr, n) in [(course_addr::GRID_PTR, 4), (course_addr::FINISH_PTR, 52), (course_addr::LENGTH_PTR, 4), (course_addr::TRIGGER_PTR, 72)] {
        let p = m.u32(ptr);
        let off = (p - base) as usize;
        assert_eq!(&dat[off..off + n], m.slice(p, n), "DOCK.DAT at {off:#x} vs RAM {p:08X}");
    }
    let c = &w.course;
    assert_eq!(c.triggers.len(), m.u8(course_addr::TRIGGER_COUNT) as usize);
    assert_eq!(c.grid.len(), (m.u16(m.u32(course_addr::GRID_PTR)) >> 1) as usize);
    let other = course_addr::OTHER_LISTS.iter().any(|&a| m.u32(a) != 0) || course_addr::OTHER_BYTES.iter().any(|&a| m.u8(a) != 0);
    assert_eq!(c.has_other_objects || !c.objects.jets.is_empty() || !c.objects.turntables.is_empty() || !c.objects.hooks.is_empty(), other);
    w
}

/// Per-tick inputs from the rest of the race, read from game memory.
pub fn tick_inputs(m: &Mem) -> TickInputs {
    let leader = m.u32(course_addr::LEADER);
    let lp = m.i32(crate::kart::KART_ARRAY + 0x504 * leader + 0x388);
    let h = |a: u32| m.i16(a);
    TickInputs {
        leader_progress: lp,
        water_normal: [h(0x800B_0CA0), h(0x800B_0CA2), h(0x800B_0CA4)],
        alt_normal: [h(0x800B_0EA8), h(0x800B_0EAA), h(0x800B_0EAC)],
        aim: None,
        cameras: [0, 1].map(|c| {
            let b = 0x800C_4F40 + 0x70 * c + 0x28;
            [m.i32(b), m.i32(b + 4), m.i32(b + 8)]
        }),
    }
}

/// Load kart `slot` from memory, including its history ring.
pub fn load_kart(m: &Mem, slot: u32) -> KartState {
    let a = crate::kart::KART_ARRAY + 0x504 * slot;
    let mut k = KartState::from_record(m.slice(a, 0x504));
    if k.history_ptr != 0 {
        // The game's 12 entries; the sim's longer ring repeats the oldest.
        for i in 0..crate::camera::TRAIL_LEN {
            let e = k.history_ptr + 16 * i.min(crate::camera::GAME_TRAIL_LEN - 1) as u32;
            k.trail[i] = TrailEntry { pos: [m.i32(e), m.i32(e + 4), m.i32(e + 8)], count: m.i32(e + 12) };
        }
    }
    k
}

/// Store kart `slot` (owned fields + history ring) into memory.
pub fn store_kart(m: &mut Mem, slot: u32, k: &KartState) {
    let a = crate::kart::KART_ARRAY + 0x504 * slot;
    let mut rec = m.slice(a, 0x504).to_vec();
    k.to_record(&mut rec);
    m.write(a, &rec);
    if k.history_ptr != 0 {
        for i in 0..crate::camera::GAME_TRAIL_LEN {
            let e = k.history_ptr + 16 * i as u32;
            let t = k.trail[i];
            m.put_u32(e, t.pos[0] as u32);
            m.put_u32(e + 4, t.pos[1] as u32);
            m.put_u32(e + 8, t.pos[2] as u32);
            m.put_u32(e + 12, t.count as u32);
        }
    }
}
