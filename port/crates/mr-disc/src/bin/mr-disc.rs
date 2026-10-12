//! mr-disc list | mr-disc get <PATH> <OUT>   (disc from $MRM_DISC or game.toml)
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut d = mr_disc::Disc::open_default().expect("open disc");
    match args.get(1).map(|s| s.as_str()) {
        Some("list") => {
            let mut p: Vec<_> = d.paths().map(String::from).collect();
            p.sort();
            for x in p {
                let e = d.entry(&x).unwrap();
                println!("{}\t{}\t{}", x, e.lba, e.size);
            }
        }
        Some("get") => {
            let b = d.read(&args[2]).expect("read");
            std::fs::write(&args[3], b).expect("write");
        }
        _ => eprintln!("usage: mr-disc list | get <PATH> <OUT>"),
    }
}
