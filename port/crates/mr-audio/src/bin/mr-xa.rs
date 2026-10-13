//! mr-xa: inspect and decode the disc's CD-XA music, and compare the decoder
//! with the oracle's cd_in audio tap (tools/audio/music_probe.py dumps it).
//!
//!   mr-xa info                                   files, channels, lengths, end markers
//!   mr-xa decode TRACK OUT.wav [--seconds S] [--mix V]
//!   mr-xa compare TRACK ORACLE.wav [--mix V] [--loops]
//!
//! TRACK is the game's track id (DOCK = 16). --mix is the CdMix level byte
//! (the game's music level 0x800B0A50; 0x6C in the oracle), decimal or 0x..
//! Output WAVs are decoded game audio: write them under analysis/
//! (gitignored), never commit them.

use mr_audio::xa::{self, CdMix, Step, Subheader, TrackRef, XaStream};
use mr_disc::Disc;

fn opt(args: &[String], k: &str) -> Option<String> {
    args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned()
}

fn num(s: &str) -> u32 {
    match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).expect("hex"),
        None => s.parse().expect("number"),
    }
}

fn music_file(d: &Disc, t: TrackRef) -> (u32, u32) {
    let p = t.music_path().expect("music track (id < 0x20)");
    let e = d.entry(&p).expect("music file on disc");
    (e.lba, (e.size / 2048) as u32)
}

fn write_wav(path: &str, frames: &[[i16; 2]]) {
    let mut b = Vec::with_capacity(44 + frames.len() * 4);
    let data = (frames.len() * 4) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&44_100u32.to_le_bytes());
    b.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for f in frames {
        b.extend_from_slice(&f[0].to_le_bytes());
        b.extend_from_slice(&f[1].to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}

fn read_wav(path: &str) -> Vec<[i16; 2]> {
    let b = std::fs::read(path).expect("read wav");
    let mut o = 12;
    while o + 8 <= b.len() {
        let id = &b[o..o + 4];
        let n = u32::from_le_bytes(b[o + 4..o + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            let d = &b[o + 8..(o + 8 + n).min(b.len())];
            return d
                .chunks_exact(4)
                .map(|c| [i16::from_le_bytes([c[0], c[1]]), i16::from_le_bytes([c[2], c[3]])])
                .collect();
        }
        o += 8 + n + (n & 1);
    }
    panic!("no data chunk in {path}");
}

/// Any track id through the exe's XA file table (music and voices).
fn track_file(d: &mut Disc, track: u16) -> (u32, u32) {
    let exe = d.read("SLUS_012.37").expect("exe");
    let names = xa::exe_xa_file_names(&exe);
    let t = TrackRef::from_track_id(track);
    let e = d.entry(&names[t.file_index as usize]).expect("XA file on disc");
    (e.lba, (e.size / 2048) as u32)
}

fn decode(d: &mut Disc, track: u16, frames: usize, mix: CdMix, looping: bool) -> (Vec<[i16; 2]>, Vec<usize>) {
    let (lba, n) = track_file(d, track);
    let mut st = XaStream::for_track(track, lba, n, looping);
    let mut out = Vec::new();
    let mut loop_at = Vec::new();
    while out.len() < frames {
        match st.step(d, &mut out) {
            Step::Ended => break,
            Step::Looped => loop_at.push(out.len()),
            _ => {}
        }
    }
    for f in out.iter_mut() {
        *f = mix.apply(*f);
    }
    (out, loop_at)
}

struct Cmp {
    n: usize,
    exact: usize,
    maxd: i32,
    corr: f64,
    first_diff: Option<(usize, [i16; 2], [i16; 2])>,
}

fn compare(mine: &[[i16; 2]], orc: &[[i16; 2]], off: i64) -> Cmp {
    let mut c = Cmp { n: 0, exact: 0, maxd: 0, corr: 0.0, first_diff: None };
    let (mut sxy, mut sxx, mut syy) = (0f64, 0f64, 0f64);
    for (i, m) in mine.iter().enumerate() {
        let j = i as i64 + off;
        if j < 0 || j as usize >= orc.len() {
            continue;
        }
        let o = orc[j as usize];
        c.n += 1;
        if o == *m {
            c.exact += 1;
        } else if c.first_diff.is_none() {
            c.first_diff = Some((i, *m, o));
        }
        for k in 0..2 {
            let (x, y) = (m[k] as f64, o[k] as f64);
            c.maxd = c.maxd.max((m[k] as i32 - o[k] as i32).abs());
            sxy += x * y;
            sxx += x * x;
            syy += y * y;
        }
    }
    c.corr = sxy / (sxx.sqrt() * syy.sqrt()).max(1e-9);
    c
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut d = Disc::open_default().expect("open disc (MRM_DISC or game.toml)");
    match args.get(1).map(|s| s.as_str()) {
        Some("info") => {
            for i in 0..4u16 {
                let t = TrackRef::from_track_id(i * 8);
                let (lba, n) = music_file(&d, t);
                println!("{} lba {} sectors {}", t.music_path().unwrap(), lba, n);
                let mut raw = Vec::new();
                d.read_raw(lba, n, &mut raw).expect("read");
                for ch in 0..8u8 {
                    let mut audio = 0;
                    let mut eof = None;
                    let mut end = None;
                    for s in 0..n as usize {
                        let r = &raw[s * xa::RAW..(s + 1) * xa::RAW];
                        let sh = Subheader::parse(r);
                        if sh.is_audio() && sh.channel == ch && sh.file == xa::GAME_XA_FILE {
                            audio += 1;
                            if sh.is_eof() {
                                eof = Some(s);
                            }
                        } else if end.is_none() && xa::is_end_marker(r, ch) {
                            end = Some(s);
                        }
                    }
                    println!(
                        "  track {:2} ch {} audio sectors {:5} ({:6.2} s) eof @{:?} end marker @{:?}",
                        i * 8 + ch as u16,
                        ch,
                        audio,
                        audio as f64 * 2016.0 / 37_800.0,
                        eof,
                        end
                    );
                }
            }
        }
        Some("decode") => {
            let track = num(&args[2]) as u16;
            let secs: f64 = opt(&args, "--seconds").map(|s| s.parse().unwrap()).unwrap_or(30.0);
            let mix = opt(&args, "--mix").map(|v| CdMix::level(num(&v) as u8)).unwrap_or(CdMix::UNITY);
            let (out, loops) = decode(&mut d, track, (secs * 44_100.0) as usize, mix, true);
            write_wav(&args[3], &out);
            println!("wrote {} frames ({:.2} s), loops at {:?}", out.len(), out.len() as f64 / 44_100.0, loops);
        }
        Some("compare") => {
            let track = num(&args[2]) as u16;
            let mix = opt(&args, "--mix").map(|v| CdMix::level(num(&v) as u8)).unwrap_or(CdMix::level(0x6C));
            let orc = read_wav(&args[3]);
            let looping = args.iter().any(|a| a == "--loops");
            let (mine, loops) = decode(&mut d, track, orc.len() + 44_100 * 5, mix, looping);
            // Anchor: 64 frames of the track searched exactly in the oracle tap
            // (track start first, else any point of the first 150 s).
            let k0 = mine.iter().position(|f| f[0] != 0 || f[1] != 0).unwrap_or(0);
            let mut found = None;
            let mut k = k0;
            while k + 64 < mine.len().min(44_100 * 150) {
                let pat = &mine[k..k + 64];
                if let Some(at) = orc.windows(64).position(|w| w == pat) {
                    found = Some((k, at));
                    break;
                }
                k += 4410;
            }
            let Some((k, at)) = found else {
                println!("no 64-frame window of the track found in the oracle WAV ({} frames)", orc.len());
                return;
            };
            let off = at as i64 - k as i64;
            println!(
                "anchor: track frame {k} = oracle frame {at}; track frame 0 at oracle {off} ({:.3} s); loops at {:?}",
                off as f64 / 44_100.0,
                loops
            );
            let c = compare(&mine, &orc, off);
            println!(
                "overlap {} frames ({:.2} s): exact {} ({:.4}%), max |diff| {}, correlation {:.6}, first diff {:?}",
                c.n,
                c.n as f64 / 44_100.0,
                c.exact,
                100.0 * c.exact as f64 / c.n.max(1) as f64,
                c.maxd,
                c.corr,
                c.first_diff
            );
        }
        Some("segments") => {
            // Piecewise: walk the oracle tap and find each run of frames that
            // equals the track decode (any alignment that starts on a decoded
            // sector boundary); report runs, gaps, and coverage.
            let track = num(&args[2]) as u16;
            let mix = opt(&args, "--mix").map(|v| CdMix::level(num(&v) as u8)).unwrap_or(CdMix::level(0x6C));
            let orc = read_wav(&args[3]);
            let (mine, _) = decode(&mut d, track, 44_100 * 125, mix, false);
            let key = |w: &[[i16; 2]]| -> u64 {
                let mut h = 0xcbf29ce484222325u64;
                for f in w {
                    for v in f {
                        h = (h ^ (*v as u16 as u64)).wrapping_mul(0x100000001b3);
                    }
                }
                h
            };
            let mut map = std::collections::HashMap::new();
            for k in (0..mine.len().saturating_sub(16)).step_by(2352) {
                if mine[k..k + 16].iter().any(|f| f[0] != 0 || f[1] != 0) {
                    map.entry(key(&mine[k..k + 16])).or_insert(k);
                }
            }
            let (mut p, mut matched, mut unmatched_run, mut runs) = (0usize, 0usize, 0usize, 0usize);
            let mut last_end: Option<(usize, usize)> = None;
            while p + 16 <= orc.len() {
                if let Some(&k) = map.get(&key(&orc[p..p + 16])) {
                    let mut n = 0;
                    while p + n < orc.len() && k + n < mine.len() && orc[p + n] == mine[k + n] {
                        n += 1;
                    }
                    if n >= 64 {
                        let note = match last_end {
                            Some((po, pk)) if po == p && pk == k => String::new(),
                            Some((po, pk)) => format!(" (after {} unmatched oracle frames; track jump {:+} frames)", p - po, k as i64 - pk as i64),
                            None => format!(" (first; {} oracle frames before)", p),
                        };
                        if !note.is_empty() || runs == 0 {
                            println!("oracle {:8} ({:7.3} s) = track {:8} ({:7.3} s) for {:8} frames{}", p, p as f64 / 44_100.0, k, k as f64 / 44_100.0, n, note);
                        }
                        runs += 1;
                        matched += n;
                        p += n;
                        last_end = Some((p, k + n));
                        unmatched_run = 0;
                        continue;
                    }
                }
                unmatched_run += 1;
                p += 1;
            }
            let _ = unmatched_run;
            println!("matched {} of {} oracle frames ({:.3}%)", matched, orc.len(), 100.0 * matched as f64 / orc.len() as f64);
        }
        _ => eprintln!("usage: mr-xa info | decode TRACK OUT.wav [--seconds S] [--mix V] | compare TRACK ORACLE.wav [--mix V] [--loops]"),
    }
}
