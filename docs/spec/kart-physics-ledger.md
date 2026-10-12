# Kart physics: differences ledger (port/crates/mr-sim)

Every place the Rust kart deliberately differs from the game, every stubbed
branch, and every input the port takes from outside. Companion to
kart-physics.md.

## 1. Stubbed branches (`unreachable_in_scope!`, panic when reached)
Paths the solo player does not take on DOCK. Address = the game code skipped.

| where | branch | why out of scope |
|---|---|---|
| update.rs | copy-the-buttons / boost-start minigame while the countdown runs (0x80064BA8) | boost-start minigame |
| update.rs | nearest-camera search with 2+ humans (0x80064FB0) | multiplayer |
| update.rs | TRIANGLE look-at when no kart is near / mode 0x14 (FUN_800728C0) | camera lock search, challenge mode |
| update.rs | special state +0x4D6 != 0 (track reset / special move, FUN_800913F4) | special move / reset |
| update.rs | AI catch-up scaling (0x80065850) | AI |
| update.rs | R1 with a full meter (+0x48A = 0x1A4) on a path: special move (0x80065B5C) | special move |
| update.rs | L1 / L2 with a charged weapon slot: fire (FUN_80068C6C) | weapons |
| update.rs | R2 with a full meter: character special (FUN_8006B828) | special move |
| update.rs | jump while carrying an object (+0x4E8 != -1) | items |
| update.rs | status bits 0x40110 / 0x40 / 0x20 / 0x400 / 0x2000 / 0x8000 / 0x80 / 1 (homing, forced float, frozen, pushes, wobbles) | item / weapon effects |
| update.rs | effects +0x4F1 = 0x15 / 0x16 | item effects |
| update.rs | time-trial ghost FUN_8006F64C (race type 3) | not a race |
| update.rs | race over: kart handed to the AI (+0x4C7 = 2) | AI after the race |
| update.rs | respawn timer +0x47A expiring (FUN_8009192C) | not reached on DOCK |
| ground.rs | ground contact in a special state; wall hit while carrying | special move / items |
| progress.rs | object lists FUN_800628FC / FUN_80061BC4 / FUN_800632BC / FUN_80063C24 / FUN_80062E38 / FUN_800627E0 with non-empty lists | DOCK has none (pointers 0x800D1C68/8C/98/A4 null, 0x800B0C58/0E00/0C68 zero) |
| progress.rs | DOCK's two trigger objects (0x800D1C9C, count 0x800B0DC6 = 2) touched, pickup states (+0x4E6, +0x4E7) | items |
| progress.rs | lap timing in modes 2/7/20, race flags 8/2, time trial, 10-minute limit | other modes |
| progress.rs | lap record beaten, second lap end (best-lap compare), lap announcement threshold, race finished, final-lap banner, finish line crossed backwards twice | not reached by the one-lap route; small, port next |
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
| race globals | SessionConfig (race settings of the oracle savestate: laps 3, event 3, cup 4, lap record 12186, end counter 1) and the ported clock FUN_8005C2EC |
| buttons, dt | caller (`tick`: dt 2; `tick_dt` for analysis) |
| path position, wobble | FUN_8009004C port, every frame (one kart) |
| water normals | FUN_8003C5FC port, state from the exe's data section |
| random pointer | reset 0x8001F94C at race load, FUN_8007D304's per-frame step and wrap |
| body matrix | FUN_80024594 port |
| leader progress | the player's own (no AI: the player leads) |
| other karts | none |

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
- Race position FUN_800837F8 / FUN_800834C0 (+0x388, +0x4BE): with one kart the
  player is first; +0x4BE keeps the grid value 6 from FUN_800609F4 and progress
  stays 0, which together give the same catch-up (0) as a leader would get.
- Turbo-meter tokens FUN_8007BBF0 (+0x48A += 12 per token collected on the
  track): not ported, the meter stays 0, so R1 turbo never fires and the boost
  glow target stays 0x60.
- FUN_80081170 / FUN_8006F410 record fields +0x488 / +0x490 at init (course
  point index, zone), the model/character slot numbers +0x4A4 / +0x4A5, the
  start-light minigame (+0x4B8, +0x4D8; the session starts at race clock 0 with
  no countdown), flag bit 1 of +0x496 (set elsewhere).
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
