//! The courses' moving scenery and weather (Session 6, track R).
//!
//! Moving scenery: the DAT special slots the race code poses every frame
//! (DAT.md "Output block": pose records; FUN_8002D9E4 angles = pose + a,
//! FUN_8002E060 origin += d, FUN_8002D72C scale = pose * s / 4096, 0 =
//! hidden):
//! - FUN_800859EC: the animated objects 0x800D8630 (mr_sim::scenery: spin
//!   angles, the turntable discs and jet fans of BASE, OVAL's and DISCO's
//!   spinners) and the lifts' slots moved by the lift's travel (BOG,
//!   ISLAND, HAPPY, ROCK, DISCO); BASE's slots 5 / 6 turned by the global
//!   spin angles 0x800A889C (FUN_8003C5FC: += 0x800A88A8[i] * dt); HAPPY's
//!   slots 0x50..0x53 pulsed in x by the frame counter (% 48);
//! - FUN_80069334: a breakable group's slots hidden (scale 0) when a weapon
//!   knocks it down (mr_sim breakables: `hidden_slots`).
//! Every other special slot outside the pickups (0x20 + the chunk 0x0C
//! items, drawn by effects.rs) keeps its load pose and is drawn as scenery.
//!
//! Each special-slot object is built in mesh space (scene.rs) and placed by
//! its pose: world = origin + (M v) >> 12 (x4 on layer 1), M = Rx Ry Rz
//! of the angles, columns scaled; between ticks the pose is interpolated as
//! the karts are (from / to and the overstep of effects.rs).
//!
//! Weather (FUN_8007E21C's weather loop, the drops of FUN_8008650C, 16 at
//! 0x800D75D0): within 20000 view units (x, z) of the camera; rain (+0x1C =
//! 1) a LineG2 from the drop (its colour) to 2 * speed above it (black),
//! additive (FUN_80032120 ... 0x20); snow a billboard (FUN_80030474) of
//! +0x1A x +0x1B, texture slot 0x15 (0x80, 0xC0) 15 x 15 clut 0xE, additive
//! (0x120). Template FUN_8008FEA8.

use bevy::prelude::*;
use mr_assets::dat::{self, Dat, SinTable};

/// One special-slot object placed by its pose (on the TrackObject entity).
#[derive(Component, Clone, Copy, Debug)]
pub struct Mover {
    pub slot: u16,
    /// Layer world scale (1, or 4 for layer 1).
    pub layer: i32,
    /// The pose record: origin, angles, scale (4.12; 0x1000 unscaled).
    pub origin: [i32; 3],
    pub angles: [u16; 3],
    pub scale: [i16; 3],
    /// Hidden by `place` (knocked down / scaled to nothing).
    pub hid: bool,
}

/// What the race code did to one slot this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotPose {
    /// FUN_8002D9E4's angles (added to the pose's).
    pub rot: [f32; 3],
    /// FUN_8002E060's accumulated y move (DAT units).
    pub dy: f32,
    /// FUN_8002D72C's scale factors (4.12).
    pub scale: [f32; 3],
    /// Row 12: FUN_8002DF88's origin (DAT units) instead of the pose's.
    pub pos: Option<[f32; 3]>,
}

impl Default for SlotPose {
    fn default() -> Self {
        SlotPose { rot: [0.0; 3], dy: 0.0, scale: [4096.0; 3], pos: None }
    }
}

/// One falling drop to draw (mr_sim::weather::Drop).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropView {
    pub pos: [i32; 3],
    pub speed: i32,
    pub timer: i16,
    pub size: [u8; 2],
    pub rgb: [u8; 3],
}

/// The scenery state of one tick.
#[derive(Clone, Debug, Default)]
pub struct SceneryFrame {
    pub poses: Vec<(u16, SlotPose)>,
    pub hidden: Vec<u16>,
    pub snow: bool,
    pub drops: Vec<DropView>,
}

/// Exe data FUN_8003C5FC's global spin angles start from (0x800A889C) and
/// their speeds (0x800A88A8), read once.
static SPIN: std::sync::OnceLock<[[i16; 6]; 2]> = std::sync::OnceLock::new();

fn global_spin(frame: i32) -> [u16; 6] {
    let t = SPIN.get_or_init(|| {
        let mut out = [[0i16; 6]; 2];
        if let Ok(mut disc) = mr_disc::Disc::open_default() {
            if let Ok(exe) = disc.read("SLUS_012.37") {
                let at = |a: u32| {
                    let o = (a - 0x8001_0000) as usize + 0x800;
                    i16::from_le_bytes([exe[o], exe[o + 1]])
                };
                for i in 0..6 {
                    out[0][i] = at(0x800A_889C + 2 * i as u32);
                    out[1][i] = at(0x800A_88A8 + 2 * i as u32);
                }
            }
        }
        out
    });
    // the phase at race start is power-on dependent [INFER]: the exe's
    // values plus the speed for every VBlank of the frame counter
    std::array::from_fn(|i| ((t[0][i] as i32).wrapping_add((t[1][i] as i32).wrapping_mul(frame)) & 0xFFF) as u16)
}

impl SceneryFrame {
    pub fn of(s: &mr_sim::session::Session) -> SceneryFrame {
        let g = &s.globals;
        let o = &g.objects;
        let id = s.world.info.id;
        let mut poses: Vec<(u16, SlotPose)> = Vec::new();
        let mut set = |slot: i32, f: &dyn Fn(&mut SlotPose)| {
            if !(0..0x100).contains(&slot) {
                return;
            }
            let slot = slot as u16;
            let i = match poses.iter().position(|p| p.0 == slot) {
                Some(i) => i,
                None => {
                    poses.push((slot, SlotPose::default()));
                    poses.len() - 1
                }
            };
            f(&mut poses[i].1);
        };
        // FUN_800859EC: the angles of every object with a slot, then the
        // lifts' travel (cur - the low end the DAT was loaded at).
        for a in &o.anim {
            if a.slot < 0 {
                continue;
            }
            let lift = (a.lift >= 0).then(|| o.lifts.get(a.lift as usize)).flatten();
            for k in 0..a.slots as i32 {
                let rot = a.rot.map(|r| (r & 0xFFF) as f32);
                set(a.slot as i32 + k, &|p| p.rot = rot);
                if let Some(l) = lift {
                    let d = (l.cur - l.min) as f32;
                    set(a.slot as i32 + k, &|p| p.dy = d);
                }
            }
        }
        // the frame counter FUN_8007A9DC saw (one race frame before it
        // stepped; animtex.rs)
        let f = g.frame - g.dt.max(1);
        if id == 0x0F {
            let a = global_spin(f).map(|v| v as f32);
            set(5, &|p| p.rot = [a[0], a[1], a[2]]);
            set(6, &|p| p.rot = [0.0, a[1], 0.0]);
        }
        if id == 9 && o.anim.iter().any(|a| a.lift == 0) && !o.lifts.is_empty() {
            let m = f.rem_euclid(0x30);
            let s = if m > 0xF { (0x30 - m) * 0x80 } else { m << 8 };
            for slot in 0x50..0x54 {
                set(slot, &|p| p.scale = [s as f32, 4096.0, 4096.0]);
            }
        }
        let mut hidden = Vec::new();
        // Row 12: the course select (FUN_80091B8C mode 0xC, mr-sim title.rs
        // `CourseSelect`): the courses' props, the "?" marks, the worlds'
        if !s.menu_slots.is_empty() {
            for m in &s.menu_slots {
                if m.scale == 0 {
                    hidden.push(m.slot);
                    continue;
                }
                set(m.slot as i32, &|p| {
                    p.scale = [m.scale as f32; 3];
                    p.rot = m.rot.map(|r| (r & 0xFFF) as f32);
                    p.pos = m.pos.map(|q| q.map(|v| v as f32));
                });
            }
        }
        // FUN_8002DC14 (phase 6): the challenge objects' slots 0x20.. (the
        // targets, then the movie and extras; the Doozer goals have none),
        // turned by 0x800B0CB0 / CB2 / CB4 (FUN_8003C5FC: sin(0x800AFEF6) /
        // 64, 0x800AFEF8, sin(0x800AFEFA) / 64) and scaled by the object's
        // +0x16 << 5; hidden while it is (scale 0: a race, a taken one).
        let [ten, goals, extra] = g.items.challenge.map(|c| c as usize);
        if ten != 0 && goals != 0 {
            let t = &s.world.tables;
            let ph = s.water.phase;
            let w = |a: u16| (mr_sim::fixed::div_pow2(t.sin_raw(a as i32 & 0xFFF), 6) & 0xFFF) as f32;
            let rot = [w(ph[3]), (ph[4] & 0xFFF) as f32, w(ph[5])];
            for k in 0..ten + 1 + extra.saturating_sub(1) {
                let idx = if k < ten { k } else { k + goals };
                let slot = 0x20 + k as i32;
                let sc = g.items.pickups.get(mr_sim::items::FIRST_CHALLENGE + idx).map_or(0, |p| p.scale as i32) << 5;
                if sc == 0 {
                    hidden.push(slot as u16);
                } else {
                    set(slot, &|p| {
                        p.rot = rot;
                        p.scale = [sc as f32; 3];
                    });
                }
            }
        }
        for &(slot, n) in &o.hidden_slots {
            for k in 0..n as i16 {
                hidden.push((slot + k) as u16);
            }
        }
        let (snow, drops) = match &o.weather {
            Some(w) => (
                w.snow,
                w.drops.iter().filter(|d| d.timer != -1).map(|d| DropView { pos: d.pos, speed: d.speed, timer: d.timer, size: d.size, rgb: d.rgb }).collect(),
            ),
            None => (false, Vec::new()),
        };
        SceneryFrame { poses, hidden, snow, drops }
    }

    /// The state shown at weight `t` from `a` to `b` (as the karts): poses
    /// in both interpolated (angles the short way), drops continuing in
    /// both moved; the rest from the nearer end.
    pub fn lerp(a: &SceneryFrame, b: &SceneryFrame, t: f32) -> SceneryFrame {
        let near = if t >= 0.5 { b } else { a };
        let mut out = near.clone();
        for (slot, p) in out.poses.iter_mut() {
            let (Some(pa), Some(pb)) = (a.poses.iter().find(|q| q.0 == *slot), b.poses.iter().find(|q| q.0 == *slot)) else { continue };
            let (pa, pb) = (pa.1, pb.1);
            for k in 0..3 {
                let mut d = (pb.rot[k] - pa.rot[k]).rem_euclid(4096.0);
                if d > 2048.0 {
                    d -= 4096.0;
                }
                p.rot[k] = pa.rot[k] + d * t;
                p.scale[k] = pa.scale[k] + (pb.scale[k] - pa.scale[k]) * t;
            }
            p.dy = pa.dy + (pb.dy - pa.dy) * t;
            if let (Some(x), Some(y)) = (pa.pos, pb.pos) {
                p.pos = Some(std::array::from_fn(|k| x[k] + (y[k] - x[k]) * t));
            }
        }
        // drops: same slot order; a drop continues while its timer falls
        for (i, d) in out.drops.iter_mut().enumerate() {
            if let (Some(x), Some(y)) = (a.drops.get(i), b.drops.get(i)) {
                if y.timer <= x.timer && x.pos[0] == y.pos[0] && x.pos[2] == y.pos[2] {
                    d.pos[1] = (x.pos[1] as f64 + (y.pos[1] - x.pos[1]) as f64 * t as f64).round() as i32;
                }
            }
        }
        out
    }
}

/// The scenery state between the last two ticks (or toward the look-ahead
/// one), set by effects.rs `session_source` with the same endpoints and
/// weight as the effects.
#[derive(Resource, Default)]
pub struct SceneryShown(pub Option<SceneryFrame>);

pub struct MoversPlugin;

impl Plugin for MoversPlugin {
    fn build(&self, app: &mut App) {
        crate::boot::on_boot(app, |w| w.insert_resource(SceneryShown::default()));
        app.add_systems(Update, place.after(crate::select_objects).after(crate::effects::session_source).before(crate::shot))
            .add_systems(Update, weather.after(crate::effects::session_source).after(crate::effects::capture_source).before(crate::effects::draw))
            .add_systems(Update, look.after(crate::follow_camera).before(crate::follow_sky).before(crate::shot))
            .add_systems(Update, hide_dev.after(place).before(crate::shot))
            .add_systems(Update, decal_check);
    }
}

/// The world transform of a pose (Bevy space, mesh built as
/// `to_bevy(v * 32)`): translation `to_bevy(layer * origin * 32)`, linear
/// part `layer * F M F` (F = diag(1, -1, -1)), M = Rx Ry Rz with the
/// columns scaled.
pub fn pose_transform(m: &Mover, p: Option<&SlotPose>) -> Option<Transform> {
    let p = p.copied().unwrap_or_default();
    let ang = |k: usize| (m.angles[k] as f32 + p.rot[k]) * std::f32::consts::TAU / 4096.0;
    let (sa, ca) = ang(0).sin_cos();
    let (sb, cb) = ang(1).sin_cos();
    let (sc, cc) = ang(2).sin_cos();
    // dat::rot_matrix in floats
    let r = [
        [cc * cb, -sc * cb, sb],
        [cc * sa * sb + ca * sc, ca * cc - sc * sa * sb, -sa * cb],
        [sa * sc - cc * ca * sb, sa * cc + sc * ca * sb, ca * cb],
    ];
    let s: [f32; 3] = std::array::from_fn(|c| m.scale[c] as f32 / 4096.0 * p.scale[c] / 4096.0);
    if s.iter().all(|v| v.abs() < 1e-6) {
        return None;
    }
    let f = [1.0f32, -1.0, -1.0];
    let l = m.layer as f32;
    let col = |c: usize| Vec3::new(f[0] * f[c] * r[0][c], f[1] * f[c] * r[1][c], f[2] * f[c] * r[2][c]);
    let rot = Mat3::from_cols(col(0), col(1), col(2));
    let rot = Quat::from_mat3(&rot).normalize();
    let scale = Vec3::new(s[0], s[1], s[2]).map(|v| if v.abs() < 1e-4 { 1e-4f32.copysign(v) } else { v }) * l;
    let o = p.pos.unwrap_or([m.origin[0] as f32, m.origin[1] as f32 + p.dy, m.origin[2] as f32]);
    let translation = Vec3::new(o[0], -o[1], -o[2]) * (l * 32.0 / crate::UNITS);
    Some(Transform { translation, rotation: rot, scale })
}

fn place(
    shown: Res<SceneryShown>,
    settings: Res<crate::settings::Settings>,
    mut q: Query<(&mut Mover, &mut Transform, &mut Visibility)>,
) {
    let Some(fr) = shown.0.as_ref() else { return };
    let full = !settings.faithful_draw_distance();
    // dev: MRM_MOVERS_DEBUG prints the moved slots once
    if std::env::var("MRM_MOVERS_DEBUG").is_ok() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            for (m, t, v) in q.iter() {
                eprintln!("mover slot {:#04x}: {:?} pose {:?} hidden {} vis {:?} at {:?}", m.slot, m.origin, fr.poses.iter().find(|p| p.0 == m.slot).map(|p| p.1), fr.hidden.contains(&m.slot), v, t.translation);
            }
        });
    }
    for (mut m, mut t, mut v) in q.iter_mut() {
        let pose = fr.poses.iter().find(|p| p.0 == m.slot).map(|p| &p.1);
        let tr = if fr.hidden.contains(&m.slot) { None } else { pose_transform(&m, pose) };
        match tr {
            Some(tr) => {
                if *t != tr {
                    *t = tr;
                }
                // shown again (a new race): the full view's choice is
                // Inherited; the faithful one is select_objects' every tick
                if m.hid {
                    m.hid = false;
                    if full {
                        *v = Visibility::Inherited;
                    }
                }
            }
            None => {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
                m.hid = true;
            }
        }
    }
}

/// dev: MRM_LOOK="ex,ey,ez,tx,ty,tz" (DAT units, game axes) puts the chase
/// camera at the eye looking at the target (side-by-sides of scenery the
/// race camera does not pass).
fn look(mut q: Query<&mut Transform, With<crate::ChaseCamera>>) {
    let Ok(v) = std::env::var("MRM_LOOK") else { return };
    let n: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if n.len() < 6 {
        return;
    }
    let w = |i: usize| crate::to_bevy(n[i] * 32, n[i + 1] * 32, n[i + 2] * 32);
    for mut t in q.iter_mut() {
        *t = Transform::from_translation(w(0)).looking_at(w(3), Vec3::Y);
    }
}

/// dev: MRM_HIDE="a-b,c" hides those DAT objects (which object draws what).
fn hide_dev(mut q: Query<(&crate::TrackObject, &mut Visibility)>) {
    let Ok(v) = std::env::var("MRM_HIDE") else { return };
    let mut set = Vec::new();
    for part in v.split(',') {
        let r: Vec<usize> = part.split('-').filter_map(|x| x.trim().parse().ok()).collect();
        match r.as_slice() {
            [a] => set.push(*a..=*a),
            [a, b] => set.push(*a..=*b),
            _ => {}
        }
    }
    for (o, mut vis) in q.iter_mut() {
        if set.iter().any(|r| r.contains(&o.0)) && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
}

/// dev: MRM_DECAL_CHECK=FILE (tools/oracle/course_ram.py JSON with
/// 800DDC50:120, 800B0DCD:1, 800CB058:600, 800B0FA4:4): the projectile
/// shadows kartfx::projectile_decals builds from the oracle's projectile
/// table (face normals from this session's TER) against the shadow-list
/// entries the game built in the same frame. Prints and exits.
fn decal_check(p: Res<crate::Player>, t: Option<Res<crate::effects::FxTables>>, mut exit: MessageWriter<AppExit>) {
    let Ok(path) = std::env::var("MRM_DECAL_CHECK") else { return };
    let (Some(t), crate::Driver::Sim(run)) = (t, &p.driver) else { return };
    let text = std::fs::read_to_string(&path).expect("decal check json");
    let hex = |key: &str| -> Vec<u8> {
        let i = text.find(&format!("\"{key}\": \"")).expect(key) + key.len() + 5;
        let h = &text[i..i + text[i..].find('"').unwrap()];
        (0..h.len() / 2).map(|k| u8::from_str_radix(&h[2 * k..2 * k + 2], 16).unwrap()).collect()
    };
    let proj = hex("800DDC50:120");
    let n = hex("800B0DCD:1")[0] as usize;
    let list = hex("800CB058:600");
    let fb = hex("800B0FA4:4");
    let faces = u32::from_le_bytes([fb[0], fb[1], fb[2], fb[3]]);
    let mut m = mr_sim::mem::Mem::new(vec![0; 0x20_0000], vec![0; 0x400]);
    m.write(0x800D_DC50, &proj);
    let w = &run.session.world;
    let normal = |ptr: u32| w.ter.faces.get(((ptr - faces) / 16) as usize).map(|f| f.normal);
    let mut mine = Vec::new();
    crate::kartfx::projectile_decals(&t.kart, &m, &normal, &mut mine);
    let i32at = |b: &[u8], o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let i16at = |b: &[u8], o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
    let mut exact = 0;
    for k in 0..n {
        let e = &list[0x30 * k..0x30 * k + 0x30];
        let origin = [i32at(e, 0), i32at(e, 4), i32at(e, 8)];
        let offs: [[i16; 3]; 4] = std::array::from_fn(|c| std::array::from_fn(|j| i16at(e, 0xC + 6 * c + 2 * j)));
        let game = (origin, offs, [e[0x24], e[0x26]], [e[0x25], e[0x27]], e[0x28], e[0x29], e[0x2B], e[0x2C] as u16);
        let found = mine.iter().find(|d| d.origin == origin);
        let ok = found.is_some_and(|d| (d.origin, d.offs, d.uv0, d.size, d.slot, d.scale, d.intensity, d.tpage_add) == game);
        exact += ok as usize;
        eprintln!("decal check: game {:?} port {:?} {}", game, found, if ok { "exact" } else { "DIFF" });
    }
    eprintln!("decal check: {exact} of {n} game entries exact; port built {}", mine.len());
    exit.write(AppExit::Success);
}

/// FUN_8007E21C's weather loop: the drops into the effects frame.
fn weather(
    shown: Res<SceneryShown>,
    p: Res<crate::Player>,
    t: Option<Res<crate::effects::FxTables>>,
    cap: Option<Res<crate::capture::CaptureReplay>>,
    mut rows: Option<ResMut<crate::effects::CapRows>>,
    mut fx: ResMut<crate::effects::FxFrame>,
) {
    let Some(t) = t else { return };
    // `--replay-capture`: the 16 drops of the captured list 0x800D75D0
    let captured = match (cap.as_ref(), rows.as_mut()) {
        (Some(cap), Some(rows)) => rows.mem(cap.vb).map(|m| {
            let flags = crate::courses::info().flags;
            let drops = (0..16u32)
                .map(|i| 0x800D_75D0 + 0x2C * i)
                .filter(|&a| m.i16(a + 0x14) != -1)
                .map(|a| DropView {
                    pos: [m.i32(a), m.i32(a + 4), m.i32(a + 8)],
                    speed: m.i32(a + 0x10),
                    timer: m.i16(a + 0x14),
                    size: [m.u8(a + 0x1A), m.u8(a + 0x1B)],
                    rgb: [m.u8(a + 0x23), m.u8(a + 0x24), m.u8(a + 0x25)],
                })
                .collect();
            SceneryFrame { snow: flags & 0x20 != 0, drops, ..Default::default() }
        }),
        _ => None,
    };
    let Some(fr) = captured.as_ref().or(shown.0.as_ref()) else { return };
    if fr.drops.is_empty() || crate::courses::info().flags & 0x60 == 0 {
        return;
    }
    // the human camera's eye >> 5 (camera +0x34 / +0x3C)
    let Some((eye, _)) = p.pose.cam else { return };
    let e = [eye[0] >> 5, eye[2] >> 5];
    for d in &fr.drops {
        let (x, z) = (d.pos[0] >> 5, d.pos[2] >> 5);
        if (x - e[0]).wrapping_add(20000) as u32 >= 0x9C41 || (z - e[1]).wrapping_add(20000) as u32 >= 0x9C41 {
            continue;
        }
        // view units as the game passes them (>> 5), back to world
        let at = [x << 5, (d.pos[1] >> 5) << 5, z << 5];
        if !fr.snow {
            let top = [x << 5, ((d.pos[1] - 2 * d.speed) >> 5) << 5, z << 5];
            fx.streaks.push(crate::effects::Streak { a: at, b: top, rgb_a: d.rgb, rgb_b: [0; 3] });
        } else {
            let (tp, cl) = t.slot_attr(0x15, 0xE, 0x20);
            fx.billboards.push(crate::effects::Billboard {
                pos: at,
                hw: d.size[0] as i32,
                hh: d.size[1] as i32,
                angle: 0,
                anchor: 0,
                flat: false,
                u: 0x80,
                v: 0xC0,
                uw: 0xF,
                vh: 0xF,
                tpage: tp,
                clut: cl,
                rgb: d.rgb,
                semi: true,
                abr: 1,
                bias: 0,
            });
        }
    }
}

/// The pickup slots (FUN_8007B448: 0x20 + each chunk 0x0C point; effects.rs
/// draws the items).
pub fn pickup_slot(dat: &Dat, slot: usize) -> bool {
    let n = dat.chunks.iter().find(|c| c.id == 0x0C).map(|c| c.count as u16).filter(|n| (0xB..=0x10).contains(n)).unwrap_or(0) as usize;
    (0x20..0x20 + n).contains(&slot)
}

/// Phase 6: the mover of DAT object `i` when it sits in a challenge
/// object's slot (0x20 + k, FUN_8007B448 / FUN_8002DF88): placed on the
/// object's point (chunk 0x0C: the targets, then the movie / extras after
/// the one Doozer goal), angles 0 (FUN_8002DC14 turns it by the global
/// wobble), unscaled.
pub fn challenge_mover(dat: &Dat, i: usize, points: &[[i32; 3]]) -> Option<Mover> {
    let s = dat.special.iter().position(|s| *s == Some(i)).filter(|&s| pickup_slot(dat, s))?;
    let k = s - 0x20;
    let idx = if k < 10 { k } else { k + 1 };
    let p = *points.get(idx)?;
    let o = &dat.objects[i];
    Some(Mover { slot: s as u16, layer: o.layer.world_scale(), origin: p, angles: [0; 3], scale: [0x1000; 3], hid: false })
}

/// The special slot of DAT object `i` that is moving scenery (not a
/// pickup), if any.
pub fn mover_slot(dat: &Dat, i: usize) -> Option<usize> {
    dat.special.iter().position(|s| *s == Some(i)).filter(|&s| !pickup_slot(dat, s))
}

/// The pose record of a special-slot object (its descriptor at load).
pub fn mover(dat: &Dat, i: usize, slot: usize) -> Mover {
    let o = &dat.objects[i];
    let d = dat.desc(o);
    Mover { slot: slot as u16, layer: o.layer.world_scale(), origin: d.origin, angles: d.angles, scale: d.scale.unwrap_or([0x1000; 3]), hid: false }
}

/// A special-slot object's polygons in mesh space (scene::object_polys
/// with the identity transform).
pub fn local_polys(dat: &Dat, o: &dat::Object, sin: &SinTable, slots: &[Option<dat::TexSlot>; 32]) -> Vec<crate::psxmesh::PsxPoly> {
    let mut out = Vec::new();
    let Some(mesh) = dat.mesh(o) else { return out };
    for g in &mesh.groups {
        let slot = if g.is_textured() { slots[g.tex_slot() as usize] } else { None };
        let n = if g.is_triangle() { 3 } else { 4 };
        let ord: &[usize] = if n == 4 { &[0, 1, 3, 2] } else { &[0, 1, 2] };
        for p in &g.prims {
            out.push(crate::psxmesh::PsxPoly {
                pos: ord.iter().map(|&k| mesh.vertices[p.idx[k] as usize].pos.map(|c| c as i32)).collect(),
                uv: ord.iter().map(|&k| p.uv[k]).collect(),
                rgb: ord.iter().map(|&k| mesh.vertices[p.idx[k] as usize].rgb()).collect(),
                tex: slot.map(|s| dat::prim_tpage_clut(s, g, p)),
                // phase 14: an environment-mapped group's first polygon is
                // opaque (its reflection pass: envmap.rs)
                semi: g.semi_transparent() && !g.double_slot(),
                abr: g.blend_bits(),
                cull: !g.double_sided(),
            });
        }
    }
    let _ = sin;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The float pose reproduces the game's integer transform (DAT world
    /// units) to within a unit for a turned, scaled, lifted object.
    #[test]
    fn pose_matches_the_integer_transform() {
        let sin = SinTable((0..5120).map(|i| ((i as f64 * std::f64::consts::TAU / 4096.0).sin() * 16384.0).round() as i16).collect());
        let m = Mover { slot: 1, layer: 1, origin: [100, -50, 2000], angles: [100, 900, 3000], scale: [0x1800, 0x1000, 0x800], hid: false };
        let p = SlotPose { rot: [0.0, 300.0, 0.0], dy: -120.0, scale: [4096.0; 3], pos: None };
        let tr = pose_transform(&m, Some(&p)).unwrap();
        let mut mi = dat::rot_matrix(&sin, [100, 1200, 3000]);
        dat::scale_matrix(&mut mi, m.scale);
        let ti = dat::Transform { m: mi, origin: [100, -170, 2000], scale: 1 };
        for v in [[100i16, 0, 0], [0, 200, 0], [-30, 40, 500]] {
            let w = ti.apply(v);
            let b = tr.transform_point(crate::to_bevy(v[0] as i32 * 32, v[1] as i32 * 32, v[2] as i32 * 32));
            let want = crate::to_bevy(w[0] * 32, w[1] * 32, w[2] * 32);
            assert!((b - want).length() * crate::UNITS / 32.0 < 1.5, "{v:?}: {b} vs {want}");
        }
    }
}
