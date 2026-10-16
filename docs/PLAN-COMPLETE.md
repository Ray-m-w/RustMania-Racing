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
| 2 | Saves + progression | memory-card-style save file (progress, unlocks, lap records, options), cups / tournament flow and standings, unlocks (secret courses, world 7), rival races ("YOU BEAT x") | |
| 3 | Time trial | time trial mode, ghosts (FUN_8006F64C), records | |
| 4 | Battle | the 10 arenas (ARENA/* + VOLCANO, HOUSE, WHATEVER), battle rules, Chicken Chase | |
| 5 | Stunt | the 6 STUNT courses and their mode | |
| 6 | Challenge / boss | challenge races (race flag 0x10), boss races (flag 0x40) | |
| 7 | Two players | split screen, 2P menus | |
| 8 | Attract + extras | attract demo, credits, ending (THE_END), cheat codes, menu voice clips | |
| 9 | FMV | STR movies (MDEC + XA audio) in Rust | |
| 10 | Ship | standalone package (zip, disc picker, settings), oracle retirement check, final report | |

Accuracy loose ends (fold into the phase that touches them, not separate
work): fresh-title random byte; GRAVE/BOG weather frames; DISCO floor
animation timing; GRAVE far pickup; item sound key-on check; AI R2 specials
check. Deferred by the owner: DLSS. Optional: AI personalities, control
remapping.
