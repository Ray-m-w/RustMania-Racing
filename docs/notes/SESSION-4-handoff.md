# Session 4 handoff (2026-10-13): items and audio

Brief: docs/SESSION-4.md. Tracks and their contract: docs/notes/SESSION-4-coord.md.
Details: docs/spec/items.md, docs/spec/effects.md, docs/spec/hud.md ("Item HUD"),
docs/spec/audio.md, kart-physics-ledger.md, render-ledger.md; logs in
docs/notes/2026-10-12.md ("Session 4, ..." sections).

## Play it
```
.\tools\play-port.ps1
```
3-lap DOCK race, Kermit vs five AI, items and sound on. Keyboard: arrows steer,
X accelerate, S brake, Z jump, Q = L1 / E = L2 (weapons), W = R1 (turbo; special
move on a full meter), R = R2 (character special), Enter = START. Pad: face
buttons + triggers, L1/R1 bumpers, L2/R2 stick clicks. Audio options
(`--music-volume`, `--sfx-volume`, `--no-music`, `--no-audio`) in port/README.md.

## Status against "Done when"
1. **Items: DONE.** Pickups (tokens, boxes, trigger objects, warp), turbo meter
   and R1, weapons, projectiles, hits and status effects, the full-meter special
   move, all 25 R2 character specials, FUN_8007F03C kart particles.
   - Player exact to the finish on 4 items-on routes, nothing injected (the
     port's own projectiles hit the player 4/4/2/3 times).
   - All six karts free-running with nothing taken from the oracle after tick 1
     (`mr-items freeall --own-rng`): exact on every tick over 4,815 ticks on 4
     races, pickups/projectiles/random streams/particle rings included.
   - AI item use, 20 oracle vs 20 port races (one per start), tolerance
     2*sqrt(sd_o^2/n_o + sd_p^2/n_p): every statistic within tolerance (table in
     items.md). Port races start at tick 1: from the restore frame (tick 0) even
     the capture's own seed does not reproduce the oracle (harness defect,
     `race --from0` kept). Still leaning low: tokens -15 (tol 21), hits -2
     (tol 2.07). Character specials untested (0 in both; the AI only uses R2
     when behind kart 0 and the oracle runs had an idle player).
2. **Item HUD: DONE.** 9,714 / 9,714 primitives vs the game's GP0 packets over
   345 frames (slots, meter, minimap, arrow, WRONG WAY, LAP 2 / FINAL LAP,
   -1 HIT, SPECIAL MOVE, place, lap, timer). Not ported: three 256x5 sprites at
   (256,193), origin unknown.
3. **Effects: DONE with approximations.** Particle list 0x800E0C20 and sparks
   0x800B4540 exact over 4,323 frames; drawn: flames, smoke, explosions,
   sparks, item boxes, tokens, 4 projectile kinds, special glow, 6 status
   effects, kart particles. Ledgered: additive/subtractive blend approximated
   (explosions dimmer), sprite depth bias, star animation 1-2 frames off; not
   drawn: white explosion core, 5 status effects, pickup shadows.
4. **Audio: DONE.** bevy_audio 0.19.1 (owner-approved, BINLIST).
   - Music: DOCK = TUNES/MUSIC3.XA channel 0 (course table 0x800ACB34), intro
     clip, seamless loop, finish clip then restart: sample-exact vs the oracle's
     CD input over 160 s.
   - SFX/engine: SFX_6 bank exact vs SPU RAM; engine pitch exact 3,582/3,585
     checkable ticks, volume all; scrape/slide loops exact; non-item key-ons
     ~99% matched. Item key-ons placed but NOT verified (sfx_verify replays only
     kart_update, not the item frame).
5. **Report: this file. Owner drive: yours.**
Carry-overs: animated textures DONE (FUN_8007A9DC); "YOU BEAT x" ported
(code + unit test; rival races only).
AI personalities: design note docs/notes/ai-personalities.md, no prototype.

## Revisit list (new this session)
- sfx_verify: run the session's item frame so item key-ons are checked.
- AI tokens / hits lean low (inside tolerance); AI R2 specials untested.
- Intro voice clip at CdMix 0x6C (music level): modelled as observed, call site
  not found.
- SPU interpolation kernel fitted (hardware table not on the disc; psxrecomp's
  is noncommercial-licensed); no reverb.
- Special move steer lag / sway / look point depend on stale stack slots
  (`RaceGlobals::stack`).
- No pause menu (music pause API exists).

## Not started (whole game)
Front end (title, menus, selection screens), other tracks, other modes (battle,
time trial, challenge, cups, boss, split screen), FMV, memory card, 60 Hz logic,
the 20 Hz slowdown.

## Housekeeping
- BINLIST.toml now parses as TOML (old unescaped backslashes fixed).
- Per-agent cargo dirs port/target-b, -c (and -d) can be deleted (~9 GB each).
- Oracle stopped at session end. All commits local on master, not pushed.
