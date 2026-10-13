//! Write one disc file to a path (analysis/ only; never commit game data).
//! `cargo run -p mr-audio --example disc_cat -- SFX/SFX_6.VH OUT`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut d = mr_disc::Disc::open_default().expect("disc");
    std::fs::write(&a[2], d.read(&a[1]).expect("read")).unwrap();
}
