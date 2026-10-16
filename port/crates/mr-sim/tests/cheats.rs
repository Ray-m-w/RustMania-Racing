//! Phase 8: the cheat codes (docs/spec/frontend.md "Cheat codes"): typed on
//! the title with the face buttons, each changes the save block.

use mr_sim::save::{Cheats, SaveBlock, SaveTables};
use mr_sim::tables::ExeImage;

fn exe() -> ExeImage {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    let mut d = mr_disc::Disc::open_default().unwrap();
    ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap()
}

/// Press each button of `seq` for one frame, release for one.
fn type_code(c: &mut Cheats, seq: &[u8]) -> Option<u8> {
    let mut got = None;
    for &b in seq {
        let pad = [0x8000u16, 0x1000, 0x2000, 0x4000][b as usize];
        if let Some(k) = c.frame(pad) {
            got = Some(k);
        }
        c.frame(pad);
        c.frame(0);
    }
    got
}

#[test]
fn codes_are_recognised() {
    let exe = exe();
    let mut c = Cheats::from_exe(&exe);
    c.frame(0);
    // END COURSES: CIRCLE TRIANGLE SQUARE TRIANGLE CROSS TRIANGLE SQUARE
    // CIRCLE TRIANGLE CROSS
    assert_eq!(c.codes[5], [2, 1, 0, 1, 3, 1, 0, 2, 1, 3]);
    for code in 1..7u8 {
        let row = c.codes[code as usize];
        assert_eq!(type_code(&mut c, &row), Some(code), "code {code}");
    }
    // a wrong press in the middle: nothing; row 0 is never taken
    let mut bad = c.codes[2];
    bad[4] = (bad[4] + 1) % 4;
    assert_eq!(type_code(&mut c, &bad), None);
    let row0 = c.codes[0];
    assert_eq!(type_code(&mut c, &row0), None);
    // two buttons at once count as nothing
    assert_eq!(c.frame(0x8000 | 0x1000), None);
}

#[test]
fn code_effects() {
    let exe = exe();
    let t = SaveTables::from_exe(&exe);
    let mut r = 1u32;
    let mut rand = || {
        r = r.wrapping_mul(1103515245).wrapping_add(12345);
        ((r >> 16) & 0x7FFF) as i32
    };
    let new = SaveBlock::new_game(&t, &mut rand);
    let mut s = new.clone();
    s.cheat(5, &t);
    assert_eq!(s.secrets(), 3, "END COURSES");
    let mut s = new.clone();
    s.cheat(4, &t);
    assert!((0..6).all(|w| (0..4).all(|k| s.flags(w, k) & 0x20 != 0)));
    assert!(s.course_open(6, 2) && s.char_open(24), "FRAGGLE ROCK and the Doozer");
    let mut s = new.clone();
    s.cheat(1, &t);
    assert_eq!(s.chars(), 0xFFFF_FFFF);
    s.cheat(6, &t);
    assert_eq!((s.chars(), s.secrets()), (0xFF, 0), "RESET");
    assert_eq!(s.flags(0, 0), new.flags(0, 0));
}

/// The ARCHES code typed on the oracle's title (tools/oracle/routes/
/// cheat_arches.json; analysis/oracle/cheats/arches, local): the save
/// block after it equals the port's `cheat(3)` of the block before.
#[test]
fn arches_matches_oracle() {
    let exe = exe();
    let root = mr_sim::oracle::repo_root().join("analysis/oracle/cheats/arches");
    let (Ok(before), Ok(after)) = (std::fs::read(root.join("block_ram0.bin")), std::fs::read(root.join("block_after.bin"))) else { return };
    let t = SaveTables::from_exe(&exe);
    let mut b = [0u8; mr_sim::save::LEN];
    b.copy_from_slice(&before[..mr_sim::save::LEN]);
    let mut s = SaveBlock(b);
    s.cheat(3, &t);
    let diff: Vec<usize> = (0..after.len()).filter(|&i| s.0[i] != after[i]).collect();
    assert!(diff.is_empty(), "bytes differ at {diff:x?}");
    assert_ne!(&before[..after.len()], &after[..], "the code changed the block");
}
