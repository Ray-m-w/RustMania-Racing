//! Scan an oracle capture's race flow: per change, the player's lap,
//! finished flag, flags, position, progress, race time and the globals.
use mr_sim::kart::KART_ARRAY;
use mr_sim::oracle::{self, Capture};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let cap = Capture::open(&a[1]).unwrap();
    let base = cap.ram0.clone();
    let mut last = String::new();
    for &vb in &cap.vbs {
        let m = cap.mem_at(vb, base.as_deref());
        let k = |o: u32| KART_ARRAY + o;
        let s = format!(
            "rt {} lap {} fin {} flags {:#x} c70 {} mode {} cd8 {} 4D6 {} race_over {}",
            m.i32(k(0x358)), m.u8(k(0x4BF)), m.u8(k(0x4C7)), m.u16(k(0x496)),
            m.i32(0x800B_0C70), m.i16(0x800B_0D46), m.u8(0x800B_0CD8), m.u8(k(0x4D6)), m.u8(0x800B_0BD2)
        );
        if s != last {
            println!("vb {vb:5} clock {:6} rt {:6} best {:5} prog {:5} spd {:5} | {s}", m.i32(0x800B_0A94), m.i32(k(0x358)), m.i32(k(0x384)), m.i32(k(0x388)), m.i32(k(0x2F4)));
            last = s;
        }
    }
}
