//! CD-XA against the owner's disc (skipped when it is absent).

use mr_audio::xa::{self, Step, TrackRef, XaStream};

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
fn exe_tables_name_the_xa_files_and_dock_tracks() {
    let Some(mut d) = disc() else { return };
    let exe = d.read("SLUS_012.37").unwrap();
    let names = xa::exe_xa_file_names(&exe);
    assert_eq!(names.len(), 16);
    assert_eq!(names[0], "TUNES/MUSIC1.XA");
    assert_eq!(names[3], "TUNES/MUSIC4.XA");
    assert_eq!(names[13], "VOICES/FILM_03.XA");
    assert_eq!(names[15], "VOICES/WIN.XA");
    for n in &names {
        assert!(d.entry(n).is_some(), "{n} on disc");
    }
    // DOCK = course 3: music 16 (MUSIC3.XA channel 0), intro clip 0xB0
    // (FILM_03.XA channel 0) - what the oracle's race load played.
    assert_eq!(xa::exe_course_music(&exe, 3), Some(16));
    assert_eq!(xa::exe_course_voice(&exe, 3), Some(0xC0B0));
    let t = TrackRef::from_track_id(0xC0B0);
    assert_eq!(names[t.file_index as usize], "VOICES/FILM_03.XA");
}

#[test]
fn dock_track_loops_at_its_end_marker() {
    let Some(mut d) = disc() else { return };
    let e = d.entry("TUNES/MUSIC3.XA").unwrap().clone();
    let mut st = XaStream::for_track(16, e.lba, (e.size / 2048) as u32, true);
    let mut out = Vec::new();
    let mut audio = 0;
    loop {
        match st.step(&mut d, &mut out) {
            Step::Audio(n) => {
                assert_eq!(n, 2352);
                audio += 1;
            }
            Step::Looped => break,
            Step::Ended => panic!("ended"),
            Step::Skip => {}
        }
    }
    // 2,251 sectors x 2,016 frames at 37,800 Hz = 120.05 s; the marker sits
    // 18,008 sectors into the file.
    assert_eq!(audio, 2251);
    assert_eq!(out.len(), 2251 * 2352);
    assert_eq!(st.lba(), e.lba);
    assert!(out.iter().any(|f| f[0].unsigned_abs() > 8000));
}

/// Phase 11 (docs/spec/audio.md "Phase 11"): the clips and tracks MEET THE
/// MUPPETS and TEST DRIVE played in the oracle (tools/oracle/xa_probe.py,
/// analysis/s8/meet) come from the file / channel the port's track ids
/// name: the first audio sector of the channel is the first LBA the
/// oracle's drive delivered for it.
#[test]
fn gallery_clips_and_test_drive_tracks_match_the_oracle_reads() {
    let Some(mut d) = disc() else { return };
    let exe = d.read("SLUS_012.37").unwrap();
    let names = xa::exe_xa_file_names(&exe);
    // (track id, file, channel, first LBA the oracle delivered)
    let cases = [
        (0x50u16, "VOICES/CHAR_04.XA", 0u8, 192_238u32), // Kermit's idle clip (base 0x50 + 0)
        (0x5C, "VOICES/CHAR_04.XA", 12, 192_250),        // Miss Piggy's (base 0x5C + 0)
        (9, "TUNES/MUSIC2.XA", 1, 127_210),              // TEST DRIVE's random track
        (10, "TUNES/MUSIC2.XA", 2, 127_211),
        (0xDC, "VOICES/WIN.XA", 12, 202_266),          // LAGOON's BOSS CHALLENGE (THE CHIEF)
        (0x53, "VOICES/CHAR_04.XA", 3, 192_241),       // Kermit's battle win (base 0x50 + 3)
    ];
    for (id, file, ch, first) in cases {
        let t = TrackRef::from_track_id(id);
        assert_eq!(names[t.file_index as usize], file, "track {id:#x}");
        assert_eq!(t.channel, ch, "track {id:#x}");
        let e = d.entry(file).unwrap().clone();
        let mut raw = Vec::new();
        let mut lba = e.lba;
        loop {
            raw.clear();
            d.read_raw(lba, 1, &mut raw).unwrap();
            let sh = xa::Subheader::parse(&raw);
            if sh.is_audio() && sh.channel == ch {
                break;
            }
            lba += 1;
        }
        assert_eq!(lba, first, "track {id:#x}: first audio sector of {file} channel {ch}");
    }
    // The results taunt (FUN_800606D0, 0xD0 + n) and the bosses' clips
    // (0xD8..0xDD): VOICES/WIN.XA; nothing names 0xC0..0xCF (CRDIT_01.XA).
    assert_eq!(names[TrackRef::from_track_id(0xD0).file_index as usize], "VOICES/WIN.XA");
    assert_eq!(names[TrackRef::from_track_id(0xDD).file_index as usize], "VOICES/WIN.XA");
    assert_eq!(names[TrackRef::from_track_id(0xC0).file_index as usize], "VOICES/CRDIT_01.XA");
}
