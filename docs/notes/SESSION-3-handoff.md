# Session 3 handoff (2026-10-12): a full race in the port

Brief: docs/SESSION-3.md (owner-approved). Details per area: race flow and AI
in docs/spec/kart-physics.md, docs/spec/ai.md, docs/notes/SESSION-3-ai.md;
HUD and results in docs/spec/hud.md; cameras in docs/spec/camera.md; scenery
and objects in docs/spec/formats/DAT.md; differences in
docs/spec/kart-physics-ledger.md (revisit list at the top) and
docs/spec/render-ledger.md.

## Play it
```
.\tools\play-port.ps1
```
A 3-lap 1-PLAYER RACE on DOCK, Kermit in the RACER, five AI opponents, from
race load: the intro camera, the "Copy the buttons" start game (copy the
button shown: Z = SQUARE, A = TRIANGLE, S = CIRCLE, X = CROSS; on a pad the
face buttons), GO, three laps, the finish with the game's trackside cameras,
and the results panel. START (Enter / pad Start) on the results restarts.
Driving: X / right trigger accelerate, S / left trigger brake-reverse, Z jump,
arrows / stick steer. `-Demo` lets the game's AI drive Kermit.

## Status against "Done when"
1. **Race flow: DONE.** Countdown, start game, 3 laps, timing, finish, results.
   The player's kart is tick-exact against the oracle over a recorded 3-lap
   race up to the finish (3,510 ticks; the AI's item hits on the player are
   injected, items being out of scope). Race load + the first 372 frames of
   all six karts are exact against the oracle.
2. **Minimal HUD: DONE**, plus the results panel. Place, lap, timer, the
   start-game prompts and the results are drawn from the game's own font
   sprites; checked against the game's GPU packets (15/15 race HUD sprites,
   59/59 results primitives at the frames checked).
3. **AI: DONE within tolerance** (the owner's standard for AI). The AI is the
   game's code (FUN_800912F8 and callees) driving the already-exact kart
   update; decisions match the game on 23,202 of 23,206 teacher-forced
   kart-ticks. 18 standalone races vs 6 items-off oracle races: every
   opponent's mean lap time within 1.1% (limit 3%), pooled +0.11%; racing line
   and finishing positions within limits. Opponents and their models load as
   in the game (default line-up from the oracle's seed; vary the seed for
   others).
4. **Frame rate: display DONE, 60 Hz logic NOT DONE (stretch).** On the
   owner's 120 Hz display the port renders 119.6 fps (p99 frame 10 ms) with
   30 Hz logic, kart, parts and camera interpolated. Running the logic at
   dt = 1 (60 Hz) leaves the player tolerance at tick 73: the game converts
   velocity to and from the kart frame and applies some kicks once per tick,
   so a smaller step changes the result. Making it dt-correct would change the
   30 Hz behaviour, which must stay exact; ledger section 10 lists the terms.
   My recommendation: keep 30 Hz logic + high-rate display, revisit only if
   input latency bothers you (it adds up to 33 ms).
5. **Report: this file + the specs. Owner drive: yours.**

Also added on the way: the start/finish banner (waving cloth, exact vertices),
kart shadows (record exact; the PS1's subtractive blend is approximated),
every kart drawn with its own character and vehicle, the intro and finish
cameras (exact on all captures), the controls fix in the recomp package (0.1.1).

## Known gaps you may notice
- Items and weapons are off for everyone (next subsystem). The original's
  AI uses turbo/specials/weapons, so the original's AI laps are ~8-13% faster
  than the port's items-off AI.
- No sound.
- Animated textures (water sparkle frames) are static.
- The original slows to 20 Hz in heavy scenes; the port does not.
- After the finish the AI keeps racing but never "finishes" (as in the game:
  mode 10 stops lap timing for everyone).

## Next (proposed, owner's order)
items (and the full HUD's item slots and minimap with them) -> audio -> menus
-> other tracks. Audio can run in parallel with items.

## Housekeeping
- Oracle harness: bridge v4 (RAM pokes, 8,192-step routes), run lock,
  first-load warm-up; build-dev/mods/state.toml keeps the bridge on and Trigger
  Controls off for oracle runs.
- All commits local on master, not pushed.
