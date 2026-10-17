//! Row 12: ADJUST SCREEN (OPTIONS row 2, the title's sub-state 4).
//!
//! The game [CODE 80074a94, 80073138, 80051698, 8005e260]: the display
//! offset (DISPENV screen x / y, FUN_8005E260) is the save's 0x800E0396 /
//! 97 (main() sets (8, 2) at power-on); the page moves it live with the
//! D-pad (x 0..32 RIGHT / LEFT, y 0..32 DOWN / UP), CROSS keeps it,
//! TRIANGLE or 0x708 idle VBlanks put the saved one back. It draws magenta
//! corner brackets, dark purple borders, x SELECT, t BACK and a red arrow
//! toward each side the picture can still move to.
//!
//! The port: settings `screen_x` / `screen_y` (mirrored into the save
//! block, save.rs); the offset moves the whole picture (the 3D view and the
//! HUD, main.rs `view_rect`) by (x - 8, y - 2) of the game's 240 lines,
//! within the window's free border (the pillarbox / letterbox of the
//! 4:3 view): a window the view fills (aspect `native`) has none, the
//! picture then stays where it is.

use std::sync::Mutex;

/// The page's live offset while it is open (None: the settings').
static LIVE: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// The saved offset (settings `screen_x` / `screen_y`, 0..32).
pub fn offset(st: &crate::settings::Settings) -> (i32, i32) {
    (st.i64("screen_x", 8).clamp(0, 32) as i32, st.i64("screen_y", 2).clamp(0, 32) as i32)
}

pub fn set_live(o: Option<(i32, i32)>) {
    *LIVE.lock().unwrap() = o;
}

/// The offset in force now.
pub fn current() -> (i32, i32) {
    LIVE.lock().unwrap().unwrap_or_else(|| offset(crate::settings::Settings::global()))
}

/// The view rectangle moved by the offset (window pixels; `view` the
/// view's height in pixels = 240 game lines), kept inside the window.
pub fn shifted(win: (u32, u32), pos: (u32, u32), size: (u32, u32)) -> (u32, u32) {
    shifted_by(current(), win, pos, size)
}

fn shifted_by((x, y): (i32, i32), win: (u32, u32), pos: (u32, u32), size: (u32, u32)) -> (u32, u32) {
    let px = size.1 as f32 / 240.0;
    let dx = ((x - 8) as f32 * px).round() as i64;
    let dy = ((y - 2) as f32 * px).round() as i64;
    let nx = (pos.0 as i64 + dx).clamp(0, (win.0 as i64 - size.0 as i64).max(0));
    let ny = (pos.1 as i64 + dy).clamp(0, (win.1 as i64 - size.1 as i64).max(0));
    (nx as u32, ny as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_stays_in_window() {
        // a 4:3 view in a 16:9 window: room left / right only
        assert_eq!(shifted_by((32, 32), (1600, 900), (200, 0), (1200, 900)), (290, 0));
        assert_eq!(shifted_by((0, 0), (1600, 900), (200, 0), (1200, 900)), (170, 0));
        assert_eq!(shifted_by((8, 2), (1600, 900), (200, 0), (1200, 900)), (200, 0));
    }
}
