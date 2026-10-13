//! The SFX bank and libsnd tables from the owner's disc (skipped when no
//! disc is configured: $MRM_DISC or game.toml's `disc = ...`).

use mr_audio::driver::{Consts, Driver};
use mr_audio::vab::Vab;

fn disc() -> Option<mr_disc::Disc> {
    mr_disc::Disc::open_default().ok()
}

fn load() -> Option<(Vab, Consts)> {
    let mut d = disc()?;
    let vh = d.read("SFX/SFX_6.VH").ok()?;
    let vb = d.read("SFX/SFX_6.VB").ok()?;
    let exe = d.read("SLUS_012.37").ok()?;
    Some((Vab::parse(&vh, vb).expect("vab"), Consts::from_exe(&exe).expect("tables")))
}

#[test]
fn bank_layout() {
    let Some((v, _)) = load() else { return };
    assert_eq!(v.version, 6);
    assert_eq!(v.programs.iter().filter(|p| p.tones != 0).count(), 6);
    assert_eq!(v.tones.len(), 6 * 16);
    assert_eq!(v.vag_sizes.len(), 89);
    // The VAG sizes add up to the body.
    assert_eq!(v.vag_sizes.iter().sum::<u32>() as usize, v.body.len());
    // libsnd's addresses (ProgAtr +0xC of the VH copy in RAM: 0x0202 for
    // VAG 1, i.e. the body at 0x1010).
    assert_eq!(v.vag_spu_addr(0x1010, 1), Some(0x0202));
    // Every tone the game can name (ids 1..=88) has a sample, and every
    // sample decodes to its END block.
    for t in 0..88 {
        let tone = v.tone(0, t).unwrap();
        assert!(tone.vag > 0, "tone {t}");
        let d = v.decode_vag(tone.vag as usize).unwrap();
        assert!(!d.pcm.is_empty());
    }
}

#[test]
fn pitch_at_centre_is_unity() {
    let Some((v, c)) = load() else { return };
    let drv = Driver::new(v, c);
    // Note = centre, fine + shift = 0: 44100 Hz (0x1000).
    assert_eq!(drv.pitch_from_note(60, 0, 60, 0), 0x1000);
    // One octave up / down.
    assert_eq!(drv.pitch_from_note(72, 0, 60, 0), 0x2000);
    assert_eq!(drv.pitch_from_note(48, 0, 60, 0), 0x0800);
    // Two octaves up saturate at 0x3FFF.
    assert_eq!(drv.pitch_from_note(84, 0, 60, 0), 0x3FFF);
}
