//! Print the player's state around and after the finish of a capture.
use mr_sim::kart::KART_ARRAY;
use mr_sim::oracle::{self, Capture};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cap = Capture::open(&a[1]).unwrap();
    let (from, to, step): (u32, u32, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap(), a.get(4).map(|s| s.parse().unwrap()).unwrap_or(1));
    println!("regions {:?}", cap.regions.iter().map(|(a, n)| format!("{a:08X}+{n:X}")).collect::<Vec<_>>());
    let base = cap.ram0.clone();
    for &vb in cap.vbs.iter().filter(|&&v| v >= from && v <= to).step_by(step) {
        let m = cap.mem_at(vb, base.as_deref());
        let k = |o: u32| KART_ARRAY + o;
        println!(
            "vb {vb} hm {:#x} ck {} st {:#x} ht {} eff {} tx {} ty {} x {} y {} z {} h {} fwd {} spd2F4 {} btn {:#x} kbtn {:#x} flags {:#x} 358 {} 2c {} ce0 {} a64 {} af0 {} 4FF {} 4CA {} 4FC {} catch {} 4DF {} mode {} ",
            m.u16(k(0x410)), m.u8(k(0x501)), m.u32(k(0x354)), m.i16(k(0x468)), m.i8(k(0x4F1)), m.i32(k(0x300)), m.i32(k(0x304)), m.i32(k(0)), m.i32(k(4)), m.i32(k(8)), m.i16(k(0xE)), m.i16(k(0x2EC)), m.i32(k(0x2F4)), m.u16(0x800B_0CE8), m.u16(k(0x4A0)), m.u16(k(0x496)), m.i32(k(0x358)),
            m.i32(0x800B_0A94), m.u8(0x800B_0CE0), m.i32(0x800B_0A64), m.i32(0x800B_0AF0), m.u8(k(0x4FF)), m.u8(k(0x4CA)), m.u8(k(0x4FC)), m.u8(k(0x4C3)), m.u8(k(0x4DF)), m.i16(0x800B_0D46)
        );
    }
}
