# Session 2 handoff (2026-10-12): Bevy port, milestone 1

Brief: docs/SESSION-2.md. Option A (port/ in this repo), owner away; the four
speed-ups from session 1 were all used (Ghidra over the recomp's functions,
the shared Rust comparator, the Bevy oracle ghost, one agent per asset format).

## Status against "Done when"
1. **Track and kart from the disc by Rust: DONE.** DOCK scenery (DAT), textures
   (RAW), terrain/walls (TER), sky (AXE), Kermit + RACER (MDL), all decoded in
   mr-assets and each proven against the running recomp (RAM images, VRAM,
   GTE input, GPU packets). The game's per-frame object selection and layer
   order are reproduced. No recomp at run time.
2. **Handling in Rust, matching within tolerance: DONE, exceeded.** The whole
   lap is tick-for-tick identical to the recomp (0 error on every field incl.
   the camera). Tolerance set in STEP 2: oracle run-to-run variation is zero, so
   the brief's ceiling applies (1% of DOCK's 116,862-unit median width = 1,169
   units, 2 deg). Standalone (disc only) the run is exact until the original
   drops to 20 Hz (tick 687) or an AI projectile hits (tick 821).
3. **Comparison report: DONE** (docs/notes/SESSION-2-report.md, plots in
   docs/notes/s2-report/). **Owner driving both: NOT DONE**: that is yours.

## What to do first
1. Play the port: `.\tools\play-port.ps1` (X accelerate, S brake, Z jump, arrows;
   or a gamepad: triggers). It's the first lap only: crossing the finish after
   the last lap stops the sim (race end is out of scope).
2. Play the recomp on DOCK for the feel comparison: Muppet Treasure Island >
   DOCK - RACE A, 1 PLAYER RACE, Kermit / RACER.
3. Optional, the strict check: `.\tools\play-port.ps1 -Record`, drive, close;
   `.\tools\run-dev.ps1 -Start`; `.\tools\compare-drive.ps1 <stamp>`.
   Expect exact agreement until the first 20 Hz frame of the original.

## Things you should know
- **Controls bug in the shipped package (fixed in source, not rebuilt):** the
  game accelerates with CROSS in every controller setup; TRIANGLE is camera
  lock-on. The Trigger Controls mod mapped RT to TRIANGLE. Source now maps
  RT -> CROSS and LT -> the configured brake. dist/ still has the old mapping:
  rebuild with `.\tools\build.ps1 -Disc <cue> -Package` (re-warm the overlay
  cache if its namespace changes, CLAUDE.md).
- **Catch-up:** in the original, a player behind the leader gets a higher top
  speed (0x800 -> 0xA00). The port has no AI, so by default it runs with the
  catch-up the oracle race had (player 6th), which is the feel you'd get in a
  real race; `-Leader` gives the no-catch-up rule (20% lower top speed).
- **Frame rate:** the original runs its logic at 20 Hz in heavy scenes (dt=3);
  the port always runs at 30 Hz. Same game speed, finer steps.
- build-dev/mods/state.toml (local) now enables the dev bridge and disables
  Trigger Controls (the bridge owns player 1's pad transform during oracle
  runs). If you play build-dev directly, re-enable Trigger Controls there.
- The oracle bridge is development-only: its package lives in mods/dev and is
  excluded from Release catalogs and builds.

## Ledgers
docs/spec/kart-physics-ledger.md (stubbed paths: items, weapons, AI,
multiplayer, other modes, race end; standalone sources), docs/spec/render-ledger.md.

## Proposed next subsystem order
camera is already done, so: 1. race flow (countdown/start minigame, laps, race
end, results) 2. AI pathing (TER racing line + AI buttons FUN_800912F8) and the
other karts 3. items/weapons 4. HUD 5. engine audio (SPU/VAB) 6. menus
7. other tracks (decoders already parse all 36 DAT/TER files) 8. FMV.

## Open items (small)
- Animated textures (RAW type 0x22): frame copy routine not found.
- Depth cueing and the near-polygon subdivision are not reproduced (render ledger).
- The body sway uses the session's own random stream (cosmetic).
