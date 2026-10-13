# SESSION 5: GRAPHICS PASS AND 60 HZ

## Goal of the whole effort (unchanged)
A 100% Rust + Bevy reimplementation of Muppet RaceMania that feels the same as
the original, using the original assets from the owner's disc. The psxrecomp
build stays the oracle until nothing depends on it.

## Where we are (read first)
docs/notes/SESSION-4-handoff.md, docs/spec/render-ledger.md, docs/spec/effects.md,
docs/spec/formats/DAT.md (visibility stream, cull radius, layer-1 stand-ins,
texture LOD), docs/notes/SESSION-3-handoff.md (60 Hz findings),
docs/spec/kart-physics-ledger.md section 10 (the per-tick terms that break dt=1).

## Owner decisions (2026-10-13)
- After racing Session 4: the pop-in is too aggressive. Draw the whole course
  at all times; pickups are visible through walls in places.
- Graphics pass BEFORE the other 20 courses, so they inherit it.
- Anti-aliasing: MSAA only for now (DLSS/DLAA deferred: needs the NVIDIA SDK
  licence + Vulkan SDK, owner's call later). MSAA 4x default.
- 60 Hz: cut input latency regardless (faithful physics untouched), AND build a
  true 60 Hz logic mode; a settings toggle chooses. 60 Hz becomes default only
  after it passes the feel tolerance and the owner has driven it.
- Every enhancement is an option with a faithful setting one switch away.

## Done when
1. Draw distance: whole course every frame (no visibility stream, no cull
   radius, no layer-1 stand-in swap where a full model exists; keep layer-1
   pieces that are real backdrop); option `faithful draw distance` keeps the
   game's rule (used by the GPU-packet checks). No visible pop-in on DOCK.
2. Sprite depth: pickups, particles, projectiles are hidden by walls correctly
   (no camera pull through geometry).
3. Widescreen: native aspect of the window (16:9 and wider), HUD placed
   correctly; option 4:3.
4. Effects gaps: additive / subtractive blending as the PS1 does it (explosions
   as bright as the game, smoke subtractive), white explosion core
   (FUN_800342B8), the five remaining FUN_80037B80 status effects, pickup
   shadows, exhaust flames (FUN_8007D660) if not already drawn. Checked against
   oracle screenshots side by side.
5. AA + texture filtering: MSAA 4x default (off / 2 / 4 / 8); textures
   nearest (PS1) or smoothed with mipmaps, default smoothed, without CLUT
   transparency fringes.
6. Lighting: one optional enhanced look, prototyped and shown to the owner
   (screenshots) before it is made default; faithful vertex colours stay.
7. 60 Hz: (a) latency: input sampled every display frame and each 30 Hz tick
   run as late as possible, measured input-to-photon improvement; faithful
   physics bit-identical (all oracle tests unchanged). (b) true 60 Hz mode:
   the sim made dt-correct for dt = 1 behind a mode flag; the 30 Hz mode stays
   tick-exact; the 60 Hz mode within the Session 3 AI-style feel tolerance
   (player lap times on recorded inputs, racing line, AI lap means, item
   rates) vs the 30 Hz mode.
8. Settings: a `Settings` resource with every option, from CLI flags and a
   plain-text settings file beside the exe (hand-parsed, no new crate), ready
   for the future settings menu.
9. Report + the owner drives it.

## Rules
As Sessions 2-4: oracle is the truth for the faithful paths; fixed point in the
sim (the 60 Hz mode included); generated C is the spec, never copied; no game
data committed; no new crates without approval + BINLIST; parallel agents with
the oracle shared through capture.py's lock; ledgers updated; commits local.
Owner's economy rule (Session 4): reuse existing captures; few new oracle runs.

## Stop and ask the owner when
A tolerance cannot be met after three attempts; a crate is needed; the recomp
needs a change; a look decision (lighting default, filtering default) needs
the owner's eyes.
