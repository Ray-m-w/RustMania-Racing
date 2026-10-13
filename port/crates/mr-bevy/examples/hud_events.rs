//! Ticks of the HUD's screens in an autopilot DOCK race (as `mr-bevy
//! --autopilot` runs it): countdown, message banner changes (+0x4FC: LAP 2,
//! FINAL LAP, ...), the player's finish. For HUD screenshots (--fx-shots).
fn main() {
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let cfg = mr_sim::session::SessionConfig::dock_default();
    let mut s = mr_sim::session::Session::with_config(&mut disc, cfg).unwrap();
    s.autopilot = true;
    let (mut msg, mut cd, mut fin) = (0u8, i32::MIN, 0);
    for t in 1..9000u64 {
        if s.tick(0).is_err() {
            break;
        }
        let r = s.record();
        if r[0x4FC] != msg {
            msg = r[0x4FC];
            println!("tick {t}: message {msg:#x} value {}", i16::from_le_bytes([r[0x444], r[0x445]]));
        }
        let c = s.countdown();
        if (c == 0) != (cd == 0) || cd == i32::MIN {
            println!("tick {t}: countdown {c}");
            cd = c;
        }
        if s.kart.finished != 0 && fin == 0 {
            fin = t;
            println!("tick {t}: player finished");
        }
        if fin != 0 && t > fin + 300 {
            break;
        }
    }
}
