//! The constant tables mr-sim reads from SLUS_012.37 on the disc are what
//! the running game has in RAM (oracle ram0.bin of analysis/oracle/lap1P3).

use mr_sim::mem::Mem;
use mr_sim::oracle;
use mr_sim::tables::*;

#[test]
fn exe_tables_match_ram() {
    let root = oracle::repo_root();
    let p = root.join("analysis/oracle/lap1P3/ram0.bin");
    let Ok(ram0) = std::fs::read(&p) else {
        eprintln!("skip: no {}", p.display());
        return;
    };
    std::env::set_current_dir(&root).unwrap();
    let w = oracle::dock_world(&ram0);
    let m = Mem::new(ram0[..0x20_0000].to_vec(), ram0[0x20_0000..0x20_0400].to_vec());
    let t = &w.tables;
    let h = |a: u32, n: usize| (0..n).map(|i| m.i16(a + 2 * i as u32)).collect::<Vec<_>>();
    assert_eq!(t.sin, h(SIN_ADDR, 5120));
    assert_eq!(t.atan, h(ATAN_ADDR, 1025));
    assert_eq!(t.sqrt, h(SQRT_ADDR, 192));
    assert_eq!(t.wall_step, h(WALL_STEP_ADDR, 16));
    for (i, row) in t.surface.iter().enumerate() {
        assert_eq!(&row[..], m.slice(SURFACE_ADDR + 8 * i as u32, 8));
    }
    assert_eq!(&t.weapon_pick[..], m.slice(WEAPON_PICK_ADDR, 32));
    // Vehicle 0 (Kermit's RACER on DOCK): FUN_80087EB4 copies bytes 0..4 of
    // the exe row to the halfwords at 0x800D9134..0x800D913C.
    let v = t.vehicle[0];
    let ram: Vec<i16> = h(0x800D_9134, 5);
    assert_eq!(ram, v[..5].iter().map(|&b| b as i16).collect::<Vec<_>>());
    // The kart on the grid uses vehicle 0.
    assert_eq!(m.u8(0x800C_30A8 + 0x4B1), 0);
}
