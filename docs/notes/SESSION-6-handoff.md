# Session 6 handoff (2026-10-14): close-outs, 18 race courses, title screen

Brief: docs/SESSION-6.md. Tracks: docs/notes/SESSION-6-coord.md. Logs:
docs/notes/2026-10-14.md ("Session 6, ..."). Specs: docs/spec/courses.md (new),
docs/spec/frontend.md (new), effects.md, render-ledger.md, kart-physics-ledger.md
(Session 6 sections), port/README.md.

## Play it
```
.\tools\play-port.ps1                 # title -> START -> course pick-list -> race
.\tools\play-port.ps1 -Course BOG     # straight to a course (title skipped)
.\tools\play-port.ps1 -Hz30           # the game's 30 Hz logic (60 Hz default)
```
START on the results returns to the pick-list. `--skip-title` starts in the race.

## Owner decisions recorded this session
- 60 Hz logic is the default (owner drove it). Oracle-grid runs stay 30 Hz.
- Haze lighting is the default (Session 5 decision).

## Status against "Done when"
1. Close-outs: DONE.
   - 4:3 bars black (and letterbox bars).
   - Water: fixed (15-bit clear, 5-bit stores; compare on the 5-bit scale):
     blue within 0.5-2 % of the oracle (was +6-7 %). Animated water texture
     was never misaligned (VRAM dump byte order).
   - Status effects: all five (plus types 4, 7) triggered on the oracle and
     matched side by side; kart colour factors tint in gamma space; kart scale
     applied; the shadow-list decals (0xC ground glow, cheese shadow, hole) and
     projectile shadows drawn (projectile shadow entries exact vs the oracle).
2. Courses: DONE. The game's tables say 18 race courses (DOCK + 17);
   VOLCANO / HOUSE / WHATEVER are arenas (battle / Chicken Chase), out of scope
   with ARENA/* and STUNT/*. All 18 load and race 3 laps at 30 and 60 Hz.
   Ported: object lists (jets, turntables, lifts, gates, breakables incl. floor
   / ceiling breaks), weather (rain, snow), overhead camera (mode 12),
   DISCO's damage race (race flag 8: damage, knock-out, gauge).
3. Oracle checks: one capture per course (tools/oracle/goto_course.py,
   capture_course.py; analysis/oracle/courses/). Race load exact on all 18;
   teacher-forced motion exact everywhere; FREE RUN EXACT TO THE END OF EVERY
   CAPTURE (~1,630 frames; DOCK 834), random state included, except two
   teacher-forced weather frames (GRAVE 273, BOG 1056). Moving scenery poses
   exact (54 slots); colours match with the oracle's camera on all 17.
   AI "stuck spots" on NEW_YORK / HAPPY: the game does the same (oracle poked
   onto the spots); no recovery exists in the code.
4. Course choice: DONE (`--course`, `-Course`, pick-list in the game's font,
   music per course from the table).
5. Title screen: DONE. Course 0x1E in mode 0x10 run by the race engine: orbit
   camera, logo, stars/fruit, PRESS START, fade, music (MUSIC4.XA ch 0), three
   battle-AI karts with knock-out re-creation, letter bounce. From the oracle's
   title state: karts, camera and 20 letters exact on 895/895 ticks.
6. Logs, ledgers, this handoff: DONE.

## Open
- Title from a fresh start uses one extra random byte in frame 1 (exact only 4
  ticks from the oracle's seed); not located.
- Not ported: attract demo (after 1,800 idle VBlanks), the game's menus,
  cheat-code text, arenas / battle, stunt courses, time trial, cups,
  split screen, FMV, memory card.
- Ceiling-break path from the code only (no oracle case).
- In the port's own races the line-up / grid differs from any oracle race (its
  own random seed; by design).
- Savestates: the runtime has 12 slots; slots 3-10 hold 8 course grids, the
  other 9 courses' grid/menu states are files in analysis/states/courses/
  (`goto_course.py install NAME grid` -> slot 11). Slots 1, 2 untouched.
- The oracle's save in RAM had secret courses unlocked by pokes during
  captures (track O reports it never chose SAVE GAME).

## Next (proposal)
The game's menus (main menu, character / vehicle / course select, options)
so the port boots like the original and the pick-list goes away; then
battle arenas (the battle AI is now exercised by the title), time trial and
cups.
