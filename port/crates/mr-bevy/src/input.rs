//! Host input -> PS1 pad button mask (active high), sampled once per sim tick.
//! The mapping mirrors what the game sees on this save: CROSS accelerates,
//! SQUARE/CIRCLE/TRIANGLE as on the pad, D-pad steers (docs/spec/kart-state.md).

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

/// Phase 7: a two-player session runs (split screen): the pads split
/// between the players (`sample_two`). Set by the session driver.
static TWO: AtomicBool = AtomicBool::new(false);

pub fn set_two_players(on: bool) {
    TWO.store(on, Ordering::Relaxed);
}

pub fn two_players() -> bool {
    TWO.load(Ordering::Relaxed)
}

/// Bit 0 (unused by the race: kept for the complete pad map).
#[allow(dead_code)]
pub const SELECT: u16 = 1 << 0;
pub const START: u16 = 1 << 3;
pub const UP: u16 = 1 << 4;
pub const RIGHT: u16 = 1 << 5;
pub const DOWN: u16 = 1 << 6;
pub const LEFT: u16 = 1 << 7;
pub const L2: u16 = 1 << 8;
pub const R2: u16 = 1 << 9;
pub const L1: u16 = 1 << 10;
pub const R1: u16 = 1 << 11;
pub const TRIANGLE: u16 = 1 << 12;
pub const CIRCLE: u16 = 1 << 13;
pub const CROSS: u16 = 1 << 14;
pub const SQUARE: u16 = 1 << 15;

/// Keyboard: arrows steer, X = cross (accelerate), Z = square, S = circle,
/// A = triangle, Enter = start, Q/W = L1/R1 (psxrecomp's default layout),
/// E/R = L2/R2 (weapon slot 2, character special).
/// Gamepad: south = cross, west = square, east = circle, north = triangle,
/// right trigger = cross, left trigger = circle, shoulders = L1/R1, stick
/// clicks L3/R3 = L2/R2, left stick folds into the D-pad.
pub fn sample(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> u16 {
    let mut m = keyboard_p1(keys);
    for g in pads.iter() {
        m |= gamepad(g);
    }
    m
}

/// Phase 7: (player 1, player 2) pads. Gamepads in connection order: the
/// first is player 1's, the second player 2's (more are ignored). The
/// keyboard: player 1's keys as `sample`; player 2's (the fallback when
/// there is one gamepad or none): J / L steer, I / K up / down, N cross
/// (accelerate), M square, B circle, H triangle, Backspace start, U / O
/// L1 / R1, Y / P L2 / R2.
pub fn sample_two(keys: &ButtonInput<KeyCode>, pads: &Query<(Entity, &Gamepad)>) -> (u16, u16) {
    let mut v: Vec<(Entity, &Gamepad)> = pads.iter().collect();
    v.sort_by_key(|(e, _)| *e);
    let mut a = keyboard_p1(keys);
    let mut b = keyboard_p2(keys);
    if let Some((_, g)) = v.first() {
        a |= gamepad(g);
    }
    if let Some((_, g)) = v.get(1) {
        b |= gamepad(g);
    }
    (a, b)
}

fn keyboard_p2(keys: &ButtonInput<KeyCode>) -> u16 {
    let k = |c: KeyCode| keys.pressed(c);
    let mut m = 0u16;
    for (c, b) in [
        (KeyCode::KeyJ, LEFT),
        (KeyCode::KeyL, RIGHT),
        (KeyCode::KeyI, UP),
        (KeyCode::KeyK, DOWN),
        (KeyCode::KeyN, CROSS),
        (KeyCode::KeyM, SQUARE),
        (KeyCode::KeyB, CIRCLE),
        (KeyCode::KeyH, TRIANGLE),
        (KeyCode::Backspace, START),
        (KeyCode::KeyU, L1),
        (KeyCode::KeyO, R1),
        (KeyCode::KeyY, L2),
        (KeyCode::KeyP, R2),
    ] {
        if k(c) {
            m |= b;
        }
    }
    m
}

fn keyboard_p1(keys: &ButtonInput<KeyCode>) -> u16 {
    let mut m = 0u16;
    let k = |c: KeyCode| keys.pressed(c);
    if k(KeyCode::ArrowLeft) { m |= LEFT; }
    if k(KeyCode::ArrowRight) { m |= RIGHT; }
    if k(KeyCode::ArrowUp) { m |= UP; }
    if k(KeyCode::ArrowDown) { m |= DOWN; }
    if k(KeyCode::KeyX) { m |= CROSS; }
    if k(KeyCode::KeyZ) { m |= SQUARE; }
    if k(KeyCode::KeyS) { m |= CIRCLE; }
    if k(KeyCode::KeyA) { m |= TRIANGLE; }
    if k(KeyCode::Enter) { m |= START; }
    if k(KeyCode::KeyQ) { m |= L1; }
    if k(KeyCode::KeyW) { m |= R1; }
    if k(KeyCode::KeyE) { m |= L2; }
    if k(KeyCode::KeyR) { m |= R2; }
    m
}

fn gamepad(g: &Gamepad) -> u16 {
    let mut m = 0u16;
    {
        let p = |b: GamepadButton| g.pressed(b);
        if p(GamepadButton::South) || p(GamepadButton::RightTrigger2) { m |= CROSS; }
        if p(GamepadButton::West) { m |= SQUARE; }
        if p(GamepadButton::East) || p(GamepadButton::LeftTrigger2) { m |= CIRCLE; }
        if p(GamepadButton::North) { m |= TRIANGLE; }
        if p(GamepadButton::DPadLeft) { m |= LEFT; }
        if p(GamepadButton::DPadRight) { m |= RIGHT; }
        if p(GamepadButton::DPadUp) { m |= UP; }
        if p(GamepadButton::DPadDown) { m |= DOWN; }
        if p(GamepadButton::Start) { m |= START; }
        if p(GamepadButton::LeftTrigger) { m |= L1; }
        if p(GamepadButton::RightTrigger) { m |= R1; }
        if p(GamepadButton::LeftThumb) { m |= L2; }
        if p(GamepadButton::RightThumb) { m |= R2; }
        let x = g.left_stick().x;
        if x < -0.4 { m |= LEFT; }
        if x > 0.4 { m |= RIGHT; }
    }
    m
}
