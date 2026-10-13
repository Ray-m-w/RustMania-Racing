//! Listening test: run a mr-sim race session (autopilot, or an oracle route
//! file's buttons) and render its sound effects through the driver + SPU
//! model to a WAV file (analysis/ only; never commit decoded game audio).
//!
//! `cargo run -p mr-audio --example sfx_render -- OUT.wav [--ticks N] [--route ROUTE.json] [--solo]`

use mr_audio::driver::{Consts, Driver};
use mr_audio::vab::Vab;
use mr_sim::sound::SoundCall;

pub fn write_wav(path: &str, pcm: &[i16]) -> std::io::Result<()> {
    let mut b = Vec::with_capacity(44 + pcm.len() * 2);
    let data = (pcm.len() * 2) as u32;
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
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b)
}

fn route_buttons(text: &str) -> Vec<(u64, u16)> {
    let names = ["select", "", "", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"];
    let body = &text[text.find("\"steps\"").expect("steps")..];
    let mut rest = &body[body.find('[').unwrap() + 1..];
    let mut out = Vec::new();
    while let Some(i) = rest.find('[') {
        let end = rest[i..].find(']').unwrap() + i;
        let mut parts = rest[i + 1..end].splitn(2, ',');
        let vb: u64 = parts.next().unwrap().trim().parse().unwrap();
        let spec = parts.next().unwrap_or("").split(',').next().unwrap().trim().trim_matches('"').to_string();
        let mut m = 0u16;
        for b in spec.split('+').filter(|x| !x.is_empty()) {
            m |= 1 << names.iter().position(|n| *n == b).unwrap();
        }
        out.push((vb, m));
        rest = &rest[end + 1..];
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let out = a.get(1).cloned().unwrap_or_else(|| "analysis/audio/sfx_render.wav".into());
    let ticks: u64 = opt("--ticks").map(|v| v.parse().unwrap()).unwrap_or(1800);
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let vab = Vab::parse(&disc.read("SFX/SFX_6.VH").unwrap(), disc.read("SFX/SFX_6.VB").unwrap()).unwrap();
    let consts = Consts::from_exe(&disc.read("SLUS_012.37").unwrap()).unwrap();
    let mut drv = Driver::new(vab, consts);
    let mut s = if a.iter().any(|x| x == "--solo") { mr_sim::session::Session::solo(&mut disc) } else { mr_sim::session::Session::new(&mut disc) }.expect("session");
    let route = opt("--route").map(|p| route_buttons(&std::fs::read_to_string(p).unwrap()));
    if route.is_some() {
        // The oracle's grid savestate (slot 2): race time 1430.
        s.idle_until_race_time(1430).unwrap();
        s.globals.sound.drain();
    } else {
        s.autopilot = true;
    }
    let mut pcm = Vec::new();
    let (mut plays, mut loops) = (0usize, 0usize);
    for k in 0..ticks {
        let b = route.as_ref().map(|r| r.iter().take_while(|(v, _)| *v <= 2 * k).last().map(|x| x.1).unwrap_or(0)).unwrap_or(0);
        if let Err(e) = s.tick(b) {
            eprintln!("tick {k}: {e:?}");
            break;
        }
        for f in s.globals.sound.drain() {
            for c in &f.calls {
                match *c {
                    SoundCall::Play { id, pitch, vol_l, vol_r } => {
                        plays += 1;
                        drv.play(id as i32, pitch as i32, vol_l as i32, vol_r as i32)
                    }
                    SoundCall::Loop { owner, id, pitch, vol, glide } => {
                        loops += 1;
                        drv.loop_sound(owner as i32, id as i32, pitch as i32, vol as i32, glide as i32, f.dt);
                    }
                }
            }
            for _ in 0..f.dt {
                drv.vblank_render(&mut pcm);
            }
        }
    }
    let kons = drv.take_keyons();
    println!("{} ticks, {} one-shots, {} loop refreshes, {} key-ons, {:.1} s", ticks, plays, loops, kons.len(), pcm.len() as f64 / 2.0 / 44_100.0);
    write_wav(&out, &pcm).unwrap();
    println!("wrote {out}");
}
