//! Row 12: VIBRATION (pad rumble).
//!
//! The game [CODE 8007d304, 8004357c, 800744ec, 80073138]: each frame
//! FUN_8007D304 gives pad p (karts 0 / 1) the value large = +0x4B7 * a / b
//! & 0xFE while a (+0x4EA, set with b by the hits, landings and bumps that
//! call the race's rumble, mr-sim update.rs `rumble`) runs down by dt,
//! when the pad's VIBRATION is on (save 0x204 + p) and the kart is a
//! human's and has not finished; FUN_8004357C hands it to PadSetAct (a
//! DualShock only, 0x800B0640 = 7). SET CONTROLLERS' VIBRATION row:
//! switching it ON rumbles 0xFF for a = b = 0x1F; while the cursor is on
//! it, ON, and nothing rumbles, a random byte < 8 starts (byte >> 1) + 0x80.
//!
//! The port: mr-sim's session computes the value (`Session::pad_motor`);
//! this system sends it to the player's gamepad (the first / second one,
//! as input.rs pairs them) as a Bevy `GamepadRumbleRequest` (strong motor =
//! large / 254, weak = the small bit). Settings `vibration` /
//! `vibration_2` (ON by default, as the game's new game) are SET
//! CONTROLLERS' rows, mirrored into the save block (save.rs).
//!
//! Note: this build has no gamepad backend (Bevy's gilrs integration is a
//! crate the port does not carry), so there are no `Gamepad` entities and
//! the requests reach nothing until one is added; the trigger points and
//! values are in place.

use std::time::Duration;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;

/// SET CONTROLLERS' test rumble per pad (FUN_800744EC): strength, a, b.
#[derive(Resource, Default)]
pub struct RumbleState {
    test: [(i32, i32, i32); 2],
    last: [u8; 2],
    vb_frac: f32,
    prev_on: [Option<bool>; 2],
    rng: u32,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RumbleState>().add_systems(Update, drive);
}

/// The pad's VIBRATION setting (`vibration` / `vibration_2`).
pub fn enabled(st: &crate::settings::Settings, pad: usize) -> bool {
    let key = if pad == 0 { "vibration" } else { "vibration_2" };
    st.bool(key, true)
}

fn drive(
    p: Res<crate::Player>,
    front: Res<crate::menus::Front>,
    real: Res<Time<Real>>,
    mut st: ResMut<RumbleState>,
    pads: Query<Entity, With<Gamepad>>,
    mut out: MessageWriter<GamepadRumbleRequest>,
) {
    st.vb_frac += real.delta_secs() * 60.0;
    let dt = st.vb_frac.floor() as i32;
    st.vb_frac -= dt as f32;
    if st.rng == 0 {
        st.rng = 0x2545_F491;
    }
    let settings = crate::settings::Settings::global();
    let mut motor = [0u8; 2];
    if front.page == crate::menus::Page::Controllers {
        // the page's own rows (the settings being edited)
        for pad in 0..2 {
            let on = front.port.as_ref().map(|s| enabled(s, pad)).unwrap_or_else(|| enabled(settings, pad));
            let was = st.prev_on[pad].replace(on);
            let t = &mut st.test[pad];
            if on && was == Some(false) {
                *t = (0xFF, 0x1F, 0x1F);
            } else if on && pad == 0 && front.row == 1 && t.1 == 0 {
                st.rng ^= st.rng << 13;
                st.rng ^= st.rng >> 17;
                st.rng ^= st.rng << 5;
                let b = (st.rng & 0xFF) as i32;
                if b < 8 {
                    st.test[pad] = ((b >> 1) + 0x80, 0x1F, 0x1F);
                }
            }
            let t = &mut st.test[pad];
            if on && t.1 > 0 {
                motor[pad] = ((t.0 * t.1 / t.2.max(1)) & 0xFE) as u8;
            }
            t.1 = (t.1 - dt).max(0);
            if !on {
                *t = (0, 0, 0);
            }
        }
    } else {
        st.prev_on = [None; 2];
        st.test = [(0, 0, 0); 2];
        if let crate::Driver::Sim(run) = &p.driver {
            let s = &run.session;
            let racing = front.page == crate::menus::Page::None && !matches!(s.phase(), mr_sim::race::RacePhase::Finished { .. }) && !crate::title::on_title() && !crate::demo::active() && !crate::menus::tool_run();
            let humans = s.config().players.max(1) as usize;
            for (pad, m) in motor.iter_mut().enumerate().take(humans.min(2)) {
                if racing && enabled(settings, pad) {
                    *m = s.pad_motor[pad];
                }
            }
        }
    }
    let mut list: Vec<Entity> = pads.iter().collect();
    list.sort();
    for pad in 0..2 {
        if motor[pad] == st.last[pad] {
            continue;
        }
        st.last[pad] = motor[pad];
        let Some(&gamepad) = list.get(pad) else { continue };
        out.write(GamepadRumbleRequest::Stop { gamepad });
        if motor[pad] != 0 {
            let intensity = GamepadRumbleIntensity { strong_motor: (motor[pad] & 0xFE) as f32 / 254.0, weak_motor: (motor[pad] & 1) as f32 };
            out.write(GamepadRumbleRequest::Add { duration: Duration::from_secs(5), intensity, gamepad });
        }
    }
}
