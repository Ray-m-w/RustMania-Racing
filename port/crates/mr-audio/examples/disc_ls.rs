//! List disc files whose path contains a filter (default: SFX banks).
//! `cargo run -p mr-audio --example disc_ls -- [FILTER]`
fn main() {
    let f = std::env::args().nth(1).unwrap_or_default().to_ascii_uppercase();
    let d = mr_disc::Disc::open_default().expect("disc");
    let mut v: Vec<_> = d.paths().filter(|p| p.contains(&f)).map(|p| (p.to_string(), d.entry(p).unwrap().size)).collect();
    v.sort();
    for (p, s) in v {
        println!("{s:>10} {p}");
    }
}
