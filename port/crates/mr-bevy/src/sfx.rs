//! Sound effects and engine sound (track D, docs/spec/audio.md "SFX +
//! engine"). Each fixed tick, after the simulation, the session's sound
//! calls (mr_sim::sound, the game's FUN_8005EECC / FUN_8005F168 calls per
//! race frame) go into mr-audio's driver (the game's sound driver, libsnd and
//! the SPU model); the frame's `dt` VBlanks of driver work and SPU output
//! follow. The SPU voice sum (before the SPU main volume, which the bus
//! applies) waits in a queue that `feed` hands to track C's mixer bus
//! (audio.rs) at the rate the device asks for.

use std::collections::VecDeque;

use bevy::prelude::*;
use mr_audio::driver::{Consts, Driver as SndDriver};
use mr_audio::vab::Vab;
use mr_sim::sound::{SoundCall, SoundFrame};

use crate::audio::AudioBus;
use crate::{Driver, Player};

/// SFX output buffered ahead of the device at most (0.5 s at 44.1 kHz):
/// beyond it the oldest frames are dropped (the simulation ran ahead).
const MAX_PENDING: usize = 22_050;

#[derive(Resource)]
pub struct Sfx {
    /// The bank and tables, kept to restart the driver with a new race.
    bank: Option<(Vab, Consts)>,
    drv: Option<SndDriver>,
    pending: VecDeque<[i16; 2]>,
    /// Session ticks seen last time (a smaller value = a new session).
    last_ticks: u64,
    buf: Vec<i16>,
    /// Set by menus.rs while the session does not tick (course menu,
    /// pause): the driver then runs on real time so the menu sounds play.
    pub idle: bool,
    idle_frac: f32,
}

impl Sfx {
    fn open() -> Sfx {
        let bank = (|| {
            let mut d = mr_disc::Disc::open_default().ok()?;
            let vh = d.read("SFX/SFX_6.VH").ok()?;
            let vb = d.read("SFX/SFX_6.VB").ok()?;
            let exe = d.read("SLUS_012.37").ok()?;
            Some((Vab::parse(&vh, vb).ok()?, Consts::from_exe(&exe)?))
        })();
        if bank.is_none() {
            warn!("sfx: no SFX/SFX_6 bank on the disc; sound effects off");
        }
        let mut s = Sfx { bank, drv: None, pending: VecDeque::new(), last_ticks: 0, buf: Vec::new(), idle: false, idle_frac: 0.0 };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.drv = self.bank.as_ref().map(|(v, c)| {
            let mut d = SndDriver::new(v.clone(), c.clone());
            d.spu.apply_main = false;
            d
        });
        self.pending.clear();
    }

    /// One race frame: its calls, then `dt` VBlanks of driver work + output.
    fn frame(&mut self, f: &SoundFrame) {
        let Some(d) = self.drv.as_mut() else { return };
        for c in &f.calls {
            apply(d, c, f.dt);
        }
        self.buf.clear();
        for _ in 0..f.dt.max(0) {
            d.vblank_render(&mut self.buf);
        }
        self.pending.extend(self.buf.chunks_exact(2).map(|s| [s[0], s[1]]));
        let over = self.pending.len().saturating_sub(MAX_PENDING);
        self.pending.drain(..over);
    }
}

impl Sfx {
    /// FUN_8005EECC(id, pitch, left, right) from the menus (menus.rs).
    pub fn play(&mut self, (id, pitch, l, r): (i32, i32, i32, i32)) {
        if let Some(d) = self.drv.as_mut() {
            d.play(id, pitch, l, r);
        }
    }

    /// `n` VBlanks of driver work and output with no race frame.
    fn idle_vblanks(&mut self, n: i32) {
        let Some(d) = self.drv.as_mut() else { return };
        self.buf.clear();
        for _ in 0..n.max(0) {
            d.vblank_render(&mut self.buf);
        }
        self.pending.extend(self.buf.chunks_exact(2).map(|s| [s[0], s[1]]));
        let over = self.pending.len().saturating_sub(MAX_PENDING);
        self.pending.drain(..over);
    }
}

/// A sound call into the driver (the game's FUN_8005EECC / FUN_8005F168).
pub fn apply(d: &mut SndDriver, c: &SoundCall, dt: i32) {
    match *c {
        SoundCall::Play { id, pitch, vol_l, vol_r } => d.play(id as i32, pitch as i32, vol_l as i32, vol_r as i32),
        SoundCall::Loop { owner, id, pitch, vol, glide } => {
            d.loop_sound(owner as i32, id as i32, pitch as i32, vol as i32, glide as i32, dt);
        }
    }
}

/// FixedUpdate, after the simulation tick: drain the session's sound frames.
fn drain(mut p: ResMut<Player>, mut sfx: ResMut<Sfx>) {
    let Driver::Sim(run) = &mut p.driver else { return };
    let ticks = run.session.ticks;
    if ticks < sfx.last_ticks {
        sfx.reset();
    }
    sfx.last_ticks = ticks;
    for f in run.session.globals.sound.drain() {
        sfx.frame(&f);
    }
}

/// Update: hand the device what it wants from the queue (and, on the
/// menus whose session does not tick, run the driver on real time).
fn feed(bus: Res<AudioBus>, mut sfx: ResMut<Sfx>, real: Res<Time<Real>>) {
    // (phase 9: a movie's skip sound plays while the scene is frozen)
    if sfx.idle || crate::movie::playing() {
        sfx.idle_frac += real.delta_secs() * 60.0;
        let n = sfx.idle_frac.floor();
        sfx.idle_frac -= n;
        sfx.idle_vblanks((n as i32).min(8));
    }
    if !bus.enabled() {
        sfx.pending.clear();
        return;
    }
    let n = bus.sfx_wanted().min(sfx.pending.len());
    if n > 0 {
        let out: Vec<[i16; 2]> = sfx.pending.drain(..n).collect();
        bus.push_sfx(&out);
    }
}

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Sfx::open())
            .add_systems(FixedUpdate, drain.after(crate::sim_tick))
            .add_systems(Update, feed);
    }
}
