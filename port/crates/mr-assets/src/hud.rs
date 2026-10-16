//! hud: the race HUD as the game builds it - which sprites, where, from which
//! texture page/CLUT - reproduced from the game's own code and the tables in
//! the executable (read from SLUS_012.37 at run time; no game data here).
//!
//! Game functions ported (docs/spec/hud.md):
//! - FUN_800474e8: draw a string with the sprite font (glyph table 0x800ABC18,
//!   colour table 0x800ABC0C, texture slot 0xF = the font page loaded with the
//!   course RAW; one SPRT per glyph, then a DR_MODE with the page + abr).
//! - FUN_8004f688 case 1 (1-player race): race position (+0x4CA, 24 px digit +
//!   12 px suffix) and "LAP n/m" (+0x4BF, DAT_800B0DA6).
//! - FUN_80050b2c: race timer from DAT_800B0A94 ("%i:%i%i.%i", hundredths/10).
//! - FUN_80050f64: pre-race texts ("RACE 3 LAPS", "COPY THE BUTTONS", "READY").
//! - FUN_8004e8e8: the "copy the buttons" minigame (four button icons from
//!   slot 5 via FUN_8002faf4 POLY_FT4s, and the count +0x446).
//! Not ported (yet): item slot/meter (FUN_8004cbc8, FUN_80049440), minimap,
//! lap-split popup (FUN_80049fd8), finish/results (FUN_8004cea0), the
//! minigame sparkles (FUN_8002fd04), accented letters (FUN_80047368's extra
//! accent sprite), split-screen layouts and the y/x clip windows.
//!
//! Coordinates are the game's 512x240 screen (draw offset applied).

use crate::le;
use crate::vram::{self, Vram};

const EXE_BASE: u32 = 0x8001_0000;
const EXE_HDR: usize = 0x800;

fn exe_off(addr: u32) -> Option<usize> {
    (addr as usize).checked_sub(EXE_BASE as usize).map(|o| o + EXE_HDR)
}
fn exe_u32(exe: &[u8], addr: u32) -> Option<u32> {
    let o = exe_off(addr)?;
    exe.get(o..o + 4).map(|_| le::u32(exe, o))
}
fn exe_bytes(exe: &[u8], addr: u32, n: usize) -> Option<&[u8]> {
    let o = exe_off(addr)?;
    exe.get(o..o + n)
}
fn exe_str(exe: &[u8], addr: u32) -> Option<String> {
    let s = exe.get(exe_off(addr)?..)?;
    let n = s.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&s[..n]).into_owned())
}
fn exe_ptr_str(exe: &[u8], ptr_addr: u32) -> Option<String> {
    exe_str(exe, exe_u32(exe, ptr_addr)?)
}

/// Exe addresses of the tables the HUD code reads [CODE].
pub mod addr {
    /// 0xC0 glyphs x (u, v, width, present): 0x00..0x3F 12 px font (char -
    /// 0x20), 0x40..0x7F 16 px (char + 0x20), 0x80..0xBF 24 px (char + 0x60).
    pub const GLYPHS: u32 = 0x800A_BC18;
    /// 16 bytes: colour index -> CLUT offset (sub-palette of slot 0xF's row).
    pub const COLOURS: u32 = 0x800A_BC0C;
    /// 6 x i8: x offset of the position suffix for places 1..6.
    pub const SUFFIX_DX: u32 = 0x800B_06A4;
    /// 6 pointers, stride 8: "ST", "ND", "RD", "TH", ...
    pub const SUFFIX_PTRS: u32 = 0x800A_B3F4;
    pub const PTR_LAP: u32 = 0x800B_0200;
    pub const PTR_COPY_THE_BUTTONS: u32 = 0x800B_057C;
    pub const PTR_READY: u32 = 0x800B_0340;
    pub const PTR_RACE_1_LAP: u32 = 0x800B_0450;
    pub const PTR_RACE_N_LAPS: u32 = 0x800B_0454;
    pub const PTR_NO_LAPS: u32 = 0x800B_0210;
    pub const PTR_TOTAL_TIME: u32 = 0x800B_0214;
    pub const PTR_DID_NOT_FINISH: u32 = 0x800B_0218;
    pub const PTR_HITS: u32 = 0x800B_0228;
    pub const PTR_BEST_LAP: u32 = 0x800B_0354;
    pub const PTR_PRESS_START: u32 = 0x800B_0284;
    pub const PTR_YOU_BEAT: u32 = 0x800B_0234;
    pub const PTR_BEAT_YOU: u32 = 0x800B_0238;
    /// The stunt trial (phase 3): "FINISH THE COURSE", "COURSE FINISHED",
    /// "OUT OF TIME", "TIME".
    pub const PTR_FINISH_THE_COURSE: u32 = 0x800B_0588;
    pub const PTR_COURSE_FINISHED: u32 = 0x800B_0580;
    pub const PTR_OUT_OF_TIME: u32 = 0x800B_0258;
    pub const PTR_TIME: u32 = 0x800B_0264;
    /// Battles (phase 4): the intro "BATTLE THE MUPPETS", the result's
    /// "DAMAGE", "BEST CRASH", "KNOCKOUTS".
    pub const PTR_BATTLE_THE_MUPPETS: u32 = 0x800B_034C;
    pub const PTR_DAMAGE: u32 = 0x800B_021C;
    pub const PTR_BEST_CRASH: u32 = 0x800B_05BC;
    pub const PTR_KNOCKOUTS: u32 = 0x800B_05B8;
    /// Character names, pointers at stride 8 ("KERMIT", "GONZO", ...).
    pub const NAMES: u32 = 0x800A_B128;
}

/// Texture slot (0x800DFF00 table, RAW.md) the font lives in.
pub const FONT_SLOT: u8 = 0xF;
/// Texture slot of the minigame's button icons.
pub const BUTTON_SLOT: u8 = 5;

/// Font sizes of FUN_800474e8's 5th argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 0: 16 px cells, spacing 3.
    Medium,
    /// 1: 24 px cells, spacing 5.
    Large,
    /// 2: 12 px cells, spacing 2.
    Small,
}

impl Size {
    pub fn from_arg(a: u8) -> Size {
        match a {
            1 => Size::Large,
            2 => Size::Small,
            _ => Size::Medium,
        }
    }
    pub fn cell_h(self) -> i32 {
        match self {
            Size::Large => 0x18,
            Size::Small => 0xC,
            Size::Medium => 0x10,
        }
    }
    pub fn spacing(self) -> i32 {
        match self {
            Size::Large => 5,
            Size::Small => 2,
            Size::Medium => 3,
        }
    }
    fn glyph_base(self) -> i32 {
        match self {
            Size::Large => 0x60,
            Size::Small => -0x20,
            Size::Medium => 0x20,
        }
    }
}

/// One textured primitive of the HUD: a SPRT (w == uw, h == vh) or a
/// POLY_FT4 stretching (u, v, uw, vh) over (x, y, w, h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub u: u8,
    pub v: u8,
    pub uw: i32,
    pub vh: i32,
    /// Texture slot (0x800DFF00 table): tpage = slot.tpage + abr bits,
    /// clut = slot.clut + clut_off.
    pub slot: u8,
    pub clut_off: u16,
    /// Blend colour (0x80 = 1.0); ignored when `raw`.
    pub rgb: [u8; 3],
    pub raw: bool,
    /// Semi-transparent (abr from `abr`) for texels with the STP bit.
    pub semi: bool,
    /// Blend mode 0..3 (0 = B/2+F/2, 1 = B+F, 2 = B-F, 3 = B+F/4).
    pub abr: u8,
}

impl Sprite {
    /// GPU texpage attribute and CLUT attribute given the slot table.
    pub fn attrs(&self, slots: &[(u16, u16); 32]) -> (u16, u16) {
        let (tp, cl) = slots[self.slot as usize & 31];
        ((tp & !0x60) | ((self.abr as u16) << 5), cl.wrapping_add(self.clut_off))
    }
}

/// Glyph table entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    pub u: u8,
    pub v: u8,
    pub w: u8,
    pub present: bool,
}

/// The HUD's tables, read from the executable.
#[derive(Clone, Debug)]
pub struct HudTables {
    pub glyphs: Vec<Glyph>,
    pub colours: [u8; 16],
    pub suffix_dx: [i8; 6],
    pub suffixes: Vec<String>,
    pub lap: String,
    pub copy_the_buttons: String,
    pub ready: String,
    pub race_1_lap: String,
    pub race_n_laps: String,
    pub best_lap: String,
    pub no_laps: String,
    pub total_time: String,
    pub did_not_finish: String,
    pub hits: String,
    pub press_start: String,
    pub you_beat: String,
    /// "BEAT" (the "BEAT x" race's intro, FUN_80050F64).
    pub beat: String,
    pub beat_you: String,
    pub names: Vec<String>,
    pub finish_the_course: String,
    pub course_finished: String,
    pub out_of_time: String,
    pub time: String,
    pub battle_the_muppets: String,
    pub damage: String,
    pub best_crash: String,
    pub knockouts: String,
}

impl HudTables {
    pub fn from_exe(exe: &[u8]) -> Option<HudTables> {
        let g = exe_bytes(exe, addr::GLYPHS, 0xC0 * 4)?;
        let glyphs = g.chunks_exact(4).map(|c| Glyph { u: c[0], v: c[1], w: c[2], present: c[3] != 0 }).collect();
        let mut colours = [0u8; 16];
        colours.copy_from_slice(exe_bytes(exe, addr::COLOURS, 16)?);
        let mut suffix_dx = [0i8; 6];
        for (i, b) in exe_bytes(exe, addr::SUFFIX_DX, 6)?.iter().enumerate() {
            suffix_dx[i] = *b as i8;
        }
        let suffixes = (0..6).map(|i| exe_ptr_str(exe, addr::SUFFIX_PTRS + 8 * i)).collect::<Option<Vec<_>>>()?;
        Some(HudTables {
            glyphs,
            colours,
            suffix_dx,
            suffixes,
            lap: exe_ptr_str(exe, addr::PTR_LAP)?,
            copy_the_buttons: exe_ptr_str(exe, addr::PTR_COPY_THE_BUTTONS)?,
            ready: exe_ptr_str(exe, addr::PTR_READY)?,
            race_1_lap: exe_ptr_str(exe, addr::PTR_RACE_1_LAP)?,
            race_n_laps: exe_ptr_str(exe, addr::PTR_RACE_N_LAPS)?,
            best_lap: exe_ptr_str(exe, addr::PTR_BEST_LAP)?,
            no_laps: exe_ptr_str(exe, addr::PTR_NO_LAPS)?,
            total_time: exe_ptr_str(exe, addr::PTR_TOTAL_TIME)?,
            did_not_finish: exe_ptr_str(exe, addr::PTR_DID_NOT_FINISH)?,
            hits: exe_ptr_str(exe, addr::PTR_HITS)?,
            press_start: exe_ptr_str(exe, addr::PTR_PRESS_START)?,
            you_beat: exe_ptr_str(exe, addr::PTR_YOU_BEAT)?,
            beat_you: exe_ptr_str(exe, addr::PTR_BEAT_YOU)?,
            beat: exe_ptr_str(exe, 0x800B_031C)?,
            names: (0..25).map(|i| exe_ptr_str(exe, addr::NAMES + 8 * i)).collect::<Option<Vec<_>>>()?,
            finish_the_course: exe_ptr_str(exe, addr::PTR_FINISH_THE_COURSE)?,
            course_finished: exe_ptr_str(exe, addr::PTR_COURSE_FINISHED)?,
            out_of_time: exe_ptr_str(exe, addr::PTR_OUT_OF_TIME)?,
            time: exe_ptr_str(exe, addr::PTR_TIME)?,
            battle_the_muppets: exe_ptr_str(exe, addr::PTR_BATTLE_THE_MUPPETS)?,
            damage: exe_ptr_str(exe, addr::PTR_DAMAGE)?,
            best_crash: exe_ptr_str(exe, addr::PTR_BEST_CRASH)?,
            knockouts: exe_ptr_str(exe, addr::PTR_KNOCKOUTS)?,
        })
    }

    /// Map a string byte to a glyph index the way FUN_800474e8 does, or None
    /// to skip it. `special` = the lowercase t/o/x/s glyphs (forced CLUT 4).
    fn map_char(c: u8, size: Size) -> Option<(i32, bool)> {
        let (c, special) = match c {
            0xA8 => (0x40, false),
            0xF8 => (0x26, false),
            b't' => (0x5B, true),
            b'o' => (0x5C, true),
            b'x' => (0x5D, true),
            b's' => (0x5E, true),
            0x61..=0x7A => return None,
            0x20..=0x5F => (c, false),
            0x80..=0xFF => (accent_base(c)?, false),
            _ => return None,
        };
        Some((c as i32 + size.glyph_base(), special))
    }

    /// FUN_800474e8(s, x, y, align, size, colour, abr): one sprite per glyph,
    /// in the order the game adds them. `align` bits: (a&5)==4 bottom,
    /// (a&5)==1 top, else vertically centred; (a&10)==8 right, (a&10)==2
    /// left, else centred. `abr` is the game's 7th argument (0, 0x20, 0x40,
    /// 0x60; 0x60 = opaque).
    pub fn text(&self, s: &[u8], x: i32, y: i32, align: u8, size: Size, colour: u8, abr: u8) -> Vec<Sprite> {
        self.text_w(s, x, y, align, size, colour, abr).0
    }

    /// `text` plus the measured width the game keeps in DAT_800B0B22
    /// (callers place labels next to the previous string with it).
    #[allow(clippy::too_many_arguments)]
    pub fn text_w(&self, s: &[u8], x: i32, y: i32, align: u8, size: Size, colour: u8, abr: u8) -> (Vec<Sprite>, i32) {
        self.text_w_rgb(s, x, y, align, size, colour, abr, [0x80; 3])
    }

    /// `text` with the blend colour DAT_800B068C..E (the menus' tints and
    /// the selected item's pulse): any colour but (0x80, 0x80, 0x80)
    /// makes the glyphs modulated (SPRT 0x64) instead of raw (0x65); the
    /// t / o / x / s button glyphs stay raw [CODE 800474e8].
    #[allow(clippy::too_many_arguments)]
    pub fn text_rgb(&self, s: &[u8], x: i32, y: i32, align: u8, size: Size, colour: u8, abr: u8, rgb: [u8; 3]) -> Vec<Sprite> {
        self.text_w_rgb(s, x, y, align, size, colour, abr, rgb).0
    }

    /// `text_w` with the blend colour (`text_rgb`).
    #[allow(clippy::too_many_arguments)]
    pub fn text_w_rgb(&self, s: &[u8], x: i32, y: i32, align: u8, size: Size, colour: u8, abr: u8, rgb: [u8; 3]) -> (Vec<Sprite>, i32) {
        let ch = size.cell_h();
        let sp = size.spacing();
        let mut y = y;
        if align & 5 == 4 {
            y = y + 1 - ch;
        } else if align & 5 != 1 {
            y -= ch >> 1;
        }
        // measure
        let mut width = 0;
        let mut i = 0;
        while i < s.len() && s[i] != 0 {
            let c = s[i];
            if c == b'#' && s.get(i + 1).is_some_and(|&n| n != 0) {
                i += 2;
                continue;
            }
            if let Some((g, _)) = Self::map_char(c, size) {
                width += self.glyphs[g as usize].w as i32 + 1 - sp;
            }
            i += 1;
        }
        width += sp;
        let mut x = x;
        if align & 10 == 8 {
            x = x + 1 - width;
        } else if align & 10 != 2 {
            x -= width / 2;
        }
        // bVar2: raw-textured when the blend colour is (0x80, 0x80, 0x80)
        // and abr is not 0x20.
        let neutral_raw = abr != 0x20 && rgb == [0x80; 3];
        let semi = abr != 0x60;
        let abr_mode = (abr >> 5) & 3;
        let mut clut_off = self.colours[colour as usize & 15] as u16;
        let mut out = Vec::new();
        // Clip windows (1 player: x 0..0x1FF = DAT_800B0E9E..DAT_800B0EA4,
        // y 0..0xEF = DAT_800B0688..DAT_800B068A) [CODE 800474e8] [RAM]:
        // the whole string is skipped when its row misses the y window or
        // its x span misses the screen; glyphs wholly outside x are skipped,
        // rows cut by the y window are trimmed (v and h adjusted).
        let (cy0, cy1, cx0, cx1) = (0, 0xEF, 0, 0x1FF);
        if y > cy1 || y + ch - 1 < cy0 || x >= 0x201 || x + width <= -1 {
            return (out, width);
        }
        let (mut gy, mut gh, mut dv) = (y, ch, 0);
        if gy < cy0 {
            dv = cy0 - gy;
            gh -= dv;
            gy = cy0;
        }
        if gy + gh - 1 > cy1 {
            gh -= gy + gh - 1 - cy1;
        }
        let mut i = 0;
        while i < s.len() && s[i] != 0 {
            let c = s[i];
            if c == b'#' {
                match s.get(i + 1).copied() {
                    Some(n @ b'0'..=b'9') => {
                        clut_off = self.colours[(n - b'0') as usize] as u16;
                        i += 2;
                        continue;
                    }
                    Some(n @ b'a'..=b'f') => {
                        // 0x800ABBA8 + c + 3 == colours[c - 'a'] [CODE]
                        clut_off = self.colours[(n - b'a') as usize] as u16;
                        i += 2;
                        continue;
                    }
                    Some(0) | None => {}
                    Some(_) => {
                        i += 2;
                        continue;
                    }
                }
            }
            if let Some((g, special)) = Self::map_char(c, size) {
                let gl = self.glyphs[g as usize];
                if gl.present {
                    if c != b' ' && x <= cx1 && x + gl.w as i32 - 1 >= cx0 {
                        out.push(Sprite {
                            x,
                            y: gy,
                            w: gl.w as i32,
                            h: gh,
                            u: gl.u,
                            v: gl.v.wrapping_add(dv as u8),
                            uw: gl.w as i32,
                            vh: gh,
                            slot: FONT_SLOT,
                            clut_off: if special { 4 } else { clut_off },
                            rgb: if special { [0x80; 3] } else { rgb },
                            raw: special || neutral_raw,
                            semi,
                            abr: abr_mode,
                        });
                    }
                    x += gl.w as i32 + 1 - sp;
                }
            }
            i += 1;
        }
        (out, width)
    }
}

/// FUN_80047368: accented capitals -> base letter (the accent itself is an
/// extra sprite the HUD never needs; not ported).
fn accent_base(c: u8) -> Option<u8> {
    Some(match c {
        0x81 | 0x9A | 0x96 | 0xEA | 0x97 | 0xEB | 0xA3 | 0xE9 => b'U',
        0x82 | 0x90 | 0x88 | 0xD2 | 0x89 | 0xD3 | 0x8A | 0xD4 => b'E',
        0x83 | 0xB6 | 0x84 | 0x8E | 0x85 | 0xB7 | 0xA0 | 0xB5 | 0xC6 | 0xC7 => b'A',
        0x8B | 0xD8 | 0x8C | 0xD7 | 0x8D | 0xDE | 0xA1 | 0xD6 => b'I',
        0x93 | 0xE2 | 0x94 | 0x99 | 0x95 | 0xE3 | 0xA2 | 0xE0 | 0xE4 | 0xE5 => b'O',
        0xA4 | 0xA5 => b'N',
        0xEC | 0xED => b'Y',
        _ => return None,
    })
}

/// The "copy the buttons" minigame shown during the countdown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Minigame {
    /// Kart +0x4DA: the button to copy (0 left, 1 up, 2 right, 3 down icon).
    pub button: u8,
    /// Kart +0x446: the count shown in the middle.
    pub count: i16,
    /// The lit icon's brightness wobble: a signed byte of the shared random
    /// stream (DAT_800B0BCC); brightness = 0x80 + byte / 8. 0 = steady.
    pub flicker: i8,
}

/// Everything the minimal HUD reads from the game state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudState {
    /// DAT_800B0C70: countdown, game units (dt per frame), 0 = racing.
    pub countdown: i32,
    /// DAT_800B07F8: the countdown value where the start sequence begins
    /// (above it the intro "RACE 3 LAPS" shows).
    pub countdown_start: i32,
    /// DAT_800B0D46: game mode (1 = race).
    pub mode: u8,
    /// Kart +0x4BF: laps completed (0 on the first lap).
    pub lap: u8,
    /// DAT_800B0DA6: laps in the race.
    pub laps: u8,
    /// DAT_800B0A94: race clock (5 per VBlank; /3 = hundredths).
    pub race_time: i32,
    /// Kart +0x4CA: the place shown (1..6).
    pub position: u8,
    /// Kart +0x4C1 (the HUD draws the place semi-transparent while lap == 0
    /// and this is set).
    pub position_flag: u8,
    /// Kart +0x4C7: finished (the race HUD gives way to results).
    pub finished: bool,
    pub minigame: Option<Minigame>,
    /// The "BEAT x" race (race flag 0x10; phase 6): kart 1's character
    /// (0x800C3A5E), named in the intro.
    pub rival: Option<u8>,
    /// After the player's finish: the results panel (FUN_8004cea0) and
    /// "PRESS START". Drawn when `finished` or in mode 10.
    pub results: Option<Results>,
    /// The stunt trial (race type 3): its HUD instead of the race's.
    pub stunt: Option<StuntHud>,
    /// A battle (game mode 2; phase 4): its HUD instead of the race's.
    pub battle: Option<BattleHud>,
    /// ADVENTURE (game mode 0xB; phase 6): its HUD instead of the race's.
    pub adventure: Option<AdventureHud>,
}

/// What ADVENTURE's HUD reads (game modes 0xB and 10; FUN_8004F1C8,
/// FUN_80050B2C, 0x800501E4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdventureHud {
    /// 0x800B0FAC (the target icon per course).
    pub course: u8,
    /// The course's save flags (4 targets, 8 boxes, 0x10 tokens found).
    pub course_flags: u16,
    /// Left to collect: 0x800B0B60 - +0x4AD, 0x800B0A52 - +0x4AE,
    /// 0x800B0EBC - +0x4AC.
    pub boxes_left: i32,
    pub tokens_left: i32,
    pub targets_left: i32,
    /// Kart +0x4F4 (the last token's colour), 0x800B0B90 (the spin).
    pub token_colour: u8,
    pub token_spin: i16,
    /// 0x800ACF30[race type].
    pub time_limit: i32,
    /// 0x800B0803.
    pub done: u8,
    /// A stunt course (race type 3): the clock at (0x1C0, 0xD2), else
    /// (0x100, 0x1E) (FUN_80050B2C).
    pub stunt: bool,
}

/// What a battle's HUD reads (race type 2, game modes 2 and 10): the
/// result FUN_8004DAE0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BattleHud {
    /// Kart +0x43C / +0x43E (damage dealt / taken), +0x478 (best crash),
    /// +0x4FB (knock-outs).
    pub dealt: i16,
    pub taken: i16,
    pub best_crash: i16,
    pub knockouts: u8,
    /// Kart +0x38C: the result's page (% 180: damage, best crash,
    /// knock-outs, 60 VBlanks each) and the record flash.
    pub counter: i32,
    /// Kart +0x4CA (the place shown), +0x4BE (the place).
    pub shown_place: u8,
    pub place: u8,
    /// Kart +0x358: the battle clock (180000 = out of time).
    pub time: i32,
    /// The course's record (0x800E0190 + 0x20 * world + 8 * slot) and the
    /// race flags (the flash needs 0).
    pub record: i32,
    pub race_flags: u16,
}

/// What the stunt trial's HUD reads (race type 3, game modes 3 and 10):
/// the progress bar FUN_80047F1C, the result FUN_8004E2A8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StuntHud {
    /// Kart +0x490 * 0x80 + +0x4A8: its place along the strip.
    pub progress: i32,
    /// 0x800B0CC8: the strip's quads (0: no bar face).
    pub quads: i16,
    /// 0x800B0C90 / 0x800B0E94: the banners' places (start, finish).
    pub marks: [i32; 2],
    pub n_marks: u8,
    /// Race flags 0x800B07E2 (4: reversed; the record flash needs 0).
    pub race_flags: u16,
    /// Kart +0x4B2 (the bar's face), +0x496 (flags 0x60: the spinning face).
    pub character: u8,
    pub kart_flags: u16,
    /// Kart +0x358 (the trial time; 180000 = out of time), +0x38C (the
    /// record flash counter), +0x4BE / +0x4CA (place).
    pub time: i32,
    pub flash: i32,
    pub place: u8,
    pub shown_place: u8,
    /// The course's record (0x800E0190 + 0x20 * world + 8 * slot).
    pub record: i32,
    /// 0x800B0C20: the first finisher's time (the "+" gap from 2nd down).
    pub first_time: i32,
}

/// What FUN_8004cea0 (the 1-player finish/results panel) and the
/// "PRESS START" prompt read. Times are clock units (300 = 1 s).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Results {
    /// Kart +0x384: best lap (0 = no lap completed).
    pub best_lap: i32,
    /// The best lap equals the course record (0x800E0190 table) and
    /// DAT_800B07E2 & 2 is clear: the game flashes it orange.
    pub best_lap_record: bool,
    /// Kart +0x38C: the counter the record flash follows ((c & 0x1F) < 0x10 = orange).
    pub flash_counter: i32,
    /// Sum of the kart's lap times +0x35C.. over the race's laps.
    pub total_time: i32,
    /// Kart +0x4BE: final place (the "+gap" shows from 2nd down).
    pub place: u8,
    /// Sum of the race leader's (kart DAT_800D8F94) lap times.
    pub leader_total: i32,
    /// Kart +0x492 / +0x494: hits given / taken.
    pub hits_given: i16,
    pub hits_taken: i16,
    /// "PRESS START" shown (DAT_800B0F88 or DAT_800B0EBE set).
    pub press_start: bool,
    /// The frame counter the prompt blinks with (shown while c % 60 < 45).
    pub frame: i32,
    /// Rival races (DAT_800B07E2 & 0x10): the rival's character
    /// (DAT_800B07FE) for "YOU BEAT x" (place 1) / "x BEAT YOU", blinking
    /// with DAT_800B0AF0 (shown while % 60 < 45). None in a normal race.
    pub rival: Option<u8>,
    pub rival_blink: i32,
}

/// "%i:%i%i.%i%i" (hundredths) as FUN_8004cea0 formats times; `plus` = the
/// "+%i:%i%i.%i%i" gap format.
pub fn time_text_cs(t: i32, plus: bool) -> String {
    let t = t / 3;
    let m = t / 6000;
    let s = t / 100 - m * 60;
    let c = t % 100;
    format!("{}{}:{}{}.{}{}", if plus { "+" } else { "" }, m, s / 10, s % 10, c / 10, c % 10)
}

impl Default for HudState {
    fn default() -> Self {
        HudState {
            countdown: 0,
            countdown_start: 300,
            mode: 1,
            lap: 0,
            laps: 3,
            race_time: 0,
            position: 6,
            position_flag: 0,
            finished: false,
            minigame: None,
            results: None,
            stunt: None,
            battle: None,
            adventure: None,
            rival: None,
        }
    }
}

/// Kart record offsets the HUD reads.
pub mod kart {
    pub const MINIGAME_COUNT: usize = 0x446;
    pub const LAP: usize = 0x4BF;
    pub const POS_FLAG: usize = 0x4C1;
    pub const FINISHED: usize = 0x4C7;
    pub const PLACE: usize = 0x4CA;
    pub const MINIGAME_BUTTON: usize = 0x4DA;
    /// Race time of this kart (sum of its lap times, same units as DAT_800B0A94).
    pub const RACE_TIME: usize = 0x358;
    pub const LAP_TIMES: usize = 0x35C;
    pub const BEST_LAP: usize = 0x384;
    pub const COUNTER: usize = 0x38C;
    pub const HITS_GIVEN: usize = 0x492;
    pub const HITS_TAKEN: usize = 0x494;
    pub const RACE_POS: usize = 0x4BE;
}

impl Results {
    /// Panel fields from the player's record, the leader's record and the
    /// lap count. `best_lap_record`, `press_start` and `frame` stay default.
    pub fn from_records(rec: &[u8], leader: &[u8], laps: u8) -> Results {
        let sum = |r: &[u8]| (0..laps as usize).map(|k| le::i32(r, kart::LAP_TIMES + 4 * k)).sum();
        Results {
            best_lap: le::i32(rec, kart::BEST_LAP),
            flash_counter: le::i32(rec, kart::COUNTER),
            total_time: sum(rec),
            place: rec[kart::RACE_POS],
            leader_total: sum(leader),
            hits_given: le::i16(rec, kart::HITS_GIVEN),
            hits_taken: le::i16(rec, kart::HITS_TAKEN),
            ..Results::default()
        }
    }
}

impl HudState {
    /// The kart-side fields from a 0x504-byte kart record (the replay sidecar
    /// or mr-sim). Globals (countdown, laps, mode, race clock) stay as given.
    pub fn with_kart(mut self, rec: &[u8]) -> HudState {
        self.lap = rec[kart::LAP];
        self.position_flag = rec[kart::POS_FLAG];
        self.finished = rec[kart::FINISHED] != 0;
        self.position = rec[kart::PLACE];
        self
    }
}

/// The global race clock DAT_800B0A94 estimated from a kart record: the
/// kart's own time +0x358 runs one tick (5*dt = 10 at dt 2) ahead of it
/// [RAM: mode of 60 paired reads during a race; 39 x 10]. Used when only the
/// kart record is known (replay sidecar).
pub fn race_clock_from_kart(rec: &[u8]) -> i32 {
    (le::i32(rec, kart::RACE_TIME) - 10).max(0)
}

/// Format the race clock like FUN_80050b2c ("%i:%i%i.%i").
pub fn timer_text(race_time: i32) -> String {
    let t = race_time.max(0) / 3;
    let m = t / 6000;
    let s = t / 100 - m * 60;
    let c = (t % 100) / 10;
    format!("{}:{}{}.{}", m, s / 10, s % 10, c)
}

impl HudTables {
    /// The 1-player race HUD for `st`, in the order the game adds the
    /// primitives (FUN_80050f64, FUN_8004f688 case 1, FUN_80050b2c).
    pub fn layout(&self, st: &HudState) -> Vec<Sprite> {
        let mut out = Vec::new();
        let c70 = st.countdown;
        // FUN_80050f64: pre-race texts
        if c70 != 0 {
            if c70 < st.countdown_start {
                if st.mode == 1 && c70 > 0x86 {
                    out.extend(self.text(self.copy_the_buttons.as_bytes(), 0x100, 0xD2, 0, Size::Large, 7, 0x60));
                }
                if c70 <= 0x77 && c70 % 0x1E >= 0xF {
                    out.extend(self.text(self.ready.as_bytes(), 0x100, 0xD2, 0, Size::Large, 6, 0x60));
                }
            } else if st.mode == 1 {
                // the "BEAT x" race (race flag 0x10): "BEAT <kart 1>" above
                if let Some(c) = st.rival {
                    let line = format!("{} {}", self.beat, self.names.get(c as usize).cloned().unwrap_or_default());
                    out.extend(self.text(line.as_bytes(), 0x100, 0x1E, 0, Size::Large, 7, 0x60));
                }
                let s = if st.laps == 1 { self.race_1_lap.clone() } else { self.race_n_laps.replace("%i", &st.laps.to_string()) };
                out.extend(self.text(s.as_bytes(), 0x100, 0xD2, 0, Size::Large, 5, 0x60));
            }
        }
        // FUN_8004f688: minigame while counting down, race HUD after
        if c70 < st.countdown_start {
            if st.mode == 1 && c70 != 0 {
                if let Some(m) = st.minigame {
                    out.extend(self.minigame(&m));
                }
            }
            if c70 <= 0 && st.mode == 1 && !st.finished {
                let abr = if st.lap == 0 && st.position_flag != 0 { 0 } else { 0x60 };
                let (x, y) = (0x100, 0x20);
                let p = st.position.clamp(1, 6);
                out.extend(self.text(&[b'0' + p], x, y, 0, Size::Large, 7, abr));
                let dx = self.suffix_dx[(p - 1) as usize] as i32;
                out.extend(self.text(self.suffixes[(p - 1) as usize].as_bytes(), x + dx, y - 5, 0, Size::Small, 4, abr));
                out.extend(self.text(self.lap.as_bytes(), 0x30, 0xD2, 4, Size::Small, 5, 0x60));
                let l = format!("{}/{}", st.lap as u32 + 1, st.laps);
                out.extend(self.text(l.as_bytes(), 0x30, 0xD1, 1, Size::Medium, 4, 0x60));
            }
        }
        // FUN_8004f688 case 1 with +0x4C7 set, and case 10: FUN_8004cea0
        if st.finished || st.mode == 10 {
            if let Some(r) = st.results {
                out.extend(self.results(st, &r));
            }
        }
        // FUN_80050b2c: race timer (1-player race; not after the finish)
        if c70 == 0 && st.mode == 1 && !st.finished {
            out.extend(self.text(timer_text(st.race_time).as_bytes(), 0x100, 0xD2, 0, Size::Medium, 4, 0x60));
        }
        out
    }

    /// FUN_8004cea0 (1 player; viewport centre (0x100, 0x78)) and the
    /// "PRESS START" prompt (FUN_80051698 / FUN_80050db8), in add order.
    pub fn results(&self, st: &HudState, r: &Results) -> Vec<Sprite> {
        let (cx, cy) = (0x100, 0x78);
        let mut out = Vec::new();
        // best lap
        if r.best_lap == 0 {
            out.extend(self.text(self.no_laps.as_bytes(), cx, cy, 0, Size::Medium, 4, 0x60));
        } else {
            let col = if r.best_lap_record && (r.flash_counter & 0x1F) < 0x10 { 7 } else { 4 };
            let (v, w) = self.text_w(time_text_cs(r.best_lap, false).as_bytes(), cx, cy, 0, Size::Medium, col, 0x60);
            out.extend(v);
            out.extend(self.text(self.best_lap.as_bytes(), cx - w / 2 - 8, cy, 8, Size::Small, 5, 0x60));
        }
        // total time, and the gap to the winner from 2nd place down
        let y = cy + 0x11;
        if st.lap == st.laps {
            let (v, w) = self.text_w(time_text_cs(r.total_time, false).as_bytes(), cx, y, 0, Size::Medium, 4, 0x60);
            out.extend(v);
            let half = w / 2;
            out.extend(self.text(self.total_time.as_bytes(), cx - half - 8, y, 8, Size::Small, 5, 0x60));
            if r.place >= 2 {
                let gap = time_text_cs(r.total_time - r.leader_total, true);
                out.extend(self.text(gap.as_bytes(), cx + half + 8, y, 2, Size::Small, 7, 0x60));
            }
        } else {
            out.extend(self.text(self.did_not_finish.as_bytes(), cx, y, 0, Size::Medium, 4, 0x60));
        }
        // hits
        let y = cy + 0x22;
        let hits = format!("+{} -{}", r.hits_given, r.hits_taken);
        let (v, w) = self.text_w(hits.as_bytes(), cx, y, 0, Size::Medium, 4, 0x60);
        out.extend(v);
        out.extend(self.text(self.hits.as_bytes(), cx - w / 2 - 8, y, 8, Size::Small, 5, 0x60));
        // place (+0x4CA)
        let p = st.position.clamp(1, 6);
        let y = cy - 0x18;
        out.extend(self.text(&[b'0' + p], cx, y, 0, Size::Large, 7, 0x60));
        let dx = self.suffix_dx[(p - 1) as usize] as i32;
        out.extend(self.text(self.suffixes[(p - 1) as usize].as_bytes(), cx + dx, y - 5, 0, Size::Small, 4, 0x60));
        // FUN_8004CEA0 tail: "YOU BEAT x" / "x BEAT YOU" in rival races
        if let Some(c) = r.rival {
            if r.rival_blink.rem_euclid(0x3C) < 0x2D {
                let name = self.names.get(c as usize).cloned().unwrap_or_default();
                let line = if r.place == 1 { format!("{} {}", self.you_beat, name) } else { format!("{} {}", name, self.beat_you) };
                out.extend(self.text(line.as_bytes(), 0x100, 0x1E, 0, Size::Medium, 6, 0x60));
            }
        }
        // PRESS START, blinking
        if r.press_start && r.frame.rem_euclid(0x3C) < 0x2D {
            out.extend(self.text(self.press_start.as_bytes(), 0x100, 0xD2, 0, Size::Medium, 4, 0x60));
        }
        out
    }

    /// FUN_8004E2A8 (1 player; viewport centre (0x100, 0x78)): the stunt
    /// trial's result: "OUT OF TIME", or the time (orange while it is the
    /// course record and the flash counter's bit 4 is clear), "TIME", the
    /// gap to the first from 2nd place down, "COURSE FINISHED".
    pub fn stunt_results(&self, s: &StuntHud) -> Vec<Sprite> {
        let (cx, cy) = (0x100, 0x78);
        let mut out = Vec::new();
        if s.time == 180_000 {
            out.extend(self.text(self.out_of_time.as_bytes(), cx, 0x1E, 0, Size::Large, 7, 0x60));
            return out;
        }
        let col = if s.race_flags == 0 && s.time == s.record && (s.flash & 0x1F) < 0x10 { 7 } else { 4 };
        let (v, w) = self.text_w(time_text_cs(s.time, false).as_bytes(), cx, cy, 0, Size::Medium, col, 0x60);
        out.extend(v);
        let half = w / 2;
        out.extend(self.text(self.time.as_bytes(), cx - half - 8, cy, 8, Size::Small, 5, 0x60));
        if s.place > 1 {
            out.extend(self.text(time_text_cs(s.time - s.first_time, true).as_bytes(), cx + half + 8, cy, 2, Size::Small, 7, 0x60));
        }
        out.extend(self.text(self.course_finished.as_bytes(), cx, 0x1E, 0, Size::Large, 5, 0x60));
        out
    }

    /// FUN_8004DAE0 (1 player, no rival; viewport centre (0x100, 0x78)):
    /// the battle's result: one of DAMAGE "+dealt -taken", BEST CRASH,
    /// KNOCKOUTS (60 VBlanks each by +0x38C % 180), the place, the time
    /// (orange while it is the course record with race flags 0 and the
    /// flash bit clear) and "TIME", or "OUT OF TIME".
    pub fn battle_results(&self, b: &BattleHud) -> Vec<Sprite> {
        let (cx, cy) = (0x100, 0x78);
        let mut out = Vec::new();
        let k = b.counter % 0xB4;
        let (v, label) = if k < 0x3C {
            (format!("+{} -{}", b.dealt, b.taken), &self.damage)
        } else if k < 0x78 {
            (b.best_crash.to_string(), &self.best_crash)
        } else {
            (b.knockouts.to_string(), &self.knockouts)
        };
        let (s, w) = self.text_w(v.as_bytes(), cx, cy, 0, Size::Medium, 4, 0x60);
        out.extend(s);
        out.extend(self.text(label.as_bytes(), cx - w / 2 - 8, cy, 8, Size::Small, 5, 0x60));
        let p = b.shown_place.clamp(1, 6);
        let y = cy - 0x18;
        out.extend(self.text(&[b'0' + p], cx, y, 0, Size::Large, 7, 0x60));
        let dx = self.suffix_dx[(p - 1) as usize] as i32;
        out.extend(self.text(self.suffixes[(p - 1) as usize].as_bytes(), cx + dx, y - 5, 0, Size::Small, 4, 0x60));
        if b.time < 180_000 {
            let col = if b.race_flags == 0 && b.place == 1 && b.time == b.record && (b.counter & 0x1F) < 0x10 { 7 } else { 4 };
            let y = cy + 0x11;
            let (s, w) = self.text_w(time_text_cs(b.time, false).as_bytes(), cx, y, 0, Size::Medium, col, 0x60);
            out.extend(s);
            out.extend(self.text(self.time.as_bytes(), cx - w / 2 - 8, y, 8, Size::Small, 5, 0x60));
        } else {
            out.extend(self.text(self.out_of_time.as_bytes(), cx, cy + 0x12, 0, Size::Medium, 4, 0x60));
        }
        out
    }

    /// FUN_8004e8e8 (1 player): the four button icons (32x24 cells of slot 5)
    /// around the count. The sparkles (FUN_8002fd04) are not drawn.
    pub fn minigame(&self, m: &Minigame) -> Vec<Sprite> {
        let yc = 0x3C;
        let cells: [(i32, i32, u8, u8); 4] = [(0xD0, yc - 0xC, 0, 0), (0xF0, yc - 0x21, 0x20, 0), (0x110, yc - 0xC, 0, 0x18), (0xF0, yc + 9, 0x20, 0x18)];
        let mut out = Vec::new();
        for (k, (x, y, u, v)) in cells.iter().enumerate() {
            let b = if m.button as usize == k { (0x80 + m.flicker as i32 / 8) as u8 } else { 0x40 };
            out.push(Sprite {
                x: *x,
                y: *y,
                w: 0x20,
                h: 0x18,
                u: *u,
                v: *v,
                uw: 0x20,
                vh: 0x18,
                slot: BUTTON_SLOT,
                clut_off: 0,
                rgb: [b; 3],
                raw: false,
                semi: false,
                abr: 3,
            });
        }
        out.extend(self.text(m.count.to_string().as_bytes(), 0x100, yc, 0, Size::Medium, 4, 0x60));
        out
    }
}

/// Texture slot attributes (tpage, clut) from the RAW decoder's slot table.
pub fn slot_attrs(slots: &[Option<crate::raw::TexSlot>; 32]) -> [(u16, u16); 32] {
    let mut out = [(0u16, 0x7FE8u16); 32];
    for (i, s) in slots.iter().enumerate() {
        if let Some(s) = s {
            out[i] = (s.tpage.unwrap_or(0), s.clut);
        }
    }
    out
}

/// Software reference renderer: draw `sprites` (game order: the GPU draws
/// the last-added first) into a 512x240 RGBA canvas, PS1 rules: texel 0
/// transparent, raw = texel colour, else texel * rgb / 0x80 (clamped),
/// semi-transparent texels (STP bit) blended with abr over what is there.
pub fn render(v: &Vram, slots: &[(u16, u16); 32], sprites: &[Sprite], canvas: &mut [[u8; 4]], cw: usize, chh: usize) {
    for s in sprites.iter().rev() {
        let (tp, cl) = s.attrs(slots);
        let (px, py, depth) = vram::tpage(tp);
        let (cx, cy) = vram::clut(cl);
        for dy in 0..s.h {
            for dx in 0..s.w {
                let (x, y) = (s.x + dx, s.y + dy);
                if x < 0 || y < 0 || x as usize >= cw || y as usize >= chh {
                    continue;
                }
                let u = (s.u as i32 + dx * s.uw / s.w.max(1)) as u8;
                let vv = (s.v as i32 + dy * s.vh / s.h.max(1)) as u8;
                let t = v.texel(depth, px, py, cx, cy, u, vv);
                if t == 0 {
                    continue;
                }
                let c5 = [(t & 31) as i32, ((t >> 5) & 31) as i32, ((t >> 10) & 31) as i32];
                let mut f = [0i32; 3];
                for k in 0..3 {
                    let c8 = c5[k] * 255 / 31;
                    f[k] = if s.raw { c8 } else { (c8 * s.rgb[k] as i32 / 0x80).min(255) };
                }
                let dst = &mut canvas[y as usize * cw + x as usize];
                if s.semi && t & 0x8000 != 0 {
                    for k in 0..3 {
                        let b = dst[k] as i32;
                        dst[k] = match s.abr {
                            0 => (b + f[k]) / 2,
                            1 => (b + f[k]).min(255),
                            2 => (b - f[k]).max(0),
                            _ => (b + f[k] / 4).min(255),
                        } as u8;
                    }
                    dst[3] = dst[3].max(if s.abr == 0 { 128 } else { 255 });
                } else {
                    *dst = [f[0] as u8, f[1] as u8, f[2] as u8, 255];
                }
            }
        }
    }
}
