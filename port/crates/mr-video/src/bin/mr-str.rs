//! mr-str: the game's STR movies from the command line (docs/spec/fmv.md).
//!
//!   mr-str info                      every movie: frames, size, audio, decode check
//!   mr-str png MOVIE FRAME OUT.png   one decoded frame (for looking; never commit)
//!   mr-str wav MOVIE OUT.wav         its audio at 44.1 kHz (listening tests; never commit)
//!   mr-str vram MOVIE DUMP.bin X Y W H [FRAME..]
//!       compare an oracle VRAM read (raw 16-bit halfwords, W x H at X, Y;
//!       the movie's 24-bit picture at VRAM (0, y_offset) or (0, 240 +
//!       y_offset)) with the decoded frames: exact bytes per frame
//!
//! MOVIE is the game's movie number (0..30, FMV.BIN's table).

use mr_video::{mdec::Mdec, movies, Item, StrReader};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let fmv = disc.read("FMV/FMV.BIN").expect("FMV/FMV.BIN");
    let table = movies::table(&fmv);
    let m = Mdec::default();
    match a.get(1).map(String::as_str) {
        Some("info") => {
            for (i, mv) in table.iter().enumerate() {
                let Some(mut r) = StrReader::open(&disc, &mv.path) else {
                    println!("{i:#04x} {:<18} (no file)", mv.path);
                    continue;
                };
                let (mut frames, mut bad, mut audio, mut size) = (0, 0, 0usize, (0, 0));
                let mut rgb = Vec::new();
                while let Some(it) = r.next(&mut disc).expect("read") {
                    match it {
                        Item::Frame(f) => {
                            frames += 1;
                            size = (f.width, f.height);
                            if f.decode(&m, &mut rgb).is_err() {
                                bad += 1;
                            }
                        }
                        Item::Audio { pcm, .. } => audio += pcm.len(),
                    }
                }
                println!(
                    "{i:#04x} {:<18} {:>5} sectors {:>4} frames (end {:>4}) {}x{} {:.1} fps, audio {:.2} s, {} bad",
                    mv.path,
                    r.sectors(),
                    frames,
                    mv.end,
                    size.0,
                    size.1,
                    frames as f64 * 150.0 / r.sectors() as f64,
                    audio as f64 / 44100.0,
                    bad
                );
            }
        }
        Some("png") => {
            let (mv, n, out) = (num(&a[2]) as usize, num(&a[3]), &a[4]);
            let f = frame(&mut disc, &table[mv].path, n).expect("no such frame");
            let mut rgb = Vec::new();
            f.decode(&m, &mut rgb).expect("decode");
            std::fs::write(out, png(f.width as u32, f.height as u32, &rgb)).expect("write");
        }
        Some("wav") => {
            let mv = num(&a[2]) as usize;
            let mut r = StrReader::open(&disc, &table[mv].path).expect("file");
            let mut pcm: Vec<[i16; 2]> = Vec::new();
            while let Some(it) = r.next(&mut disc).expect("read") {
                if let Item::Audio { pcm: p, .. } = it {
                    pcm.extend(p);
                }
            }
            let mut w = Vec::new();
            let n = (pcm.len() * 4) as u32;
            w.extend(b"RIFF");
            w.extend((36 + n).to_le_bytes());
            w.extend(b"WAVEfmt ");
            w.extend(16u32.to_le_bytes());
            w.extend([1, 0, 2, 0]);
            w.extend(44_100u32.to_le_bytes());
            w.extend((44_100u32 * 4).to_le_bytes());
            w.extend([4, 0, 16, 0]);
            w.extend(b"data");
            w.extend(n.to_le_bytes());
            for f in &pcm {
                w.extend(f[0].to_le_bytes());
                w.extend(f[1].to_le_bytes());
            }
            std::fs::write(&a[3], w).expect("write");
        }
        Some("vram") => {
            let mv = num(&a[2]) as u8;
            let dump = std::fs::read(&a[3]).expect("dump");
            let (x, y, w, h) = (num(&a[4]) as usize, num(&a[5]) as usize, num(&a[6]) as usize, num(&a[7]) as usize);
            let wanted: Vec<u32> = a[8..].iter().map(|s| num(s)).collect();
            let yo = movies::y_offset(mv) as usize;
            let mut r = StrReader::open(&disc, &table[mv as usize].path).expect("file");
            let mut rgb = Vec::new();
            while let Some(it) = r.next(&mut disc).expect("read") {
                let Item::Frame(f) = it else { continue };
                if !wanted.is_empty() && !wanted.contains(&f.number) {
                    continue;
                }
                f.decode(&m, &mut rgb).expect("decode");
                let fw = f.width as usize * 3;
                for buf in [0usize, 240] {
                    // VRAM bytes at row y + j, byte column 2x .. 2(x + w);
                    // each picture level (the player's fades) tried
                    let mut best = (0usize, 0usize, 8u8);
                    for level in (0..=8u8).rev() {
                        let mut d = rgb.clone();
                        movies::darken(&mut d, level);
                        let (mut same, mut n) = (0usize, 0usize);
                        for j in 0..h {
                            let Some(py) = (y + j).checked_sub(buf + yo) else { continue };
                            if py >= f.height as usize {
                                continue;
                            }
                            for i in 0..w * 2 {
                                let bx = x * 2 + i;
                                if bx >= fw {
                                    continue;
                                }
                                n += 1;
                                same += (dump[(j * w) * 2 + i] == d[py * fw + bx]) as usize;
                            }
                        }
                        if same > best.0 {
                            best = (same, n, level);
                        }
                    }
                    let (same, n, level) = best;
                    if n > 0 && (same * 10 > n * 9 || !wanted.is_empty()) {
                        println!("frame {:>4} buffer {buf:>3} level {level}: {same}/{n} bytes equal", f.number);
                    }
                }
            }
        }
        _ => eprintln!("usage: mr-str info | png MOVIE FRAME OUT.png | wav MOVIE OUT.wav | vram MOVIE DUMP X Y W H [FRAME..]"),
    }
}

fn num(s: &str) -> u32 {
    s.strip_prefix("0x").map(|h| u32::from_str_radix(h, 16).unwrap()).unwrap_or_else(|| s.parse().unwrap())
}

fn frame(disc: &mut mr_disc::Disc, path: &str, n: u32) -> Option<mr_video::Frame> {
    let mut r = StrReader::open(disc, path)?;
    while let Some(it) = r.next(disc).ok()? {
        if let Item::Frame(f) = it {
            if f.number == n {
                return Some(f);
            }
        }
    }
    None
}

/// A minimal PNG (stored deflate blocks), RGB 8-bit.
fn png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    fn crc(d: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in d {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, ty: &[u8], d: &[u8]) {
        out.extend((d.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend(d);
        out.extend(&c);
        out.extend(crc(&c).to_be_bytes());
    }
    let mut raw = Vec::new();
    for row in rgb.chunks_exact(w as usize * 3) {
        raw.push(0);
        raw.extend(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in &raw {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    z.extend((s2 << 16 | s1).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ih = Vec::new();
    ih.extend(w.to_be_bytes());
    ih.extend(h.to_be_bytes());
    ih.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ih);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
