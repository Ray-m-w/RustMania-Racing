//! Host input -> PS1 pad button mask (active high), sampled once per sim tick.
//! The mapping mirrors what the game sees on this save: CROSS accelerates,
//! SQUARE/CIRCLE/TRIANGLE as on the pad, D-pad steers (docs/spec/kart-state.md).

use bevy::prelude::*;

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
/// A = triangle, Enter = start, Q/W = L1/R1 (psxrecomp's default layout).
/// Gamepad: south = cross, west = square, east = circle, north = triangle,
/// right trigger = cross, left trigger = circle, left stick folds into the D-pad.
pub fn sample(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> u16 {
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
    for g in pads.iter() {
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
        let x = g.left_stick().x;
        if x < -0.4 { m |= LEFT; }
        if x > 0.4 { m |= RIGHT; }
    }
    m
}
