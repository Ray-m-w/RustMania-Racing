//! ANM decoder tests against the owner's disc (skipped when it is absent).

use mr_assets::anm::{Anm, Channel};
use mr_assets::mdl::Mdl;

fn disc() -> Option<mr_disc::Disc> {
    match mr_disc::Disc::open_default() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("SKIP: no disc ({e:?})");
            None
        }
    }
}

#[test]
fn every_anm_on_disc_parses_and_agrees_with_its_mdl() {
    let Some(mut d) = disc() else { return };
    let mut paths: Vec<String> = d.paths().filter(|p| p.ends_with(".ANM")).map(|s| s.to_string()).collect();
    paths.sort();
    assert_eq!(paths.len(), 65);
    let (mut poses, mut stale) = (0, Vec::new());
    for p in &paths {
        let b = d.read(p).unwrap();
        let a = Anm::parse(&b).unwrap_or_else(|e| panic!("{p}: {e}"));
        // runtime fields are zero on disc
        assert_eq!((a.loaded_flag, a.first_slot, a.end_slot), (0, 0, 0), "{p}");
        assert_eq!(a.blocks.len(), 1, "{p}");
        let blk = a.active_block().unwrap();
        assert_eq!(blk.header[0], -16);
        assert_eq!((blk.header[1], blk.header[2], blk.header[6], blk.header[7]), (3, 2, 2, -1), "{p}");
        assert_eq!(blk.header[3], blk.header[5], "{p}");
        assert_eq!(blk.frames.len(), 2);
        assert_eq!(blk.frames[0], blk.frames[1], "{p}: the two frames are identical");
        let m = Mdl::parse(&d.read(&p.replace(".ANM", ".MDL")).unwrap()).unwrap();
        let meshes: Vec<_> = m.meshes().map(|(_, part)| part).collect();
        assert_eq!(blk.part_count(), meshes.len(), "{p}: one channel per model slot");
        for (i, part) in meshes.iter().enumerate() {
            match (blk.channels[i], &blk.frames[0][i]) {
                (Channel::Seam, None) => assert_ne!(part.record.point_w[1], 0, "{p} part {i}: seam = neck"),
                (Channel::Pose(_), Some(pose)) => {
                    assert_eq!(part.record.point_w[1], 0, "{p} part {i}");
                    poses += 1;
                    if pose.pivot() != part.record.pivot {
                        stale.push(format!("{p}:{i}"));
                    }
                    assert_eq!(pose.pos4.map(|c| c % 4), [0, 0, 0]);
                    assert_eq!(pose.rot[0], 0);
                    assert_eq!(pose.scale.is_some(), blk.scale_mask & (1 << i) != 0);
                }
                other => panic!("{p} part {i}: {other:?}"),
            }
        }
    }
    // pose / 4 equals the MDL pivot for most parts; the rest are stale
    // (all-zero poses, or the MDL moved): more evidence the game never
    // reads the poses (it places parts from the MDL pivots, RAM slots).
    eprintln!("{} of {poses} poses differ from the MDL pivot: {stale:?}", stale.len());
    assert_eq!(poses, 208);
    assert_eq!(stale.len(), 24);
    assert!(!stale.iter().any(|s| s.contains("KERMIT") || s.contains("RACER")));
}

#[test]
fn kermit_and_racer() {
    let Some(mut d) = disc() else { return };
    let k = Anm::parse(&d.read("CHARS_A/KERMIT.ANM").unwrap()).unwrap();
    assert_eq!(k.block_offsets, [16, 0]); // RAM: [0x800ebcec, 0] at DAT_800dd598[7]
    let b = k.active_block().unwrap();
    assert_eq!(b.channels, [Channel::Pose(0), Channel::Pose(5), Channel::Seam]);
    assert_eq!(b.scale_mask, 4);
    assert_eq!(b.seam_parts(), [2]);
    assert_eq!(b.frames[0][0].unwrap().pos4, [0, -580, -112]);

    let r = Anm::parse(&d.read("VEHIC_A/RACER.ANM").unwrap()).unwrap();
    let b = r.active_block().unwrap();
    assert_eq!(b.channels.iter().map(|c| match c { Channel::Pose(o) => *o as i32, Channel::Seam => -2 }).collect::<Vec<_>>(), [0, 5, 10, 18, 26]);
    assert_eq!(b.scale_mask, 0b01100);
    assert_eq!(b.frames[0][2].unwrap().scale, Some([3522; 3]));
    assert_eq!(b.frames[0][0].unwrap().pivot(), [154, -62, -199]); // RAM slot 108 pivot
}
