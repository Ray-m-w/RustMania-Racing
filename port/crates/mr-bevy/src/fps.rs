//! --fps-log SECONDS: measure display frame pacing (frame-to-frame real time)
//! and fixed-tick rate, print a summary and exit. SESSION-3 item 4.

use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct FpsLog {
    pub secs: f32,
    pub frames: Vec<f32>,
    pub ticks: u32,
    pub started: Option<f32>,
}

pub fn plugin(app: &mut App) {
    let a: Vec<String> = std::env::args().collect();
    let Some(secs) = a.iter().position(|x| x == "--fps-log").and_then(|i| a.get(i + 1)).and_then(|s| s.parse::<f32>().ok()) else {
        return;
    };
    // full rate even without focus (Bevy drops an unfocused window to 60 Hz)
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(FpsLog { secs, ..default() })
        .add_systems(FixedUpdate, |mut l: ResMut<FpsLog>| l.ticks += 1)
        .add_systems(Last, record);
}

fn record(time: Res<Time<Real>>, mut l: ResMut<FpsLog>, mut exit: MessageWriter<AppExit>) {
    let now = time.elapsed_secs();
    // skip the first 2 s (startup, shader compiles)
    if now < 2.0 {
        l.ticks = 0;
        return;
    }
    let start = *l.started.get_or_insert(now);
    l.frames.push(time.delta_secs() * 1000.0);
    if now - start >= l.secs {
        let mut f = l.frames.clone();
        f.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = f.len();
        let mean = f.iter().sum::<f32>() / n as f32;
        let pct = |p: f32| f[((n as f32 - 1.0) * p) as usize];
        let over = |ms: f32| f.iter().filter(|&&x| x > ms).count();
        eprintln!(
            "FPS-LOG frames {n} in {:.1}s = {:.1} fps; frame ms mean {mean:.2} p50 {:.2} p95 {:.2} p99 {:.2} max {:.2}; >1.5x mean: {}; fixed ticks {} = {:.2} Hz",
            now - start, n as f32 / (now - start), pct(0.5), pct(0.95), pct(0.99), f[n - 1], over(mean * 1.5), l.ticks, l.ticks as f32 / (now - start)
        );
        exit.write(AppExit::Success);
    }
}
