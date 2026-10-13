//! Race-load facts from a capture that starts in the menus: per kart slot
//! the grid-relevant fields, the round-robin counter and the random pointer.
use mr_sim::kart::KART_ARRAY;
use mr_sim::oracle::{self, Capture};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cap = Capture::open(&a[1]).unwrap();
    for &vb in cap.vbs.iter().filter(|&&v| v >= 630 && v <= 670) {
        let m = cap.mem_at(vb, None);
        let rr: Vec<u8> = (0..8).map(|i| m.u8(0x800C_3058 + i)).collect();
        print!("vb {vb} c98 {} dfe {} c70 {} rng {:08X} seed {:08X} rr {:?} |", m.i16(0x800B_0C98), m.i16(0x800B_0DFE), m.i32(0x800B_0C70), m.u32(0x800B_0BCC), m.u32(0x8000_9010), rr);
        for s in 0..6u32 {
            let b = KART_ARRAY + 0x504 * s;
            print!(" {}:{}/{}", s, m.u8(b + 0x4CB), m.i16(b + 0x49A));
        }
        println!();
    }
}
