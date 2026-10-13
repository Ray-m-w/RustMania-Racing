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
