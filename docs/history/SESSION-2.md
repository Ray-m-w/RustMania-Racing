# SESSION 2: BEVY PORT, MILESTONE 1 - ONE KART, ONE TRACK, SAME FEEL

## Goal of the whole effort (agreed 2026-10-12)
A 100% Rust + Bevy reimplementation of Muppet RaceMania that feels the same
as the original, using the original assets from the owner's disc. The route is
hybrid first: the psxrecomp build stays the running game and the correctness
oracle while subsystems move to Rust one at a time, each verified against it
with identical inputs. The recomp is retired only when nothing depends on it.
Hybrid is a means, not the destination; every step must move toward removing
the recomp, never deepen dependence on it.

## Done when (this session)
In a Bevy window, the player drives one kart (Kermit, standard kart) around
the DOCK track, with:
1. Track and kart geometry and textures decoded from the owner's disc by Rust
   code (no recomp at runtime for this mode).
2. Kart handling implemented in Rust, and a recorded input route (accelerate,
   steer, brake, a full lap) driven through both the recomp and the Rust kart
   produces trajectories that match within an agreed tolerance (proposed:
   position error < 1% of track width and heading error < 2 degrees for the
   first 10 s; lap time within 2%). The tolerance is set in STEP 2 from the
   recomp's own run-to-run variation, not guessed.
3. A comparison report (plots + numbers) saved under docs/history/notes/, and the owner
   has driven both and agrees they feel the same.
Out of scope: AI, items, other tracks, menus, audio beyond engine sound.

## Where the code lives (owner decision at session start)
- Option A (recommended): new Cargo workspace `port/` in this repo. Copy (never
  link) verified facts and code from F:\Muppet Racemania as needed, recorded in
  BINLIST.toml and the notes.
- Option B: continue in F:\Muppet Racemania under its own rules, with this repo
  as the oracle. That repo is read-only to me unless the owner says otherwise.
Ask before writing any Rust.

## Rules (in addition to CLAUDE.md)
- The recomp is the oracle. Never tune Rust handling by feel alone: every
  constant is either read from the game's code/data or fitted to oracle traces,
  and the notes say which.
- The generated C (generated/, local) is the behavioral spec for the functions
  being ported. Port the behavior into clean Rust; never transliterate the
  generated C wholesale and never commit it.
- Assets are read from the owner's disc at runtime or by a local
  extraction step; extracted assets are gitignored like the disc.
- New crates (bevy and friends) are dependencies: owner approves them, and
  BINLIST.toml records version and source. Reuse F:\Muppet Racemania's ADR-0009
  Bevy feature set as the starting point.
- Fixed-point first: reproduce the game's integer/fixed math (16.16 positions,
  4.12 GTE matrices) in the simulation; convert to floats only for rendering.
- Simulation runs at the game's rate (30 Hz logic, 60 Hz VBlank, see STEP 2),
  decoupled from Bevy's render rate.

## STEP 1. INVENTORY AND DECIDE  ->  docs/history/notes/SESSION-2-plan.md
1. Read docs/history/notes/SESSION-1-handoff.md and this repo's CLAUDE.md.
2. From F:\Muppet Racemania: docs/spec/kart-state.md, symbols.md,
   kernel-usage.md, the SESSION-3..6 notes (container layout, kart struct,
   pad chain, goldens), the mr-* crates, ADR-0009/0011. List what is reusable
   as-is, what needs re-verification against the recomp, and what is missing.
3. Owner decides Option A or B. Record it.

## STEP 2. ORACLE HARNESS  ->  tools/oracle/, docs/history/notes/SESSION-2-oracle.md
Build the measurement loop before any gameplay code.
1. Deterministic start: a savestate (or a scripted route) that puts Kermit on
   the DOCK grid at the same frame every run. Check A/A determinism with
   frame_fingerprint (tools/fp_identity.py); record the run-to-run variation.
2. Deterministic input: the TCP input_route_* commands (frame-exact replay)
   driving recorded routes; store routes as data files in the repo.
3. State capture: a per-frame trace of the player kart (position, velocity,
   heading/orientation matrix, speed, surface, input) for each route. First
   find the per-track player record (the quarry's 0x801C4D84 was DOCK-only and
   moves per track): wtrace/fntrace on the kart update, cross-checked with
   kart-state.md.
4. Better than polling TCP: a trusted vblank plugin in src/mods that copies the
   kart record each frame into a shared-memory block (or file ring). This is
   also the hybrid bridge for STEP 5.
5. Output format: JSONL or Arrow per frame, one file per route; a Python or
   Rust comparator that prints error vs frame and draws plots.

## STEP 3. ASSETS  ->  port/crates/mr-assets (or B equivalent)
1. Decode DOCK's track (ARENA/*.TER, *.DAT, *.RAW, *.AXE, by name pattern;
   confirm which files the game loads for DOCK with the recomp's cd_read_log)
   and the Kermit kart (CHARS_A/KERMIT.MDL/.ANM).
2. Verify each decoder against the running game: compare the decoded
   texture pages with the recomp's VRAM (read_vram), and decoded geometry with
   GTE input/output (gte_ring_dump) for a known frame.
3. Render in Bevy with a camera placed from the recomp's camera state; side by
   side screenshot against present_shot from the same frame.

## STEP 4. KART HANDLING IN RUST  ->  port/crates/mr-sim
1. Locate the kart physics: start from the per-frame kart update found in
   STEP 2.3, the quarry's FUN_ addresses and the pad consumer chain
   (0x80043fa8 pump, 0x80079b28 consumer, button word 0x800B0DA8). Read the
   generated C for those functions as the spec.
2. Port in layers: input mapping -> acceleration/speed -> steering/heading ->
   ground contact/collision with the track -> camera follow. After each layer,
   run the routes through the comparator; do not move on until that layer is
   inside tolerance.
3. Keep a "differences" ledger: every place the Rust deliberately differs
   (and why).

## STEP 5. HYBRID OPTION (only if STEP 4 stalls)
If full physics takes longer than the session, demonstrate the hybrid: the
recomp runs headless, the shared-memory bridge streams kart and camera state,
and Bevy renders DOCK and the kart from it with the Rust asset pipeline. This
still delivers "Bevy shows the race", and STEP 4 continues afterwards.

## STEP 6. CLOSE  ->  docs/history/notes/YYYY-MM-DD.md, docs/history/notes/SESSION-2-handoff.md
Comparison report, the ledger, what is native Rust now, what the next
subsystem should be (proposed order: camera, AI pathing, items, HUD, menus,
other tracks, audio, FMV), commit, STOP.

## Stop and ask the owner when
- choosing Option A/B; adding any crate; a tolerance cannot be met after three
  attempts per layer; the recomp itself needs a change (framework or title).
