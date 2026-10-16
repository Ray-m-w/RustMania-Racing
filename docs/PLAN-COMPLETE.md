# Plan to 100%: lean checklist (owner request 2026-10-16)

Working rules (owner): lean and conservative; at most 1-2 agents at a time
(normally one); one phase at a time, end to end, committed and logged before
the next; reuse oracle captures; one short new capture per feature at most;
no statistics batteries; stop cleanly when the night ends.

Each phase is "done" when: it works in the Bevy window, is checked against the
oracle at a few chosen moments (screens: GPU packets / screenshots; sim:
teacher-forced + free run on one capture), tests green, docs + log updated.

| # | phase | contents | status |
|---|---|---|---|
| 0 | Stability | TRIANGLE lock-on (A-key freeze), every reachable stub ported, fuzz, crash log instead of freeze | DONE 2026-10-16 (1,520 fuzz runs, 0 panics) |
| 1 | Front end | main menu, 1P race flow: character, vehicle, course select (replaces the pick-list), pause menu, options menu (= our settings, incl. faithful toggles) | DONE 2026-10-16 (title main menu, film-strip course select, course / game menus, world + course lists, character / vehicle select, pause + YES/NO, adjust sound, set controllers, port settings; GP0 font match on 7 screens; not-ported entries greyed, docs/spec/frontend.md "Menus") |
| 2 | Saves + progression | memory-card-style save file (progress, unlocks, lap records, options), cups / tournament flow and standings, unlocks (secret courses, world 7), rival races ("YOU BEAT x") | DONE 2026-10-16 (save.bin = the game's block 0x800E0190 + header, autosave, card import; progression FUN_8008BE68 matched on the oracle; lap records / BEST LAP, won bits, vehicles; TOURNAMENT race leagues A / B / random with league menu, points and standings, GP0 match on 5 screens. Left: GAME STATUS, ADVENTURE / "BEAT x" (phase 6), battle league (phase 4), tie-break + presentation, standings' 3D line-up; frontend.md "Not ported in phase 2") |
| 3 | Time trial | time trial mode, ghosts (FUN_8006F64C), records | DONE 2026-10-16: the game has no time trial or ghost; its timed solo mode is the 1 PLAYER STUNT TRIAL (race type 3 = game mode 3; FUN_8006F64C = the stunt strip locator), ported instead (docs/spec/stunt-trial.md): course menu, set-up, gates / clock / finish / course record + won bit, strip, pipes, paired triggers, mirror planes, cameras 3 / 9, HUD (progress bar, result), save. Oracle (MOUNTAIN, 3 captures): free run / teacher forced exact, record and won bit equal the save block, GP0 HUD 120/121. Left: 2 PLAYER STUNT RACE (phase 7), SECRETS, banner animation / mirror reflections (render), reversed trial unchecked |
| 4 | Battle | the 10 arenas (ARENA/* + VOLCANO, HOUSE, WHATEVER), battle rules, Chicken Chase | DONE 2026-10-16 (docs/spec/battle.md): 1 PLAYER BATTLE (game mode 2) on all ten arenas (the six worlds' ARENA/*, ARENA/BATTLE 0x1B, VOLCANO / HOUSE / WHATEVER): set-up, damage, knock-outs, the battle clock, last kart standing wins, record (BEST TIME) + won bit, 10-minute limit; course menu, HUD (gauge, clock, BATTLE THE MUPPETS), result (DAMAGE / BEST CRASH / KNOCKOUTS, place, time). Oracle (LAGOON, 3 captures): race load, free run and teacher forced exact (incl. 3 natural knock-outs), win + save block equal; GP0 result 116/116. CHICKEN CHASE is two-player only (split screen): rules documented, moved to phase 7. Left: BATTLE LEAGUE, 2 PLAYER BATTLE (phase 7), mode 7 boss battles / DOOZER BATTLE (phase 6), the secret screen 0x1D |
| 5 | Stunt | the 6 STUNT courses and their mode | mostly done in phase 3 (the stunt trial); left: the rendering items above, a check on a second stunt course |
| 6 | Challenge / boss | challenge races (race flag 0x10), boss races (flag 0x40) | in progress |
| 7 | Two players | split screen, 2P menus; 2 PLAYER BATTLE and CHICKEN CHASE (game mode 0x14, battle.md) | |
| 8 | Attract + extras | attract demo, credits, ending (THE_END), cheat codes, menu voice clips | |
| 9 | FMV | STR movies (MDEC + XA audio) in Rust | |
| 10 | Ship | standalone package (zip, disc picker, settings), oracle retirement check, final report | |

Accuracy loose ends (fold into the phase that touches them, not separate
work): fresh-title random byte; GRAVE/BOG weather frames; DISCO floor
animation timing; GRAVE far pickup; item sound key-on check; AI R2 specials
check; course-select sky backdrop + world props; fades / LOADING screen
between menus; menu voice clips (phase 8); in-process course change (no
relaunch); save.bin lives beside the exe in port/target (a cargo clean
would delete it): move it to a user-data folder in phase 10. Deferred by the owner: DLSS. Optional: AI personalities, control
remapping.
