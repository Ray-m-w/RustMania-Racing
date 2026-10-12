//! .RAW decoder tests. Disc-backed tests skip (with a message) when the
//! owner's disc is not configured. Values marked [RAM] were read from the
//! running recomp on DOCK (docs/spec/formats/RAW.md, Verification).

use mr_assets::raw::{self, LoadEnv, UploadKind};
use mr_assets::vram::Vram;

fn disc() -> Option<mr_disc::Disc> {
    match mr_disc::Disc::open_default() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("SKIP: disc not available ({e:?})");
            None
        }
    }
}

fn fnv(px: &[u16]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &p in px {
        for b in p.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

#[test]
fn crc16_check_value() {
    // CRC-16/ARC check value
    assert_eq!(raw::rnc_crc16(b"123456789"), 0xBB3D);
}

#[test]
fn get_tpage_matches_ram() {
    // [RAM] slot 9 (8-bit page at x=0x380) and slot 4 (4-bit at x=0x2c0)
    assert_eq!(raw::get_tpage(1, 0, 0x380, 0), 0x8E);
    assert_eq!(raw::get_tpage(0, 0, 0x2C0, 0), 0x0B);
}

#[test]
fn dock_decodes_like_the_game() {
    let Some(mut d) = disc() else { return };
    let file = d.read("COURSE_A/DOCK.RAW").unwrap();
    let exe = d.read("SLUS_012.37").unwrap();
    let env = LoadEnv::from_exe(&exe, raw::DOCK_COURSE).unwrap();
    assert_eq!(raw::course_raw_path(&exe, raw::DOCK_COURSE).as_deref(), Some("COURSE_A/DOCK.RAW"));
    assert_eq!(raw::course_raw_path(&exe, 30).as_deref(), Some("FRONTEND/OPTIONS.RAW"));
    assert_eq!(env.fog_rgb, [0x00, 0x13, 0x5F]);
    assert_ne!(env.course_flags & 0x10, 0, "DOCK uses desaturated CLUTs [RAM]");

    let rf = raw::parse(&file).unwrap();
    let kinds: Vec<u32> = rf.chunks.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [0x0, 0x1, 0x2, 0x3, 0x10, 0x11, 0x12, 0x14, 0xF, 0x15, 0x13, 0x4, 0x5, 0x22, 0x22, 0x22, 0x22, 0x22, 0xA, 0x9]
    );
    assert_eq!(rf.terminator, 0x44B33);
    assert_eq!(&file[rf.terminator..rf.terminator + 8], &[0xFF; 8]);
    // every byte accounted for: chunks back to back, then terminator + pad
    let mut at = 0;
    for c in &rf.chunks {
        assert_eq!(c.file_offset, at);
        at += raw::RNC_HEADER_LEN + c.header.packed_len as usize;
        assert_eq!(c.data.len(), c.header.unpacked_len as usize);
    }
    assert_eq!(at, rf.terminator);
    assert_eq!(rf.terminator + 8 + rf.trailing, file.len());

    let dec = raw::decode_chunks(&rf.chunks, &env).unwrap();
    let pages = dec.uploads.iter().filter(|u| u.what == UploadKind::Page).count();
    let cluts = dec.uploads.iter().filter(|u| matches!(u.what, UploadKind::Clut { .. })).count();
    assert_eq!((pages, cluts, dec.uploads.len()), (15, 33, 48));

    // [RAM] slot table at 0x800dff00: (slot, tpage, clut, x)
    let want: [(usize, u16, u16, i16); 15] = [
        (0x0, 0x00, 0x3070, 0x000),
        (0x1, 0x01, 0x30B0, 0x040),
        (0x2, 0x02, 0x30F0, 0x080),
        (0x3, 0x03, 0x3130, 0x0C0),
        (0x4, 0x0B, 0x3170, 0x2C0),
        (0x5, 0x0C, 0x31B0, 0x300),
        (0x9, 0x8E, 0x3DF0, 0x380),
        (0xA, 0x8D, 0x3E30, 0x340),
        (0xF, 0x08, 0x3EF0, 0x200),
        (0x10, 0x04, 0x3FF0, 0x100),
        (0x11, 0x05, 0x3FB0, 0x140),
        (0x12, 0x06, 0x3F70, 0x180),
        (0x13, 0x0A, 0x3E70, 0x280),
        (0x14, 0x07, 0x3F30, 0x1C0),
        (0x15, 0x09, 0x3EB0, 0x240),
    ];
    for (s, tp, cl, x) in want {
        let e = dec.slots[s].unwrap_or_else(|| panic!("slot {s:#x} missing"));
        assert_eq!((e.tpage, e.clut, e.x, e.y), (Some(tp), cl, x, 0), "slot {s:#x}");
    }
    assert_eq!(dec.slots.iter().flatten().count(), 15);

    // [RAM] 0x22 strips kept back to back in the work buffer
    // (DAT_800bbb20[slot] - 8 = buffer start 0x80120aac + offset)
    let r: Vec<(u32, usize, usize)> = dec
        .retained
        .iter()
        .map(|r| (le32(&r.data, 4), r.buffer_offset, r.data.len()))
        .collect();
    assert_eq!(r, [(0xA, 0x0, 32780), (0xC, 0x800C, 16396), (0x19, 0xC018, 28684), (0x1E, 0x13024, 2060), (0x27, 0x13830, 15244)]);
    let frames: Vec<usize> = dec.anims.iter().map(|a| a.frames.len()).collect();
    assert_eq!(frames, [16, 8, 16, 8, 17]);

    // Decoded VRAM: matched the oracle 100% outside the five animated
    // rectangles (mr-raw, 2026-10-12). Lock it.
    let mut v = Vram::default();
    dec.apply(&mut v);
    assert_eq!(fnv(&v.px), DOCK_VRAM_FNV, "decoded VRAM hash {:#018x}", fnv(&v.px));
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

const DOCK_VRAM_FNV: u64 = 0xbbd4_06fb_40dc_b272;

#[test]
fn every_raw_on_disc_decodes() {
    let Some(mut d) = disc() else { return };
    let exe = d.read("SLUS_012.37").unwrap();
    let env = LoadEnv::from_exe(&exe, raw::DOCK_COURSE).unwrap();
    let mut names: Vec<String> = d.paths().filter(|p| p.ends_with(".RAW")).map(|s| s.to_string()).collect();
    names.sort();
    assert_eq!(names.len(), 70);
    for n in &names {
        let file = d.read(n).unwrap();
        let rf = raw::parse(&file).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert!(rf.trailing < 8 && file[rf.terminator + 8..].iter().all(|&b| b == 0), "{n}: trailing {:?}", &file[rf.terminator..]);
        let dec = raw::decode_chunks(&rf.chunks, &env).unwrap_or_else(|e| panic!("{n}: {e}"));
        let mut v = Vram::default();
        dec.apply(&mut v);
        if n.starts_with("GFX/") {
            // one 320x256 15-bit image at the second cursor (0x280, 0x100)
            let img: Vec<_> = dec.uploads.iter().filter(|u| u.kind == 0x25).collect();
            assert_eq!(img.len(), 1, "{n}");
            assert_eq!((img[0].x, img[0].y, img[0].w, img[0].h), (0x280, 0x100, 320, 256), "{n}");
        }
        if n.starts_with("FRONTEND/MEM_") {
            let img: Vec<_> = dec.uploads.iter().filter(|u| u.kind == 0x20).collect();
            assert_eq!((img[0].x, img[0].y, img[0].w, img[0].h), (0x40, 0, 512, 256), "{n}");
        }
    }
}

#[test]
fn course_0x1e_skips_reduction() {
    let Some(mut d) = disc() else { return };
    let file = d.read("COURSE_A/DOCK.RAW").unwrap();
    let a = raw::decode(&file, &LoadEnv::new(3, [0; 3], 0x10)).unwrap();
    let b = raw::decode(&file, &LoadEnv::new(0x1E, [0; 3], 0x10)).unwrap();
    // page type 0 differs (reduced vs stored), page type 0x10 does not
    let p = |d: &raw::Decoded, k: u32| d.uploads.iter().find(|u| u.kind == k && u.what == UploadKind::Page).unwrap().data.clone();
    assert_ne!(p(&a, 0), p(&b, 0));
    assert_eq!(p(&a, 0x10), p(&b, 0x10));
}
