# Kart physics: differences ledger (port/crates/mr-sim)

Every place the Rust kart deliberately differs from the game, every stubbed
branch, and every input the port takes from outside. Companion to
kart-physics.md.

## Revisit list (not one-for-one; first to make exact for full fidelity)
AI (Session 3, mr-sim ai.rs; docs/spec/ai.md, docs/notes/SESSION-3-ai.md).
The owner's rule: AI behaves the same within a tolerance; these are the
places it is not one-for-one:
- Items: the AI's L1/L2 (weapons), R1 (turbo from the meter) and R2 /
  full-meter R1 (specials) are produced by ai.rs and dropped
  (`ai::items_off`); item boxes, turbo tokens (FUN_8007BBF0), weapons
  (FUN_80068C6C, projectiles FUN_80069648) and specials (+0x4D6,
  FUN_800913F4, FUN_8006B828) are not ported. The oracle runs the tolerance
  is measured on had them switched off by TCP pokes (capture_noitems.py; a
  boost tick can slip through, none did) or bridge v4 pokes once built.
- rand(): the BIOS rand() stream (seed 0x80009010) is shared with other
  game code (camera mode changes, contact effects, race load); a session
  has its own seed, so AI path branches and jumps differ from any recorded
  race. With the oracle's seed the decisions are exact except in frames
  where other code drew first (4 of 23,206 kart-ticks).
- Contacts: the session runs the full pair loop (`collide::frame_contacts_all`,
  one reset, pairs (j, i) j < i in order, one nearest-kart table). The
  mr-ai harness (src/bin/mr-ai) still calls `frame_contacts` once per kart
  and so forgets earlier pairs' nearest / close fields (its own analysis
  only).
- update.rs now calls `ai::catchup_scale` (0x80065850) and `ai::reroll`
  (0x80068078) in place; the mr-ai harness's `ai_kart_tick` (flag bit 0
  cleared, both after the update) is kept for its teacher forcing only.
- FUN_80090FAC (battle AI), FUN_80090670 (backward path walk) and the
  time-trial ghost branch are ported from the code but never ran against
  the oracle (no battle / reverse / ghost capture).
- Opponent choice (`ai::pick_opponent`) uses the exe's racing flag; the
  RAM tables 0x800C3028 (character model loaded) and 0x800D1560 (vehicle
  model loaded) are taken as "all loaded" (true on DOCK: all 25 racers and
  their default vehicles).
- Vehicle 22 (boss LOG) template: wheel positions only once its model is
  loaded (the oracle's RAM kept an earlier load's).

Race flow (Session 3, mr-sim race.rs / session.rs):
- After the finish the AI drives the player's kart (`ai_buttons`, items
  off: no R1 specials / weapons as the oracle's race3P had).
- Camera during the intro fly-by (countdown >= 300, camera mode 10) and after
  the finish (finished-kart / results cameras): not in camera.rs; the session
  holds the camera. Consequence: during the intro the game's FUN_80041CC4
  makes every kart look at the flying camera (aim +0x2C0, distance 0); in
  the session the camera is inactive then (mode -1), so the aim and the
  body sway that follows it differ (cosmetic) and the random pointer
  0x800B0BCC drifts by a few draws per frame from the oracle's (the sway
  draws), which reaches AI jumps / blink timing.
- Water animation state (0x800AFEF0) at race load: it runs on between races
  in the game; the session starts it from the exe's initial values. With
  the oracle's state the load frame is exact (random pointer included).
- Race load: the BIOS rand() seed and the random pointer at load decide
  the opponents and the grid. `SessionConfig::dock_default` uses the
  oracle's (start_probe: seed 0x832B2260, pointer 0x8001FC4E); the
  pointer reset to 0x8001F94C (`RNG_START`, FUN_8008ACE4) did not happen
  at that load, so it is only the solo session's start.
- The frame that switches to the results (mode 10) runs FUN_800606D0 (not
  ported; sound / results set-up) and is a long frame in the game (dt 4
  after it); the session switches mode and skips that frame's simulation.
- AI start boost at GO (0x8005C538): rand() / (32767 / (best start x
  difficulty / 16 + 1) + 1) as an i16; with no best start yet the divisor
  wraps to -32768 and the AI gets 0. Ported, but not yet seen non-zero
  against the oracle.
- The finish voice (FUN_80060580), the "race won" save bit 0x800E0196 and the
  lap-record table 0x800E0190 bytes beyond the time: not modelled.
- After the player finishes (mode 10) the game stops lap timing for every
  kart, so AI karts still racing never finish (oracle race3P: 0x800B0CD8
  stays); the session does the same. The results screen's ranking of those
  karts (by progress?) is not ported.

## 1. Stubbed branches (`unreachable_in_scope!`, panic when reached)
Paths the solo player does not take on DOCK. Address = the game code skipped.

| where | branch | why out of scope |
|---|---|---|
| update.rs | nearest-camera search with 2+ humans (0x80064FB0) | multiplayer |
| update.rs | TRIANGLE look-at when no kart is near / mode 0x14 (FUN_800728C0) | camera lock search, challenge mode |
| update.rs | special state +0x4D6 != 0 (track reset / special move, FUN_800913F4) | special move / reset |
| update.rs | R1 with a full meter (+0x48A = 0x1A4) on a path: special move (0x80065B5C) | special move |
| update.rs | L1 / L2 with a charged weapon slot: fire (FUN_80068C6C) | weapons |
| update.rs | R2 with a full meter: character special (FUN_8006B828) | special move |
| update.rs | jump while carrying an object (+0x4E8 != -1) | items |
| update.rs | status bits 0x40110 / 0x20 / 0x400 / 0x2000 / 0x8000 / 0x80 / 1 (homing, forced float, frozen, pushes, wobbles); 0x40 (spun by a hit) is ported, its timer +0x468 is FUN_8006C388's | item / weapon effects |
| update.rs | effects +0x4F1 = 0x15 / 0x16 | item effects |
| update.rs | time-trial ghost FUN_8006F64C (race type 3) | not a race |
| update.rs | 10-minute limit: kart handed to the AI (+0x4C7 = 2) | race clock > 179999 |
| ground.rs | ground contact in a special state; wall hit while carrying | special move / items |
| progress.rs | object lists FUN_800628FC / FUN_80061BC4 / FUN_800632BC / FUN_80063C24 / FUN_80062E38 / FUN_800627E0 with non-empty lists | DOCK has none (pointers 0x800D1C68/8C/98/A4 null, 0x800B0C58/0E00/0C68 zero) |
| progress.rs | DOCK's two trigger objects (0x800D1C9C, count 0x800B0DC6 = 2) touched, pickup states (+0x4E6, +0x4E7) | items |
| progress.rs | lap timing in modes 2/7/20, race flags 8/2, time trial, 10-minute limit | other modes |
| progress.rs | challenge win bits at the finish (race flag 0x10) | challenge mode |
| race.rs | race progress in battle / time trial / race flag 1 | other modes |
| session.rs | one of two human players finished | split screen |
| progress.rs | FUN_80090670 path mode (+0x4DF != 0), track direction in modes 2/7/0x14 | other modes |
| collide.rs | battle-mode damage on contact (modes 2/7, flag 8), challenge tag (0x800B07E6, 0x800B07FE) | other modes |

## 2. Code not ported (no effect on the kart record the port owns)
- Sound calls (FUN_8007A620, FUN_8005EECC): skipped; their random-byte draws are
  kept where the kart update itself draws them.
- Shadow polygon FUN_8003E59C + FUN_8003E3A0 (writes 0x800C8B38..), particles
  FUN_800326B4 / FUN_800870E8 (random draws kept): render side.
- The body matrix +0x18C (FUN_80025D8C / FUN_80024594) and wheel matrices
  (FUN_80086DF0): render prep after the kart loop. The oracle trace's m0..m8 come
  from there; not in mr-sim yet.
- 0x800B0A20 / 0x800B0B20 / 0x800B0F3C / 0x800B0CBE (path walk outputs) and the
  ground/wall query globals (0x800B0B0C, 0x800B0C40, ...) are locals in Rust.

## 3. Inputs taken from outside the port (the harness reads them from the oracle)
`mr_sim::harness` documents these; a Bevy game must produce them itself:
- buttons and dt per tick (dt is 3 for 15 ticks of lap1, VBlanks ~1374-1398);
- race globals (mode, race clock, countdown, flags, humans, laps, lap record);
- the race leader's progress (+0x388 of kart 0x800D8F94) for the catch-up;
- the water animation's normals 0x800B0CA0..CA4 and 0x800B0EA8..EAC
  (FUN_8003C5FC; it draws random bytes, so it must be ported with the shared
  random pointer to run without the oracle);
- kart fields other code writes: look point +0x324..+0x32C and +0x338 (render),
  race position +0x388/+0x4BE (FUN_800837F8), turbo meter +0x48A, wrong-way
  monitor +0x48C (after the kart loop: previous capture); path position
  +0x498/+0x49A (FUN_8009004C) and look-at target +0x2C0..+0x2CC
  (FUN_80041A20/80041CC4) (before the kart loop: same frame);
- the other karts' records (for the contact pass), from the previous frame.

## 4. External events injected in the lap free run
- Tick 821 (VBlank 1650): an AI projectile hits the player (FUN_80069648 after
  the kart loop: velocity halved and pushed, spin-out, flash). Weapons are out
  of scope and the capture does not include the projectile table (0x800DDC50),
  so the free run copies the fields the hit changed from the oracle at that tick
  (vx, vy, vz, spin, flash, rumble, ...). Without it the free run is exact for
  ticks 1..821 and then diverges.

## 5. Deliberate API differences from the brief
- `tick(k, g, w, inputs, buttons)`: race globals (`RaceGlobals`, includes dt) and
  per-tick outside inputs (`TickInputs`) are explicit arguments instead of
  `dt: i32`; `tick_with_karts` adds the kart-kart contact pass.
- The kart record's face pointers (+0x2E0/+0x2E4) are RAM addresses; the port
  writes them from the face index and the heap address of the faces
  (`World::heap`, taken from the oracle RAM) so records compare byte for byte.
- `KartState::trail` owns the position-history ring (+0x2DC -> 0x800D4098)
  for the camera (camera.rs, `push_trail`).

## 6. Known imprecisions (no effect on motion)
- The random pointer 0x800B0BCC at the moment kart 0 updates depends on code
  earlier in the frame (water animation, AI karts) that the port does not
  run, so random-driven cosmetic bits (event bits +0x4DD, blink timer +0x44C,
  skid +0x4ED, contact tie-breaks) can differ from the game; in the lap this
  touched no motion field. (Function-level fixtures, which start from the same
  pointer, match it exactly.)
- Body-tilt outputs +0x458/+0x45C and pad rumble +0x4B7/+0x4EA/+0x4EB differ
  from the oracle by small amounts (the game's own code run on the captured
  state shows the same difference; the extra input was not found).
- Course data: the finish line (DOCK.DAT +0xA14), course length (+0xA48) and
  trigger list (+0x39B4) are read from the disc at the offsets the game's
  pointers give (pointer - DAT load address, taken from the oracle RAM and
  checked byte for byte). They should come from mr_assets::dat once that
  decoder exposes these records.
- Vehicle stats come from the exe table at 0x800ACDB0 (FUN_80087EB4 copies them
  to 0x800D9130 + 0x174 * vehicle); verified for vehicle 0 (Kermit's RACER).

## 7. Standalone session (session.rs)
What the oracle supplied and how the session produces it now:
| input | standalone source |
|---|---|
| initial kart record | FUN_800609F4 port (init.rs) + vehicle template (vehicle.rs) from the exe and RACER.MDL / KERMIT.MDL |
| race globals | SessionConfig (race settings of the oracle savestate: laps 3, event 3, cup 4, lap record 12186, end counter 1) and the ported clock / countdown FUN_8005C2EC (race.rs) |
| buttons, dt | caller (`tick`: dt 2; `tick_dt` for analysis) |
| path position, wobble | FUN_8009004C port, every frame (one kart) |
| water normals | FUN_8003C5FC port, state from the exe's data section |
| random pointer | reset 0x8001F94C at race load, FUN_8007D304's per-frame step and wrap |
| body matrix | FUN_80024594 port |
| leader progress | the leading kart's (race position 1) progress; alone, the player's own |
| race progress, positions | FUN_800837F8 / FUN_800834C0 port (race.rs) over the session's karts |
| other karts | `Session::others` (empty until the AI port fills it) |

Render-side fields (render.rs, ported since): the look-at target +0x2C0..
(FUN_80041A20 with FUN_80081170's mode-1 nearest-point search over DAT chunk
7; FUN_80041CC4 checked), the wheel / driver part matrices (FUN_80086DF0), the
vehicle and driver matrices, driver position, look point, bounce offset,
wheel and light positions (FUN_80025D8C). Not ported: +0x1EC (driver matrix
times the character model's own part matrix; nothing reads it for placement),
the model / character slot numbers +0x4A4 / +0x4A5 (0 in the session; the
render code only tests them against -1).
Not produced (cosmetic or other subsystems), with the consequence:
- Other karts as look-at targets: in a race with AI the contact pass aims
  the body sway at the nearest kart's look point; alone, only the course
  look points remain, so the sway differs from the oracle's once an AI kart
  comes close (lap1: tick 69).
- Race position (ported since, race.rs): with one kart the player is first,
  so the catch-up is 0 as for a leader (analysis: `race_pos_override`).
- Turbo-meter tokens FUN_8007BBF0 (+0x48A += 12 per token collected on the
  track): not ported, the meter stays 0, so R1 turbo never fires and the boost
  glow target stays 0x60.
- FUN_80081170 / FUN_8006F410 record fields +0x488 / +0x490 at init (course
  point index, zone), the model/character slot numbers +0x4A4 / +0x4A5, flag
  bit 1 of +0x496 (set elsewhere). (The countdown and the start minigame are
  ported since: the session starts at race load, countdown 480.)
- BIOS rand() seed: unknown at boot; `SessionConfig::rand_seed` drives the
  initial wheel angles (cosmetic) and the camera's random bits on a mode change.
- The water animation's state starts from the exe's initial values; in the
  game it has run since the race (or an earlier one) loaded, and the random
  pointer is shared with the AI, so wave phases differ from any oracle run.

**Catch-up consequence (the main behavioural difference):** the oracle runs
had the player 6th of 6 with the leader far ahead, so FUN_80064B5C's catch-up
(+0x4C3) sat at 180 and raised the forward speed cap from 0x40*32 = 2048 to
0x50*32 = 2560 (cap = 0x40 + 16*catchup/180, times 32). With no AI the player
leads, catch-up is 0 and the cap is 2048: top speed is 20% lower than in the
recorded lap. This is what the game does in a race the player leads.
Measured on the lap1 route (buttons replayed per tick): the standalone run
first differs at tick 43 (the speed reaches the lower cap); with the leader
forced far ahead (`mr-standalone --catchup-as-6th`) it is bit-exact through
tick 686 (dt 3 begins at 687 in the oracle), and with the oracle's dt as well
(`--oracle-dt`) bit-exact on every motion field, the body matrix and the
camera through tick 821, the AI projectile hit.

## 8. Render-side fields: verification and the random pointer
The body-sway springs (+0x3A0..), their outputs (+0x454..+0x45C) and the
driver head matrix (+0x29C) take random kicks from the shared random pointer
0x800B0BCC. The capture holds the pointer at VBlanks, not at the moment kart 0
updates (the water animation and the contact pass draw from it first), so the
harness tries pointer skips 0..12 per tick and keeps the first that reproduces
the random-driven fields (`harness::Run::teacher_forcing`, skips found on every
lap tick: 0 x 874, 1 x 517, 2 x 136, 3 x 8, 4 x 2). With that pointer:
- teacher forcing: every render range (body, vehicle, driver and wheel
  matrices, driver position, look point, bounce offset, wheel and light
  positions, aim, sway springs, weapon slots) is exact on every tick except
  172 and 177 (weapon pickups give weapon slots: code outside the port, items)
  and 821/822 (the AI hit);
- free run with those three events injected and the pointer per frame: all
  render ranges and motion fields exact on all 1537 ticks;
- standalone session with the oracle's catch-up, dt, pointer and events:
  exact until tick 69, where the oracle's look-at target becomes an AI kart.
The pointer is an input like the buttons: in a real race the AI karts and
other code draw from it too, so a standalone session's body sway (cosmetic)
cannot match a recorded race's.

## 9. Race flow (Session 3: race.rs, session.rs, progress.rs)
Ported, game order per frame (`Session::tick`):
1. FUN_8005C2EC (before the race frame): frame counter, countdown 0x800B0C70
   (480 at race load = 300 + 180, FUN_8008C4AC) or race clock 0x800B0A94
   (+5 per VBlank once the countdown is 0), one rand(); GO when the countdown
   drops below 1 (0x8005C4A8: humans' start boost -> best 0x800E02BA, every
   kart +0x4FC = 0xB, +0x444 = +0x446, +0x446 <<= 3). It steps by the
   previous frame's dt (0x800B0AEC is measured after it; oracle: countdown -2
   in a frame whose kart update ran with dt 3). The first frame after the race
   load runs with the countdown held at 480 (oracle start_probe).
2. FUN_8007916C's start: all humans finished -> mode 10, that frame runs no
   race simulation.
3. The race frame as before; in the kart update the "Copy the buttons" start
   game (0 < countdown < 300, mode 1): button table exe 0x800B077C (SQUARE,
   TRIANGLE, CIRCLE, CROSS) by +0x4DA; a match adds 1 or 2 (+0x4B8 > 0x1D) to
   the start boost +0x446 and draws the next button from the random pointer
   (not the same one); a wrong button takes 1 off.
4. Lap end FUN_80082708 / 0x80082DA0: lap time rounded to a multiple of 3,
   lap record, best lap +0x384, announcement threshold 0x800B0DD0, lap++,
   banners (+0x4FC = 8 on the last lap, lap - 1 before). Finish (0x80083004)
   when lap == laps or another kart has finished (0x800B0CD8 > 1): +0x4C7 = 1,
   +0x4F9 = 0x80, place = 0x800B0CD8, flags |= 1 (AI drives), an early finish
   counts karts still racing on a later lap and re-sums the race time,
   +0x4FF = 0x10, +0x4CA = place, progress = length * lap + 1 + karts - place
   (keeps the finishing order sorted), 0x800B0CD8++. After it the kart's
   race time stops; +0x38C counts VBlanks since the finish.
5. The spin after a hit (status 0x40, 0x800668A8): heading += ramp * +0x300
   / 32 * dt, ramp rising / falling over 32 VBlanks of the 300-VBlank effect
   timer +0x468 (the timer itself is FUN_8006C388, after the kart loop: an
   input in the harness).
6. FUN_800837F8 (after the kart loop): the round-robin kart's progress along
   DAT chunk 5 (FUN_80081170 mode 0x10; not for lap 0 with +0x4C1 = 1, capped
   at length * laps; none in modes 10 / 0x11), FUN_800834C0 bubble sort
   (equal neighbours swap), race_pos = rank + 1, position flash +0x4FF = 0x1E
   and +0x4CA on a change, -dt per frame.

Verified (analysis/oracle/race3P, route tools/oracle/routes/race3.json, 3
laps, player 3rd; analysis/oracle/start_probe, slot 1 through the race load):
- teacher forcing to the finish (3510 ticks): exact except the external
  events 806, 829, 1884, 3179 (AI projectile hits / contacts), 1344 and 1493
  (start and end of the spin effect those hits cause; FUN_8006C388);
- free run with those events injected: every motion field and render range
  exact on all 3510 ticks; at the finish lap times, best lap, race time,
  place (3rd), shown place, progress and the AI hand-over bit match;
- FUN_800837F8 / FUN_800834C0 on the oracle's six karts: progress, course
  point, positions and flashes reproduced on all 3511 frames;
- standalone session from race load (start_probe, 834 frames, no buttons):
  countdown, race clock and every kart field equal except random-pointer
  driven ones (blink, sway, event bits: the AI karts share the pointer),
  AI contacts (close kart, flag bit 1) and the first copy button (drawn at
  init after the AI karts' draws; injected from the oracle);
- start game (analysis/oracle/start_game, route start_game.json: face
  buttons in turn through the minigame, 70 frames with a press): countdown,
  race clock, start-boost count, holds, engine rev, GO, best start 20 and
  every non-random kart field equal on every frame to GO, with the next
  button to copy taken from the oracle (random pointer; tests/race.rs);
- standalone session on the race3 route (`mr-standalone --capture
  analysis/oracle/race3P --oracle-dt --catchup-as-6th`): motion and camera
  exact through tick 662; at 663 an AI kart bumps the player (contact pass,
  +0x501 / +0x410): the first AI-caused divergence.
Not ported: what the AI does with the player's kart after the finish (see
the revisit list), the results screen / mode-10 timer 0x800B0AF0.

## 10. 60 Hz logic (dt = 1) — first pass
- Position history: the game's ring is 12 entries (12 VBlanks at dt = 1),
  but the chase camera walks 16 VBlanks back, so dt = 1 hit
  `unreachable_in_scope!` in camera.rs. Fixed: the sim's ring is
  `camera::TRAIL_LEN` = 24 entries; `GAME_TRAIL_LEN` = 12 is what oracle.rs
  loads / stores and the camera tests compare (entries 12.. repeat the
  oldest when loaded from RAM). At dt >= 2 the walk ends within 9 entries,
  so dt = 2 / 3 results are bit-identical (all tests green).
- Measured (`cargo run --release -p mr-sim --example dt_compare --
  tools/oracle/routes/lap1.json 1500`: both sessions idled to the oracle
  grid, catch-up as the oracle race, then the route's buttons at dt 2 and
  dt 1 compared at equal game time): beyond the player tolerance (1169
  units / 2 deg) from tick 73; |dpos| ~2300 at tick 100, ~20000 by tick 500,
  ~400000 by 1400 (after wall contacts the runs part ways); mean 68229.
  (mr-bevy/examples/dt1.rs idles 143 ticks, which with the countdown now
  ends inside the countdown; it needs `idle`-to-race-time 1430 alignment.)
- Terms not dt-correct found so far (inference from the per-field diff of the
  two runs; no fix yet, none can change without changing dt = 2):
  - the kart-frame <-> world-frame velocity conversion (0x80067028: fwd /
    side from vx / vz and back, each `>> 12` with truncation) runs once per
    tick around the dt sub-steps: at dt 1 it rounds twice as often (first
    accelerating tick: forward speed 162 at dt 2, 161 after two dt 1 ticks;
    position 1 unit off, growing to ~100 units in 10 ticks);
  - heading and steering (yaw from +0x2D2 steer / 8 per sub-step) are applied
    inside the sub-step loop, but the velocity is rotated to the new heading
    once per tick: turning at dt 1 rotates the velocity more often (dheading
    stays 0 on straights, reaches 1-2 deg in turns);
  - per-tick random kicks to the body-sway springs (+0x3A0..) and per-tick
    random draws (engine pitch, blink): twice as many draws at dt 1, so the
    random stream and the sway differ from tick 0 (cosmetic);
  - the look-at target search (FUN_80041A20) runs per frame: aim found at a
    different frame (cosmetic, feeds the sway);
  - per-tick events: pushes +0x45E / +0x462 are cleared each tick after the
    sub-steps, the history ring gets one entry per tick (count = dt).
  Next: list the remaining per-tick constants in update.rs (timers that step
  by a constant instead of dt, easing `x += (t - x) >> n` once per tick:
  lurch, steer lag, catch-up ramp, boost glow) and measure each one's share
  of the divergence by making it per-VBlank in the dt = 1 run only.

## 11. Six-kart session (AI integrated)
`Session::new` loads the race as FUN_8008C4AC: grid draw (`ai::grid_start`),
the player's FUN_800609F4, then per AI slot the opponent draw
(`ai::pick_opponent`) and its FUN_800609F4 (`ai::init_opponent`, vehicle
template from `ai::vehicle_templates` + the vehicle model's wheels), then
FUN_8009192C over every kart (`ai::race_load_roll`). Per frame: FUN_8005C2EC
(GO now includes the AI start boost), water, contacts over every pair,
FUN_80041A20 (the human) and FUN_80041CC4 (every kart), FUN_8009004C for the
round-robin kart (none while the countdown runs), the kart loop in slot order
(AI and the finished player: `ai_buttons` -> `items_off` -> update with
player -1; humans: the pad; FUN_80086B7C for human slots), then the render
pass and race positions for every kart.

Verified:
- race load against the oracle (analysis/oracle/start_probe, with the
  oracle's seed / pointer / water state): opponents, grid, all six records'
  named fields, random pointer and rand() seed equal; then every kart's
  motion fields and buttons (item bits masked) exact for 372 frames
  through the countdown and GO to 130 race ticks; at 373 an oracle AI kart
  turbos (items on in that capture) (tests/race.rs
  six_kart_race_load_matches_oracle);
- the player's race3 route in the oracle-fed harness: unchanged (exact to
  the finish);
- AI behaviour, 18 standalone races (`examples/full_race.rs --oracle-lineup
  --seed S`, player idle, items off) vs the six items-off oracle races
  (analysis/ai/n_*), tools/oracle/ai_stats.py: lap means +0.3 / -0.5 / -0.5
  / +0.2 / +1.1 % per kart (limit 3 %), pooled +0.11 %, line median /
  p90 deltas <= 0.012 / 0.057 (limit 0.10), mean finishing positions within
  0.5 places (limit 1.5): PASS;
- the finished player driven by the AI past the finish (tests/race.rs).
