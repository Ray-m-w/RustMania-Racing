//! Row 12: the in-process course change (no relaunch).
//!
//! The game loads a course between two course sessions (main() ->
//! FUN_8008ACE4: the LOADING picture, the track loader, the session). The
//! port used to restart its program on the new course (the scene is built
//! at start-up from the launch arguments). Now it reboots in place:
//!
//! - the boot arguments (settings.rs `args`) are replaced with the new
//!   course's (`--course NAME --skip-title --menu --character C --vehicle
//!   V [--league]`, `--ending`, `--demo ...`: what the relaunch passed);
//! - every entity with a transform (the scene, cameras, karts, HUD, movie
//!   and effect quads) is despawned, except `KeepOnBoot` ones (the LOADING
//!   picture, loading.rs), which frees their meshes, materials and images;
//! - each plugin's resources go back to their start-up values (`on_boot`
//!   registrations: the same function that inserted them at start-up);
//! - the `Boot` schedule (every former Startup system that builds the
//!   course: scene, VRAM textures, sky, movers, effects, HUD tables, title,
//!   movie quad, cameras) runs again.
//!
//! The process keeps what the game keeps across course sessions: the save
//! block (save.rs), the music thread and audio device, the movies seen
//! since power-on (movie.rs), the settings.

use std::sync::atomic::{AtomicU32, Ordering};

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

/// The schedule of the course's start-up systems (run in Startup, and again
/// after each in-process course change).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Boot;

/// Entities that survive a course change (the LOADING picture).
#[derive(Component)]
pub struct KeepOnBoot;

/// Boots so far (0 = the start-up one).
static GEN: AtomicU32 = AtomicU32::new(0);

pub fn generation() -> u32 {
    GEN.load(Ordering::Relaxed)
}

/// For systems with `Local` state: true once after each course change
/// (`seen` is the system's own copy of the generation).
pub fn fresh(seen: &mut u32) -> bool {
    let g = generation();
    if *seen != g {
        *seen = g;
        true
    } else {
        false
    }
}

/// The resets registered by the plugins.
#[derive(Resource, Default)]
struct Resets(Vec<fn(&mut World)>);

/// Insert a plugin's per-course resources now and again at every course
/// change.
pub fn on_boot(app: &mut App, f: fn(&mut World)) {
    f(app.world_mut());
    app.world_mut().get_resource_or_init::<Resets>().0.push(f);
}

pub struct BootPlugin;

impl Plugin for BootPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(Boot);
        app.init_resource::<Resets>();
        app.add_systems(Startup, |world: &mut World| world.run_schedule(Boot));
    }
}

/// Tear the course down and build `args`' course in place (loading.rs calls
/// it while the LOADING picture shows).
pub fn reboot(world: &mut World, args: Vec<String>) {
    let t0 = std::time::Instant::now();
    let before = live_entities(world);
    crate::settings::set_args(args);
    crate::courses::reset();
    GEN.fetch_add(1, Ordering::Relaxed);
    // the game's main(): the course session starts unpaused
    if let Some(mut t) = world.get_resource_mut::<Time<Virtual>>() {
        t.unpause();
    }
    // despawn the course's entities (children go with their roots)
    let mut q = world.query_filtered::<Entity, (With<GlobalTransform>, Without<KeepOnBoot>, Without<ChildOf>)>();
    let roots: Vec<Entity> = q.iter(world).collect();
    for e in roots {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    let mut q = world.query_filtered::<Entity, (With<GlobalTransform>, Without<KeepOnBoot>)>();
    let left: Vec<Entity> = q.iter(world).collect();
    for e in left {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    let resets = world.resource::<Resets>().0.clone();
    for f in resets {
        f(world);
    }
    world.run_schedule(Boot);
    world.flush();
    let after = live_entities(world);
    info!("boot {}: {} in {:.0} ms (entities {} -> {})", generation(), crate::courses::info().name, t0.elapsed().as_secs_f64() * 1000.0, before, after);
    if crate::settings::args().iter().any(|a| a == "--boot-stats") {
        eprintln!("boot {} {} entities {} -> {} mem {} KiB ms {:.0}", generation(), crate::courses::info().name, before, after, working_set_kib(), t0.elapsed().as_secs_f64() * 1000.0);
    }
}

fn live_entities(world: &mut World) -> usize {
    world.query::<Entity>().iter(world).count()
}

/// Entities and asset counts (the course-change soak, loading.rs).
pub fn census(world: &mut World) -> String {
    let n = live_entities(world);
    let meshes = world.resource::<Assets<Mesh>>().len();
    let images = world.resource::<Assets<Image>>().len();
    let mats = world.resource::<Assets<StandardMaterial>>().len();
    let push = world.resource::<Assets<crate::scenepush::PushMaterial>>().len();
    let kart = world.resource::<Assets<crate::karts::KartMaterial>>().len();
    format!("entities {n} meshes {meshes} images {images} materials {mats}+{push}+{kart} mem {} KiB", working_set_kib())
}

/// The process's working set (KiB; Windows), for the course-change soak.
#[cfg(windows)]
pub fn working_set_kib() -> u64 {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c: Counters = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<Counters>() as u32;
    // SAFETY: a plain query of this process's counters into a sized struct
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok != 0 {
        (c.pagefile_usage / 1024) as u64
    } else {
        0
    }
}

#[cfg(not(windows))]
pub fn working_set_kib() -> u64 {
    0
}
