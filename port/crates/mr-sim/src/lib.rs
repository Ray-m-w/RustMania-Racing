//! mr-sim: the player kart's per-tick update, ported from the game's code.
//!
//! Scope (docs/spec/PHYSICS-AGENT-BRIEF.md): FUN_80064B5C(kart, buttons, 0)
//! and FUN_80086B7C(kart) with everything they call on the paths a solo
//! human kart takes on DOCK. Integers exactly as the game: 16.16-ish world
//! positions, 12-bit angles, 4.12 sines, MIPS truncating division. No floats,
//! no I/O inside the simulation; constant tables come from the executable on
//! the owner's disc (`tables`), terrain from mr_assets::ter.
//!
//! Model and code addresses: docs/spec/kart-physics.md. Every deliberate
//! difference and stubbed branch: docs/spec/kart-physics-ledger.md.

/// A branch the solo player never takes on DOCK (items, weapons, AI,
/// multiplayer, boss, special moves, ...). Reaching it panics; each use is
/// listed in docs/spec/kart-physics-ledger.md.
#[macro_export]
macro_rules! unreachable_in_scope {
    ($why:expr) => {
        panic!("unreachable_in_scope: {}", $why)
    };
}

pub mod body;
pub mod camera;
pub mod collide;
pub mod fixed;
pub mod globals;
pub mod harness;
pub mod init;
pub mod ground;
pub mod kart;
pub mod mem;
pub mod oracle;
pub mod progress;
pub mod render;
pub mod session;
pub mod tables;
pub mod update;
pub mod vehicle;
pub mod water;
pub mod world;

pub use globals::RaceGlobals;
pub use kart::KartState;
pub use world::{TickInputs, World};

/// One simulation tick for the human player's kart: FUN_80064B5C(kart,
/// buttons, player) followed by FUN_80086B7C(kart) (the race frame
/// FUN_8007916C calls both, in that order, for each human kart).
/// `buttons` is the active-high PS1 pad word the game saw this tick.
pub fn tick(k: &mut KartState, g: &mut RaceGlobals, w: &World, inp: &TickInputs, buttons: u16) {
    update::kart_update(k, g, w, inp, buttons, 0);
    progress::track_direction(k, g, w);
}

/// A race-frame tick with other karts on the track: the kart-kart contact
/// pass FUN_800418D8 (start of the frame; `others` = karts 1.. in the state
/// they ended the previous frame with), then [`tick`]. Changes made to
/// `others` are the game's too, but only kart 0's side is verified.
pub fn tick_with_karts(
    k: &mut KartState,
    others: &mut [KartState],
    g: &mut RaceGlobals,
    w: &World,
    inp: &TickInputs,
    rules: &collide::ContactRules,
    buttons: u16,
) {
    collide::frame_contacts(k, others, g, w, rules);
    // FUN_80041A20: the course look points (FUN_80041CC4 is checked by the
    // caller, which knows the camera mode).
    render::aim_at_course(k, g, w);
    if let Some([x, y, z, d]) = inp.aim {
        k.aim_x = x;
        k.aim_y = y;
        k.aim_z = z;
        k.aim_dist = d;
    }
    tick(k, g, w, inp, buttons);
    // After the kart loop: FUN_80025D8C rebuilds the body matrix.
    k.body = body::body_matrix(&w.tables, k);
}
