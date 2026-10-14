# SESSION 6: CLOSE-OUTS, THE OTHER 20 RACE COURSES, THEN THE TITLE SCREEN

Owner instructions (2026-10-14, then away ~2 h, work autonomously):
- 60 Hz feels great: default (done, commit "60 Hz logic is the default").
- Close out: 4:3 pillarbox bars (black), water brightness, the five kart
  status effects verified against the oracle. Startup errors: ignore unless
  they come back.
- Get all 20 other race courses working (COURSE_A/B/C on the disc besides
  DOCK: BASE, BOG, HAPPY, LONDON, PARK, CAPE, GALLERY, GHOST, GRAVE, ISLAND,
  NEW_YORK, DISCO, GLACIER, HOUSE, OVAL, RALLY, ROCK, SPEEDWAY, VOLCANO,
  WHATEVER — names from the TER files; the game's own course table decides
  which are race courses, laps, music, event numbers).
- Then the title screen.
- Wait for the owner only for a huge issue.

## Done when
1. Close-outs as above, with evidence.
2. Every race course loads and races in the port (1-player race, Kermit vs five
   AI, items, music, the Session 5 view): no `unreachable_in_scope!` panics in
   headless AI races at 30 and 60 Hz; each course's stubs ported from the code.
3. Each course checked against the oracle cheaply (owner's economy rule): one
   short capture per course (race load + about one lap of the AI field, player
   idle or AI-driven) and the port exact on race load + as far as the capture
   goes at 30 Hz (or each divergence explained in the ledger).
4. A course choice in mr-bevy (`--course NAME` + a plain pick-list until the
   real menus exist); music per course from the game's table.
5. Title screen (after 2): the game's title as the port's first screen, from
   the disc's FRONTEND data, checked against oracle screenshots; START goes to
   the course pick-list.
6. Logs, ledgers, handoff (docs/notes/SESSION-6-handoff.md).

## Rules
As Sessions 2-5 (oracle is the truth at 30 Hz; fixed point; generated C is the
spec, never copied; no game data committed; no new crates; savestate slots 1
and 2 never overwritten; commits local; parallel agents share the oracle
through capture.OracleLock).
