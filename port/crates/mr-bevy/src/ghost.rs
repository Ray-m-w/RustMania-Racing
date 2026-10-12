//! The oracle ghost (session 1 suggestion 3): an oracle trace replayed tick
//! for tick next to the Rust kart, drawn translucent, with the planar drift in
//! game units shown in the window title. Same anchor as the trace (tick 0 =
//! the savestate restore), so drift is exactly the comparator's position error.

use bevy::prelude::*;
use mr_trace::Trace;

use crate::{to_bevy, Pose};

#[derive(Resource)]
pub struct Ghost {
    trace: Option<Trace>,
    at: usize,
    pub pose: Option<Pose>,
    prev: Option<Pose>,
}

#[derive(Component)]
pub struct GhostKart;

impl Ghost {
    pub fn new(trace: Option<Trace>) -> Self {
        Ghost { trace, at: 0, pose: None, prev: None }
    }
    pub fn drift(&self, p: &Pose) -> Option<f64> {
        let g = self.pose?;
        Some((((g.x - p.x) as f64).powi(2) + ((g.z - p.z) as f64).powi(2) + ((g.y - p.y) as f64).powi(2)).sqrt())
    }
}

pub fn tick(mut g: ResMut<Ghost>) {
    let g = &mut *g;
    let Some(t) = &g.trace else { return };
    if g.at < t.rows.len() {
        g.prev = g.pose;
        g.pose = Some(Pose::from_row(t, &t.rows[g.at]));
        g.at += 1;
    }
}

pub fn place(g: Res<Ghost>, time: Res<Time<Fixed>>, mut q: Query<(&mut Transform, &mut Visibility), With<GhostKart>>) {
    let a = time.overstep_fraction();
    for (mut t, mut vis) in q.iter_mut() {
        match (g.prev.or(g.pose), g.pose) {
            (Some(p0), Some(p1)) => {
                *vis = Visibility::Visible;
                t.translation = to_bevy(p0.x, p0.y, p0.z).lerp(to_bevy(p1.x, p1.y, p1.z), a);
                t.rotation = p0.rotation().slerp(p1.rotation(), a);
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}
