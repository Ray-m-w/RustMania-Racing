//! Particle lists (Session 4, track B): one race frame of FUN_8008727C and
//! FUN_800327D0 on slots recorded from the oracle (race3, items on,
//! tools/oracle/fx_capture.py). The full check over 4,323 frames is
//! `mr-particles analysis/fx/race3b` (docs/spec/effects.md).

use mr_sim::mem::Mem;
use mr_sim::particles::{addr, Particles, Spark, Sparks, Spawn};

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// A list with slot `i` = (entry, side) bytes.
fn list(i: usize, entry: &str, side: &str) -> Particles {
    let mut m = Mem::new(vec![0; 0x20_0000], vec![0; 0x400]);
    m.write(addr::LIST + 0x3C * i as u32, &hex(entry));
    m.write(addr::SIDE + 8 * i as u32, &hex(side));
    Particles::read(&m, vec![])
}

#[test]
fn update_matches_game_frames() {
    let sin = vec![0i16; 4096];
    // type 8, behaviour 4 (fade out over the last quarter, grow to double), dt 2
    let mut a = list(50, "79b8f8ffe0a9000039fe000098ffffffc0ffffffea000000f8ffffffffffffff0000000004004b004b0000000208000408044002b00dfcff8080802e", "8080800c2d002d00");
    let b = list(50, "a9b7f8ff48a900000d00010098ffffffb0ffffffea000000f8ffffffffffffff0000000002005200520000000208000408054002a80dfcff5555552e", "8080800c2d002d00");
    a.update(2, &sin);
    let (mut pa, mut pb) = (a.list[50], b.list[50]);
    pa.flags &= !0x200; // the drawer's on-screen bit
    pb.flags &= !0x200;
    assert_eq!(pa, pb);
    // type 13, behaviour 7 (shrink over the last quarter), dt 2
    let mut a = list(22, "de420e00aad5000088defbff7f00000071000000e4ffffff08000000ffffffff0000000006003000300000001c0d000701006002660c1b008080802c", "8080801830003000");
    let b = list(22, "dc430e00a4d6000050defbff7f00000081000000e4ffffff08000000ffffffff0000000004002000200000001c0d0007010060029c0c1b008080802c", "8080801830003000");
    a.update(2, &sin);
    let (mut pa, mut pb) = (a.list[22], b.list[22]);
    pa.flags &= !0x200;
    pb.flags &= !0x200;
    assert_eq!(pa, pb);
}

#[test]
fn spawn_ring_and_death() {
    let mut p = Particles { types: vec![[0; 4], [1, 1, 0x20, 3]], ..Particles::from_exe(&vec![0u8; 0xA2000]) };
    p.next = 0x3F;
    let s = Spawn { pos: [1, 2, 3], vel: [0, 0, 0], gravity: 0, angle: 0, spin: 0, kind: 1, rgb: [100, 50, 25], w: 8, h: 8, life: 4 };
    // pre-increment, wraps to 0
    assert_eq!(p.spawn(&s), Some(0));
    assert_eq!(p.list[0].code, 0x2E);
    assert_eq!(p.list[0].flags, 0x220);
    // a zero size spawns nothing
    assert_eq!(p.spawn(&Spawn { w: 0, ..s }), None);
    let sin = vec![0i16; 4096];
    p.update(2, &sin);
    assert_eq!(p.list[0].life, 2);
    p.update(2, &sin);
    // life 0 -> -1 -> freed in the same call (sprite cleared)
    assert_eq!(p.list[0].life, 0);
    assert_eq!(p.list[0].sprite, 0);
}

#[test]
fn sparks_move_and_fade() {
    let mut s = Sparks::default();
    s.spawn([0, 0, 0], [10, -3, 5], 9, 16, [255, 127, 0]);
    s.update(2);
    let k: Spark = s.list[0];
    assert_eq!((k.pos, k.life, k.rgb), ([20, -6, 10], 7, [223, 111, 0]));
    // dead slots stay 0xFF
    assert_eq!(s.list[1].life, 0xFF);
    for _ in 0..3 {
        s.update(2);
    }
    assert_eq!(s.list[0].life, 1);
    s.update(2);
    assert_eq!(s.list[0].life, 0xFF);
}
