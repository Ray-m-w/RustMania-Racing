# SESSION 3: A FULL RACE IN THE PORT (approved by the owner 2026-10-12)

## Goal of the whole effort (unchanged)
A 100% Rust + Bevy reimplementation of Muppet RaceMania that feels the same as
the original, using the original assets from the owner's disc. The psxrecomp
build stays the oracle until nothing depends on it.

## Owner decisions (2026-10-12)
- Subsystem order: race flow, then AI, then items, then HUD, then audio, then
  menus, then the rest (other tracks, FMV).
- Fidelity: the PLAYER kart stays tick-exact against the oracle. AI (and what
  depends on the shared random stream) only has to BEHAVE the same within a
  stated tolerance. Every place that is not one-for-one goes on a "revisit
  list" (top of docs/spec/kart-physics-ledger.md): the first things to make
  exact if we come back for full fidelity.
- Frame rate: 30 Hz logic is the minimum. Target: display at the monitor's rate
  (60/120 Hz) on top of the logic. Stretch: 60 Hz logic.

## Done when (this session)
In the Bevy window the owner can play a full 3-lap 1-PLAYER RACE on DOCK as
Kermit against the five AI karts, from the countdown to the results:
1. Race flow: countdown and the "Copy the buttons" start minigame, 3 laps,
   lap and race timing, finish, results (positions, times, best lap). The
   player's race is tick-exact against the oracle for a recorded route as far as
   the AI lets it be (no contact = exact).
2. Minimal HUD from the disc's own sprites: countdown, lap counter, race timer,
   position. (Full HUD with minimap and item slots comes with items.)
3. AI: the five opponents, their characters/vehicles from the race setup,
   driving the racing line with the game's AI code feeding the already-ported
   kart update. Tolerance (set in STEP 2 from 5+ oracle races, not guessed):
   proposed - each AI kart's lap time within 3% of the oracle's mean, its line
   within 10% of track width of the oracle's distribution, finishing order
   distribution comparable. Kart-kart contacts and catch-up run for real.
4. Frame rate: the display runs at the monitor's refresh (measured at 60 and
   120 Hz: frame pacing, interpolation of kart, parts, camera) with 30 Hz logic.
   Stretch: 60 Hz logic (dt = 1), accepted when a dt=1 run of the recorded
   routes stays within the player tolerance of the dt=2 run, with every
   non-dt-correct term found and fixed in the ledger.
5. Comparison report + the owner drives a full race in both and agrees.
Out of scope: items/weapons (the AI does not fire; the oracle races used for
the tolerance are taken with items disabled or their effects excluded), menus,
other tracks, audio beyond what is already there.

## Steps (sketch)
1. Race flow + minimal HUD: port the countdown/start/finish state machine
   (the stubbed paths in the ledger), lap-end and race-end, results data; HUD
   sprites from the RAW/FRONTEND textures. Verify the player's 3-lap route.
2. Oracle for AI: capture 5+ races (same savestate, different player routes
   or idle) with all 6 karts; per-kart traces; the tolerance from their spread.
3. AI port: FUN_800912F8 and its callees (racing line from TER, steering,
   throttle, catch-up), AI kart init from the race setup. Verify per kart:
   teacher forcing where exact is cheap, behaviour statistics otherwise.
4. Frame rate: measure display pacing at 60/120 Hz; then the dt=1 experiment.
5. Close: report, ledgers (+ revisit list), handoff, commit, STOP.

## Rules
As SESSION-2.md (oracle is the truth, constants from code/data or fitted to
oracle traces, fixed point in the sim, no game data committed, new crates need
approval and a BINLIST line). Parallel agents for independent pieces (HUD
sprites, AI, frame-rate measurement) as in Session 2.

## Stop and ask the owner when
A tolerance cannot be met after three attempts; a crate is needed; the recomp
needs a change; the AI tolerance numbers from STEP 2 look wrong to you.
