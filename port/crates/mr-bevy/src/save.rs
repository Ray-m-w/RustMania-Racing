//! The port's save file (phase 2; docs/spec/frontend.md "Saves"): the
//! game's save block 0x800E0190.. (mr_sim::save) in `save.bin` beside the
//! executable (or `--save PATH`): "MRMSAVE1", a format version, the game's
//! own checksum (the word sum its memory-card save carries) and the 0x20C
//! bytes in the game's layout.
//!
//! - At start: save.bin; else a read-only import of this game's save from a
//!   PS1 memory card image (psxrecomp's `saves/card1.mcd` beside the
//!   executable, or `--import-card PATH`); else a new game (FUN_8008F448).
//! - Autosave instead of the MEMORY CARD page: the file is rewritten
//!   whenever the block changes (a race's lap record, won bit, vehicles
//!   won; the progression check between courses; laps; options), where the
//!   game would set "GAME STATUS CHANGED - SAVE GAME".
//! - Options stay in settings.txt (music / sound volume, controller
//!   configuration); the block mirrors them when written.
//! - `unlock_all` (settings, default false) shows everything without
//!   changing the file (the game's own everything code, FUN_8008F884
//!   case 7, applied to a copy).
//! Tool runs (oracle replays, routes, shots) and tests never write.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use bevy::prelude::*;
use mr_sim::save::{SaveBlock, SaveTables};

pub struct SaveState {
    pub block: SaveBlock,
    written: Option<SaveBlock>,
    path: Option<PathBuf>,
    pub tables: SaveTables,
    /// Where the block came from: "file", "card", "new".
    pub source: &'static str,
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf()))
}

fn may_write() -> bool {
    !cfg!(test) && !crate::menus::tool_run()
}

fn load() -> SaveState {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let tables = SaveTables::from_exe(&mr_sim::tables::ExeImage::parse(&exe).expect("parse the executable"));
    let path = crate::settings::arg_value("--save").map(PathBuf::from).or_else(|| exe_dir().map(|d| d.join("save.bin")));
    let new_game = |tables: &SaveTables| {
        let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u32).unwrap_or(1);
        let mut rand = || mr_sim::session::bios_rand(&mut seed);
        SaveBlock::new_game(tables, &mut rand)
    };
    if cfg!(test) {
        return SaveState { block: new_game(&tables), written: None, path: None, tables, source: "new" };
    }
    if let Some(p) = path.as_ref().filter(|p| p.exists()) {
        match std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| SaveBlock::from_file(&b)) {
            Ok(block) => {
                info!("save: loaded {}", p.display());
                return SaveState { written: Some(block.clone()), block, path, tables, source: "file" };
            }
            Err(e) => {
                let bad = p.with_extension("bin.bad");
                warn!("save: {}: {e}; kept as {} and a new game started", p.display(), bad.display());
                if may_write() {
                    let _ = std::fs::rename(p, &bad);
                }
            }
        }
    }
    let cards = crate::settings::arg_value("--import-card").map(PathBuf::from).into_iter().chain(exe_dir().map(|d| d.join("saves").join("card1.mcd")));
    for c in cards {
        if let Some(block) = std::fs::read(&c).ok().and_then(|b| SaveBlock::from_card(&b)) {
            info!("save: imported this game's save from the memory card {} (read only)", c.display());
            return SaveState { block, written: None, path, tables, source: "card" };
        }
    }
    SaveState { block: new_game(&tables), written: None, path, tables, source: "new" }
}

fn state() -> &'static Mutex<SaveState> {
    static S: OnceLock<Mutex<SaveState>> = OnceLock::new();
    S.get_or_init(|| {
        let mut s = load();
        info!("save: {} ({} of 31 complete)", match s.source { "file" => "the save file", "card" => "imported from the memory card", _ => "a new game" }, s.block.status());
        // main()'s FUN_8008BE68 before the first course session
        s.block.progression(&s.tables);
        Mutex::new(s)
    })
}

/// Run `f` on the save state.
pub fn with<R>(f: impl FnOnce(&mut SaveState) -> R) -> R {
    let mut g = state().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

/// The block the menus show: the save, or everything open while the
/// `unlock_all` setting is on.
pub fn view() -> SaveBlock {
    with(|s| {
        let mut b = s.block.clone();
        if crate::settings::Settings::global().bool("unlock_all", false) {
            b.unlock_all(&s.tables);
        }
        b
    })
}

/// Write the file if the block changed since the last write (autosave).
pub fn persist() {
    if !may_write() {
        return;
    }
    with(|s| {
        let st = crate::settings::Settings::global();
        let mv = st.i64("music_volume", 12).clamp(0, 15) as u8;
        let sv = st.i64("sound_volume", 12).clamp(0, 15) as u8;
        s.block.set_volumes(mv, sv);
        s.block.set_alt_controls(0, st.get("controller_config") == Some("B"));
        if s.written.as_ref() == Some(&s.block) {
            return;
        }
        let Some(p) = s.path.clone() else { return };
        let tmp = p.with_extension("bin.tmp");
        match std::fs::write(&tmp, s.block.to_file()).and_then(|_| std::fs::rename(&tmp, &p)) {
            Ok(()) => {
                info!("save: written {}", p.display());
                s.written = Some(s.block.clone());
            }
            Err(e) => warn!("save: cannot write {}: {e}", p.display()),
        }
    })
}

/// main()'s progression check between course sessions (FUN_8008BE68),
/// then autosave. Returns the course the game would load next (a newly
/// opened course; 0x1D = the secret screen).
pub fn between_courses() -> Option<u8> {
    let m = with(|s| s.block.progression(&s.tables));
    persist();
    m
}

/// A race tick's effect on the save (the player's kart, a 1-player race
/// on world / slot `place`): a lap record (FUN_80082708 compares each lap
/// with the block; the session keeps the lowered record in
/// `globals.lap_record`), the vehicles won by specials (0x800E02C0, the
/// session's `items.hit_bits`), and the won bit at the player's first
/// place. Returns true when the block changed.
pub fn race_tick(s: &mr_sim::session::Session, place: Option<(usize, usize)>, plain_race: bool) -> bool {
    with(|st| {
        let before = st.block.clone();
        if let Some((w, t)) = place {
            st.block.lap_record(w, t, s.globals.lap_record, s.kart.character, s.kart.vehicle);
            // 0x800832EC: player 1 first over the line; the bit is set as the
            // kart finishes (race flags 0, or DISCO's 8)
            if plain_race && s.globals.challenge == 0 && s.kart.finished == 1 && s.kart.race_pos == 1 && s.world.course.length.is_some() && st.block.race_won(w, t) {
                info!("save: course {w}/{t} won");
            }
            // The stunt trial (FUN_80081DD8 @0x8008209C): through the last
            // gate with no race flags (not out of time): the won bit
            if s.globals.race_type == 3 && s.globals.race_flags == 0 && s.globals.challenge == 0 && s.kart.finished == 2 && s.world.course.gates.as_ref().is_some_and(|g| g.len() >= 4 && s.kart.gate as usize == g.len() / 2) && st.block.race_won(w, t) {
                info!("save: stunt course {w}/{t} finished");
            }
            // A battle won (FUN_8007916C @0x80079398): a 1-player battle
            // with no race flags, the player the last kart standing
            // (not a BOSS CHALLENGE: game mode 7 sets flag 0x100 instead)
            if s.globals.race_type == 2 && s.globals.race_flags == 0 && s.globals.challenge == 0 && !s.config().boss_challenge && s.kart.finished == 1 && s.kart.race_pos == 1 && st.block.race_won(w, t) {
                info!("save: arena {w}/{t} won");
            }
        }
        // ADVENTURE and its "BEAT x" race (phase 6): the secrets found
        // (course flags 4 / 8 / 0x10 / 0x20 / 0x40 / 0x80) and the
        // characters collected (0x800E02BC), as the race code set them
        // Phase 6b: a BOSS CHALLENGE won (game mode 7, 0x800793E0): flag
        // 0x100 (BATTLE AS BOSS opens)
        if s.globals.challenge != 0 || s.globals.race_flags & 0x10 != 0 || s.config().boss_challenge {
            if let Some((w, t)) = place {
                let f = st.block.flags(w, t);
                if s.globals.adv.course_flags & !f != 0 {
                    st.block.or_flags(w, t, s.globals.adv.course_flags);
                    info!("save: course {w}/{t} secrets {:#x}", s.globals.adv.course_flags);
                }
            }
            let c = st.block.chars();
            if s.globals.adv.characters & !c != 0 {
                st.block.set_u32(0x12C, c | s.globals.adv.characters);
                info!("save: characters {:#x}", c | s.globals.adv.characters);
            }
        }
        for c in 0..25u8 {
            let have = st.block.vehicles(c);
            let o = 0x130 + 8 * c as usize;
            for (i, h) in have.iter().enumerate() {
                let won = s.globals.items.hit_bits.get(2 * c as usize + i).copied().unwrap_or(0);
                if won & !h != 0 {
                    st.block.set_u32(o + 4 * i, h | won);
                }
            }
        }
        st.block != before
    })
}

/// The vehicles won so far, for a race session (its specials' check).
pub fn vehicles_won() -> Vec<u32> {
    with(|s| (0..25u8).flat_map(|c| s.block.vehicles(c)).collect())
}
