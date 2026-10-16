//! TOURNAMENT - 1 PLAYER: the race leagues (phase 2; docs/spec/frontend.md
//! "Leagues"). The game's flow [CODE 80074a94 sub-state 2, 8008b9ac,
//! 8008baf8, 8008c4ac, 800597cc]:
//! - the league page (title, sub-state 2): RACE LEAGUE A / B (the six
//!   worlds' race A / race B courses, 0x800AD078), BATTLE LEAGUE (arenas;
//!   phase 4), RANDOM RACE LEAGUE (six of the twelve race courses, drawn;
//!   open once A and B are won); "?" while closed; LEFT / RIGHT laps 1..10
//!   once the league is won; CROSS loads the first course, where the
//!   character select picks the player's kart;
//! - FUN_8008BAF8 then draws the five opponents (character and own vehicle,
//!   as a race does) who stay for the league;
//! - after each race START on the results goes to RACE POINTS (game mode
//!   0x11, page 1: per kart in finishing order its place points 3 / 2 / 1,
//!   fastest lap 1, best start 1, most hits 1 and the total), CROSS to
//!   TOURNAMENT POINTS (page 2: the table, "AFTER n RACES"), CROSS to the
//!   next course; after the sixth, FINAL TABLE, then the league result in
//!   the save (won when the player tops the table; the best points kept).
//! The port carries the league across its per-course restarts in
//! `league.txt` beside the save file (`--league` on the command line).
//! Not ported: the karts lined up on the standings screens (the port shows
//! the course's orbit behind them), the tie-break race on STUDIO (a tie for
//! first with the player: the port's table keeps the player first, as the
//! game's sort does for equal points), the trophy presentation (course
//! 0x1A, game mode 0x12), BATTLE LEAGUE.

use std::path::PathBuf;

use mr_sim::save::{LeagueKart, RacePoints};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct League {
    /// 0x800C1E48: the league (0 A, 1 B, 2 battle, 3 random).
    pub league: u8,
    /// 0x800C1DEE: the race being run (0..5); 6 after the last.
    pub race: u8,
    /// 0x800C1DE8: the six courses.
    pub courses: [u8; 6],
    /// 0x800C1DF8 / 0x800C1DF0: character / vehicle per kart (0 = player).
    pub karts: [(u8, u8); 6],
    /// 0x800C1E00: league points per kart.
    pub points: [u8; 6],
    /// 0x800C1E08 + 5 * kart: the last race's points.
    pub last: [RacePoints; 6],
    /// The last race's finishing order (kart ids; 0x800D8F94).
    pub finish: [u8; 6],
    /// The opponents are drawn (after the first character select).
    pub started: bool,
}

pub fn path() -> Option<PathBuf> {
    let save = crate::settings::arg_value("--save").map(PathBuf::from).or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("save.bin"))))?;
    Some(save.with_file_name("league.txt"))
}

impl League {
    /// FUN_8008B9AC: the league's courses (the random league: six distinct
    /// of the first twelve, rand() / 0xAAB).
    pub fn new(league: u8, tables: &mr_sim::save::SaveTables, rand: &mut dyn FnMut() -> i32) -> League {
        let mut courses = [0u8; 6];
        if league < 3 {
            for (k, c) in courses.iter_mut().enumerate() {
                *c = tables.league_courses[league as usize][k] as u8;
            }
        } else {
            let all: Vec<i8> = tables.league_courses.iter().flatten().copied().collect();
            let mut used = [false; 12];
            for c in courses.iter_mut() {
                let i = loop {
                    let i = (rand() / 0xAAB) as usize;
                    if i < 12 && !used[i] {
                        break i;
                    }
                };
                used[i] = true;
                *c = all[i] as u8;
            }
        }
        League { league, race: 0, courses, karts: [(0, 0); 6], points: [0; 6], last: [RacePoints::default(); 6], finish: [0, 1, 2, 3, 4, 5], started: false }
    }

    /// FUN_8008BAF8: the player's kart and five opponents (a race's
    /// draw: a racer no other kart has, its own vehicle), points cleared.
    pub fn draw_karts(&mut self, exe: &mr_sim::tables::ExeImage, character: u8, vehicle: u8, rand: &mut dyn FnMut() -> i32) {
        self.karts[0] = (character, vehicle);
        let mut taken = vec![character];
        for k in 1..6 {
            let (c, v) = mr_sim::ai::pick_opponent(exe, &taken, &|_| true, &|_| true, rand);
            taken.push(c);
            self.karts[k] = (c, v);
        }
        self.points = [0; 6];
        self.started = true;
    }

    /// The opponents as a session line-up (slots 1..5).
    pub fn lineup(&self) -> [(u8, u8); 5] {
        [self.karts[1], self.karts[2], self.karts[3], self.karts[4], self.karts[5]]
    }

    /// Entering game mode 0x11 after a league race (FUN_8008C4AC): every
    /// kart's points from the session's finishing order, fastest lap
    /// (0x800B0DD0), best start (0x800B0E8C) and hits (+0x492 / +0x494).
    pub fn score(&mut self, s: &mr_sim::session::Session) {
        let rows = s.results();
        let mut karts = vec![LeagueKart { rank: 5, best_lap: 0, hits: 0, hits_taken: 0 }; 6];
        for (rank, r) in rows.iter().enumerate() {
            if r.slot < 6 {
                karts[r.slot] = LeagueKart { rank, best_lap: r.best_lap, hits: r.hits, hits_taken: r.hits_taken };
                self.finish[rank] = r.slot as u8;
            }
        }
        let pts = mr_sim::save::race_points(&karts, s.globals.lap_threshold, s.globals.start_best_mask);
        for (k, p) in pts.iter().enumerate() {
            self.last[k] = *p;
            self.points[k] = self.points[k].saturating_add(p.total);
        }
    }

    /// 0x800C1E30: the table (kart ids, most points first).
    pub fn table(&self) -> Vec<usize> {
        mr_sim::save::table_order(&self.points)
    }

    // ------------------------------------------------------------ file

    pub fn to_text(&self) -> String {
        let join = |v: &mut dyn Iterator<Item = String>| v.collect::<Vec<_>>().join(" ");
        let mut s = String::new();
        s += &format!("league {}\nrace {}\nstarted {}\n", self.league, self.race, self.started as u8);
        s += &format!("courses {}\n", join(&mut self.courses.iter().map(|c| c.to_string())));
        s += &format!("karts {}\n", join(&mut self.karts.iter().map(|(c, v)| format!("{c}:{v}"))));
        s += &format!("points {}\n", join(&mut self.points.iter().map(|c| c.to_string())));
        s += &format!("finish {}\n", join(&mut self.finish.iter().map(|c| c.to_string())));
        s += &format!("last {}\n", join(&mut self.last.iter().map(|p| format!("{},{},{},{},{}", p.place, p.fastest_lap, p.best_start, p.most_hits, p.total))));
        s
    }

    pub fn from_text(t: &str) -> Option<League> {
        let mut l = League { league: 0, race: 0, courses: [0; 6], karts: [(0, 0); 6], points: [0; 6], last: [RacePoints::default(); 6], finish: [0, 1, 2, 3, 4, 5], started: false };
        let nums = |v: &str| v.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse::<u8>().ok()).collect::<Vec<u8>>();
        for line in t.lines() {
            let (k, v) = line.split_once(' ')?;
            let n = nums(v);
            match k {
                "league" => l.league = *n.first()?,
                "race" => l.race = *n.first()?,
                "started" => l.started = *n.first()? != 0,
                "courses" => l.courses = n.get(..6)?.try_into().ok()?,
                "points" => l.points = n.get(..6)?.try_into().ok()?,
                "finish" => l.finish = n.get(..6)?.try_into().ok()?,
                "karts" => {
                    for k in 0..6 {
                        l.karts[k] = (*n.get(2 * k)?, *n.get(2 * k + 1)?);
                    }
                }
                "last" => {
                    for k in 0..6 {
                        let p = n.get(5 * k..5 * k + 5)?;
                        l.last[k] = RacePoints { place: p[0], fastest_lap: p[1], best_start: p[2], most_hits: p[3], total: p[4] };
                    }
                }
                _ => {}
            }
        }
        (l.league < 4 && l.race <= 6).then_some(l)
    }

    pub fn store(&self) {
        if crate::menus::tool_run() || cfg!(test) {
            return;
        }
        if let Some(p) = path() {
            if let Err(e) = std::fs::write(&p, self.to_text()) {
                bevy::log::warn!("league: cannot write {}: {e}", p.display());
            }
        }
    }

    /// The league of this run: `--league` and league.txt.
    pub fn load() -> Option<League> {
        if !std::env::args().any(|a| a == "--league") {
            return None;
        }
        std::fs::read_to_string(path()?).ok().and_then(|t| League::from_text(&t))
    }

    pub fn clear() {
        if crate::menus::tool_run() || cfg!(test) {
            return;
        }
        if let Some(p) = path() {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        let mut l = League { league: 3, race: 2, courses: [8, 9, 16, 1, 3, 15], karts: [(0, 0), (5, 30), (13, 12), (17, 16), (23, 18), (1, 1)], points: [7, 3, 9, 0, 1, 2], last: [RacePoints::default(); 6], finish: [2, 0, 1, 5, 4, 3], started: true };
        l.last[2] = RacePoints { place: 3, fastest_lap: 1, best_start: 0, most_hits: 1, total: 5 };
        assert_eq!(League::from_text(&l.to_text()), Some(l));
    }
}
