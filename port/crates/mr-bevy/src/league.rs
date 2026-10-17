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
//! Row 12: after the sixth race a tie for first with the player is played
//! off on OVAL (course 0x1A; the battle league on ARENA/BATTLE, 0x1B):
//! START TIE-BREAKER, the tied karts only, its points added as a race's;
//! then the presentation (game mode 0x12 on that course, `PRESENTATION`):
//! no karts (FUN_8008C4AC counts them for the load's game mode 0, oracle:
//! 0x800B0DFE = 0), camera mode 11, TOURNAMENT CHAMPION and
//! the champion's name, the human champion's voice clip 3, the league's
//! result into the save (FUN_8008C4AC @0x8008CE98); START: a tie-break
//! opens that course, which then shows its course menu, else the title.
//! The standings' karts line up in front of the camera (menus.rs
//! `lineup_records`, FUN_8006B1C8).

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
    /// Row 12: the karts tied for first with the player after the sixth
    /// race (0x800C1E49 on; the tie-break race's karts), player first.
    pub tie: Vec<u8>,
    /// Phase 13: TOURNAMENT - n PLAYERS (0x800C1DEF = 0x800B07ED): with
    /// two the league's karts 0 and 1 are the players and only three race
    /// (0x800C1E4B = 3: split screen and one opponent).
    pub players: u8,
}

/// `League::race` of the presentation (game mode 0x12) after the last race.
pub const PRESENTATION: u8 = 7;
/// The tie-break / presentation course: OVAL, ARENA/BATTLE for the battle
/// league (FUN_8008C4AC: 0x800B0FAC - 0x1A < 2).
pub fn final_course(league: u8) -> u8 {
    if league as usize == mr_sim::save::LEAGUE_BATTLE {
        0x1B
    } else {
        0x1A
    }
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
        League { league, race: 0, courses, karts: [(0, 0); 6], points: [0; 6], last: [RacePoints::default(); 6], finish: [0, 1, 2, 3, 4, 5], started: false, tie: Vec::new(), players: 1 }
    }

    /// 0x800C1E4B: the karts of a league race (6; 3 with two players).
    pub fn karts_n(&self) -> usize {
        if self.players == 2 {
            3
        } else {
            6
        }
    }

    /// The tie-break race's human players (the tied karts below
    /// `players`).
    pub fn tie_humans(&self) -> Vec<u8> {
        self.tie.iter().copied().filter(|&k| k < self.players).collect()
    }

    /// After the sixth race: the karts tied on the most points when the
    /// player is one of them (the tie-break race's field), else None.
    pub fn tie_needed(&self) -> Option<Vec<u8>> {
        // (FUN_80074A94 @0x80075xxx: among the 0x800B0DFE karts, at least
        // one human tied: 0x800C1E4A)
        let n = self.karts_n() as u8;
        let top = *self.points[..n as usize].iter().max()?;
        let tied: Vec<u8> = (0..n).filter(|&k| self.points[k as usize] == top).collect();
        (tied.len() > 1 && tied.iter().any(|&k| k < self.players)).then_some(tied)
    }

    /// The tie-break race runs (race 6 with a field).
    pub fn tie_break(&self) -> bool {
        self.race == 6 && self.tie.len() > 1
    }

    /// The session's slot `s` -> the league's kart (the tie-break race's
    /// field is the tied karts).
    pub fn kart_of_slot(&self, s: usize) -> usize {
        if self.tie_break() {
            self.tie.get(s).copied().unwrap_or(0) as usize
        } else {
            s
        }
    }

    /// FUN_8008BAF8: the player's kart and five opponents (a race's
    /// draw: a racer no other kart has, its own vehicle), points cleared.
    pub fn draw_karts(&mut self, exe: &mr_sim::tables::ExeImage, character: u8, vehicle: u8, rand: &mut dyn FnMut() -> i32) {
        self.draw_karts_for(exe, &[(character, vehicle)], rand)
    }

    /// FUN_8008BAF8 for 0x800C1DEF players (phase 13): the players' picks
    /// (0x800B0B28 / 0x800B0C18), then the opponents drawn for the other
    /// league karts (all six, as the game does; three race with two
    /// players).
    pub fn draw_karts_for(&mut self, exe: &mr_sim::tables::ExeImage, picks: &[(u8, u8)], rand: &mut dyn FnMut() -> i32) {
        for (k, &p) in picks.iter().enumerate().take(2) {
            self.karts[k] = p;
        }
        let mut taken: Vec<u8> = picks.iter().map(|p| p.0).collect();
        for k in picks.len().min(2)..6 {
            let (c, v) = mr_sim::ai::pick_opponent(exe, &taken, &|_| true, &|_| true, rand);
            taken.push(c);
            self.karts[k] = (c, v);
        }
        self.points = [0; 6];
        self.started = true;
    }

    /// The opponents as a session line-up (slots 1..5).
    pub fn lineup(&self) -> [(u8, u8); 5] {
        if self.tie_break() {
            // the tied humans drive slots 0.. (phase 13), the rest follow
            let h = self.tie_humans().len().max(1);
            let opp: Vec<(u8, u8)> = self.tie[h..].iter().map(|&k| self.karts[k as usize]).collect();
            if opp.is_empty() {
                return [self.karts[1]; 5];
            }
            return std::array::from_fn(|i| opp[i % opp.len()]);
        }
        [self.karts[1], self.karts[2], self.karts[3], self.karts[4], self.karts[5]]
    }

    /// The opponents in the session (the tie-break: the tied ones).
    pub fn opponents(&self) -> u8 {
        if self.tie_break() {
            (self.tie.len() - self.tie_humans().len().max(1)) as u8
        } else {
            (self.karts_n() - self.players.max(1) as usize) as u8
        }
    }


    /// Entering game mode 0x11 after a league race (FUN_8008C4AC): every
    /// kart's points from the session's finishing order, fastest lap
    /// (0x800B0DD0), best start (0x800B0E8C) and hits (+0x492 / +0x494).
    pub fn score(&mut self, s: &mr_sim::session::Session) {
        if self.league as usize == mr_sim::save::LEAGUE_BATTLE {
            // the BATTLE LEAGUE (phase 6b; 0x800C1E4C = 1): finishing
            // order by damage, least first (0x800D8F94), its points from
            // best crash, knock-outs and damage dealt
            let ks: Vec<&mr_sim::kart::KartState> = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).take(6).collect();
            let mut order: Vec<usize> = (0..ks.len()).collect();
            order.sort_by_key(|&i| ks[i].race_pos);
            let mut karts = vec![mr_sim::save::BattleKart { rank: 5, ..Default::default() }; 6];
            for (rank, &i) in order.iter().enumerate() {
                let k = ks[i];
                karts[i] = mr_sim::save::BattleKart { rank, dealt: k.hits_dealt, taken: k.stun, best_crash: k.max_hit, knockouts: k.knockouts };
                self.finish[rank] = i as u8;
            }
            for (k, p) in mr_sim::save::battle_points(&karts).iter().enumerate() {
                self.last[k] = *p;
                self.points[k] = self.points[k].saturating_add(p.total);
            }
            return;
        }
        let rows = s.results();
        let n = rows.len().min(self.karts_n());
        let mut karts = vec![LeagueKart { rank: 5, best_lap: 0, hits: 0, hits_taken: 0 }; n.max(1)];
        for (rank, r) in rows.iter().enumerate() {
            if r.slot < n {
                karts[r.slot] = LeagueKart { rank, best_lap: r.best_lap, hits: r.hits, hits_taken: r.hits_taken };
            }
        }
        // the finishing order of the league's karts (the tie-break race:
        // its field, then the others in kart order)
        let mut order: Vec<u8> = rows.iter().filter(|r| r.slot < n).map(|r| self.kart_of_slot(r.slot) as u8).collect();
        for k in 0..6u8 {
            if !order.contains(&k) {
                order.push(k);
            }
        }
        self.finish.copy_from_slice(&order[..6]);
        let pts = mr_sim::save::race_points(&karts, s.globals.lap_threshold, s.globals.start_best_mask);
        self.last = [RacePoints::default(); 6];
        for (slot, p) in pts.iter().enumerate() {
            let k = self.kart_of_slot(slot);
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
        s += &format!("league {}\nrace {}\nstarted {}\nplayers {}\n", self.league, self.race, self.started as u8, self.players);
        s += &format!("courses {}\n", join(&mut self.courses.iter().map(|c| c.to_string())));
        s += &format!("karts {}\n", join(&mut self.karts.iter().map(|(c, v)| format!("{c}:{v}"))));
        s += &format!("points {}\n", join(&mut self.points.iter().map(|c| c.to_string())));
        s += &format!("finish {}\n", join(&mut self.finish.iter().map(|c| c.to_string())));
        s += &format!("last {}\n", join(&mut self.last.iter().map(|p| format!("{},{},{},{},{}", p.place, p.fastest_lap, p.best_start, p.most_hits, p.total))));
        if !self.tie.is_empty() {
            s += &format!("tie {}\n", join(&mut self.tie.iter().map(|c| c.to_string())));
        }
        s
    }

    pub fn from_text(t: &str) -> Option<League> {
        let mut l = League { league: 0, race: 0, courses: [0; 6], karts: [(0, 0); 6], points: [0; 6], last: [RacePoints::default(); 6], finish: [0, 1, 2, 3, 4, 5], started: false, tie: Vec::new(), players: 1 };
        let nums = |v: &str| v.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse::<u8>().ok()).collect::<Vec<u8>>();
        for line in t.lines() {
            let (k, v) = line.split_once(' ')?;
            let n = nums(v);
            match k {
                "league" => l.league = *n.first()?,
                "race" => l.race = *n.first()?,
                "started" => l.started = *n.first()? != 0,
                "players" => l.players = (*n.first()?).clamp(1, 2),
                "courses" => l.courses = n.get(..6)?.try_into().ok()?,
                "points" => l.points = n.get(..6)?.try_into().ok()?,
                "finish" => l.finish = n.get(..6)?.try_into().ok()?,
                "tie" => l.tie = n.into_iter().filter(|&k| k < 6).collect(),
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
        (l.league < 4 && l.race <= PRESENTATION).then_some(l)
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
        if !crate::settings::args().into_iter().any(|a| a == "--league") {
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

    /// Row 12: a tie for first with the player -> the tie-break field.
    #[test]
    fn tie_break_field() {
        let mut l = League { league: 0, race: 6, courses: [1, 9, 8, 7, 3, 4], karts: [(0, 0), (5, 30), (13, 12), (17, 16), (23, 18), (1, 1)], points: [20, 20, 9, 20, 4, 2], last: [RacePoints::default(); 6], finish: [0, 1, 2, 3, 4, 5], started: true, tie: Vec::new(), players: 1 };
        assert_eq!(l.tie_needed(), Some(vec![0, 1, 3]));
        l.tie = vec![0, 1, 3];
        assert!(l.tie_break());
        assert_eq!(l.opponents(), 2);
        assert_eq!(l.lineup()[0], (5, 30));
        assert_eq!(l.lineup()[1], (17, 16));
        assert_eq!(l.kart_of_slot(2), 3);
        // no tie with the player: no tie-break
        l.points = [19, 20, 9, 20, 4, 2];
        assert_eq!(l.tie_needed(), None);
    }

    /// Phase 13: a two-player league: three race (two humans, one
    /// opponent); a tie with player 2 alone is played off by player 2.
    #[test]
    fn two_player_league() {
        let mut l = League { league: 0, race: 6, courses: [1, 9, 8, 7, 3, 4], karts: [(0, 0), (5, 30), (13, 12), (17, 16), (23, 18), (1, 1)], points: [9, 20, 20, 25, 0, 0], last: [RacePoints::default(); 6], finish: [0, 1, 2, 3, 4, 5], started: true, tie: Vec::new(), players: 2 };
        assert_eq!((l.karts_n(), l.opponents()), (3, 1));
        // karts 3.. do not race: kart 3's 25 points do not count
        assert_eq!(l.tie_needed(), Some(vec![1, 2]));
        l.tie = vec![1, 2];
        assert_eq!((l.tie_humans(), l.opponents(), l.lineup()[0]), (vec![1], 1, (13, 12)));
        // no human in the tie: no tie-break
        l.points = [1, 2, 9, 9, 0, 0];
        l.tie = Vec::new();
        assert_eq!(l.tie_needed(), None);
    }

    #[test]
    fn text_round_trip() {
        let mut l = League { league: 3, race: 2, courses: [8, 9, 16, 1, 3, 15], karts: [(0, 0), (5, 30), (13, 12), (17, 16), (23, 18), (1, 1)], points: [7, 3, 9, 0, 1, 2], last: [RacePoints::default(); 6], finish: [2, 0, 1, 5, 4, 3], started: true, tie: vec![0, 2], players: 2 };
        l.last[2] = RacePoints { place: 3, fastest_lap: 1, best_start: 0, most_hits: 1, total: 5 };
        assert_eq!(League::from_text(&l.to_text()), Some(l));
    }
}
