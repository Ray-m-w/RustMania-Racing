# Kart physics: the player kart's per-tick update (port/crates/mr-sim)

Evidence tags as in kart-state.md: [CODE addr] read from the game's code (Ghidra
decomp as reading aid, generated C / MIPS as spec), [RAM] matched the oracle,
[INFER] unverified. Everything below is [CODE] and verified bit-exact against
the oracle unless marked otherwise (Verification, end of file).

## Where it runs
The race frame FUN_8007916C runs once per game tick (dt = 0x800B0AEC VBlanks,
normally 2, 3 in heavy scenes). In order:
1. FUN_800306D0, FUN_8003C5FC (water/wave animation: normals 0x800B0CA0/0x800B0EA8,
   draws random bytes), FUN_80063A98.
2. FUN_800418D8: kart-kart contacts, every pair (j, i), j < i (FUN_80040018),
   then FUN_80041A20 / FUN_80041CC4 (look-at target +0x2C0..+0x2CC), FUN_8009004C
   (race progress / path position +0x498/+0x49A).
3. For each kart: FUN_80064B5C(kart, buttons, player) (AI karts: buttons from
   FUN_800912F8, player -1); for human karts then FUN_80086B7C(kart).
4. FUN_80086DF0 (wheel/body render matrices), FUN_800837F8 (race positions),
   FUN_80025D8C (body matrix +0x18C), ..., FUN_80069648 (projectiles: hits on
   karts), camera FUN_80070418, ...

mr-sim ports 2 (contacts, `collide.rs`) and 3 (`update.rs`, `ground.rs`,
`progress.rs`) for kart 0. `mr_sim::tick` = step 3; `mr_sim::tick_with_karts` =
the contact pass, then step 3.

## Fixed-point conventions (`fixed.rs`)
- Positions: i32 world units (TER units << 5); y grows downward. Angles 12-bit
  (0x1000 = 360 deg). Heading h: forward = (sin h, cos h) in (x, z).
- Sine table [exe 0x800A88C0], 5120 entries, 1.0 = 0x4000; cosine = entry + 1024
  (pointer 0x800AFF20 = 0x800A90C0). Almost always read as `(short)t[a] >> 2`
  (4.12); a few places use the raw entry.
- Signed division by 2^n is the compiler's bias-and-shift (`div_pow2`), MIPS `div`
  truncates (`div`), products keep the low 32 bits (`mul`), halfword/byte stores
  truncate (`s16`, `s8`). Per-tick rates run as `dt` sub-steps
  (`for _ in 0..substeps(dt)`), others are multiplied by dt.
- ratan2 [CODE 0x80095010] and SquareRoot0 [CODE 0x80093D70] are the PSYQ
  routines; their tables [exe 0x800AD608 (1025), 0x800AD2E8 (192)].
  GTE: only LZCS/LZCR (SquareRoot0): LZCR = number of leading bits equal to
  bit 31 of LZCS (psx-spx "GTE Registers"). No other GTE op is on the path.

## FUN_80064B5C, in order (update.rs)
```
buttons = mode == 0x12 ? 0 : buttons;  kart.buttons(+0x44E) = buttons
engine_rev(+0x4DE) -= dt                          ; engine sound draws 1 random byte
[SELECT toggles +0x4F9; TRIANGLE: +0x4FE = nearest kart (camera look-at), FUN_800728C0]
history ring (+0x2DC -> 12 x {x,y,z,dt}): shift, push {pos at tick start, dt}
speed_est(+0x400) = SquareRoot0(|pos - prev|^2 in x,z) / dt
engine(+0x442) eases to 0x1000 by 0x80*dt
controls (0x80065510):  accel = CROSS (bit 14) always
    brake/jump = CIRCLE/SQUARE, or SQUARE/CIRCLE when 0x800E0392[player] == 1
    left = bit 7, right = bit 5; status bits 4/8 or stun > 999 disable them
stats (0x800655E0): from vehicle row [exe 0x800ACDB0 + 8*vehicle(+0x4B1)] bytes 1..4:
    accel(+0x17A) = b1*75/100, top(+0x17C) = b2*96/100,
    steer_rate(+0x17E) = b3*36/100, stat_d(+0x180) = (b4<<7)/100
catch-up (0x80065714): if race_pos(+0x4BE) > 1: target = clamp(leader.progress -
    progress, 0, len) * 180 / len (len = DAT +0xA48 halfword >> 4);
    catchup(+0x4C3) moves toward it by dt
weapon slots recharge (+0x4D0..+0x4D5; new random type every 8 charge)
shoulder buttons: R1 = turbo (uses +0x48A meter), L1/L2 = fire weapon slot,
    R2 = character special; only their "not ready" paths are in scope
prev = pos; prev_vy16 = vy; old_fwd/old_side saved
cap = 0x40 + 16*catchup/180
limits by float_state(+0x4C8):  0: max = cap*32, hard = 0xE00, thrust = top
    1: (airborne, not floating) max = cap*32/3, thrust = 2top/3, hard 0x4AA
       else max = cap*64/3, thrust = 3top/4, hard 0x955
    2: max = cap*32/3, thrust = 2top/3, hard 0x4AA, accel /= 4
drag = accel/2; grip = accel; airborne: everything halved; on ground with
    float 0: max, thrust, drag, accel scaled by surface bytes 0..3 / 128
    [exe 0x800ACCF0 + 8*surface]
jump (SQUARE): on ground -> vy = ((engine/2 + 0x800) * -0x700) >> 12, state machine +0x4E2
spin-out: spin_rate(+0x482) decays by 1/4 per sub-step unless spin_timer(+0x484)
accel+brake on the ground: power turn: heading += steer*min(fwd>>6,16)*dt >> 6
on ground: heading += slope_turn(+0x474)*dt
steering (sub-steps): steer(+0x472) += -+steer_rate (left/right, swapped when
    reversing), clamp +-8*steer_rate, decays by steer_rate when released;
    heading += steer >> 3 (when not spinning)
steer_lag(+0x47C) follows steer
throttle holds (+0x4AA/+0x4AB/+0x4AF), lurch(+0x480), skid (1 random byte)
conveyor faces push the kart along +0x476
turbo: boost_count(+0x446) or (R1 and meter +0x48A): thrust 0x80, max = hard = 0xE00, grip 0x4B
else thrust = accel ? top-ish : brake ? -3/4*thrust : 0
velocity (0x80067028): rotate (vx, vz) into the kart frame (side, fwd), add the
    pending push (+0x45E/+0x462, then cleared); per sub-step:
      side -> 0 by grip, soft cap +-max (1/8 of the excess per sub-step)
      fwd += thrust, or -> 0 by drag when no thrust; below -max/2 recover 1/8;
      above hard recover 1/4; above max (not boosting) recover 1/8
    fwd_speed(+0x390), side_speed(+0x394) = final; (vx, vz) = rotate back;
    pos += rotate(sum of the sub-step velocities)
drift meter(+0x486), reversing(+0x4C9)
vertical (0x80067608): floating (float 2, or 1 with chassis bit 1): y eases to
    the water line by 1/0x50 (1/0x14 hover) per sub-step, vy damped;
    else gravity: vy += 0x60 + gravity_mod, cap 0x1800 down, soft cap -0xC00 up;
    y += vy per sub-step
contact phase (progress::contact_phase): object lists (empty on DOCK),
    trigger proximity, FUN_8003E91C (below)
wheels: spin_rate eases toward fwd*11/radius (+drift on driven wheels)
sliding(+0x4E1) = |fwd| < |side|
lap timing FUN_80082708; respawn timer; race-end check
body sway springs FUN_80064738 (+0x3A0.., render-side), suspension bounce
    (+0x348/+0x34C/+0x350), yaw_rate(+0x448) = heading change / dt, shape lerp,
    boost glow, sway outputs (+0x454..+0x45C), blink timer, wrong-way timer
```

## FUN_8003E91C, ground contact (ground.rs)
```
walls (FUN_8003DA18): for each wall edge of the destination grid cell the move
  prev->pos crosses (mr_assets::ter::Ter::wall_candidates), FUN_8003D240:
    if the wall has height, the crossing must be between its interpolated top
    and bottom (+-table[exe 0x800A88AA][dt+5]);
    bounce: velocity in the wall frame (along = max(along, 0x100)),
    along' = -along*wall_bounce(+0x175) >> 4; new pos = prev + v'*dt;
    FUN_8003D760: if prev->new crosses a wall again, stay at prev.
  first hit wins; rumble, 3-4 random bytes (sparks)
ground query (Ter::ground = FUN_8003DDB0) at (x, (2y+probe_top+probe_bottom)/2, z):
  no surface -> undo the move, zero velocity, return
  ceiling (alt) surface between the probes -> push down, ceiling_hit(+0x4BA)
  floor: y = min(y, nearest_y - probe_bottom); ground_y(+0x31C)
  smoothed normal(+0x2D0..) eases to the floor face normal by speed_est*dt/2
  surface(+0x4F3) = face kind; water_y(+0x398)
  ground_contact(+0x4E0): +dt (max 0x40) when within 0x200 of the floor, else 0
  conveyor (kind 0x0C quads)
  float_state: 2 if the water line is above y - |ride_height|*32, 1 if above y
  support_y(+0x320); surface bytes 5..7 -> +0x4BB..+0x4BD
  sink(+0x464) toward sink_target; vy on landing / leaving the ground = dy/dt
  slope push: push += normal(x,z) * (0x60 | 0x30 in water) >> 12
  target pitch/roll from the normal (ratan2), eased by 1/8 (1/4 just landed)
  FUN_8003CD7C: slope_roll(+0x470), slope_turn(+0x474) = -roll_angle*fwd >> 18
  suspension input, FUN_80064384 body lean
```

## FUN_80086B7C (progress.rs)
`track_dir`(+0x48E) eases (1/8 per sub-step) toward the direction of the path
point n points ahead of (+0x498, +0x49A), n = clamp(speed_est>>5, 24, 55) >> 3
(3 while the wrong-way timer runs); FUN_80090424 walks the TER path (first
link at a branch for the player, so rand() is never called).

## Kart-kart contact (collide.rs)
FUN_80040018(a, b): bookkeeping (nearest kart, look-at target); vertical
overlap of the probe spans (scaled by +0x442) and horizontal overlap of the radii
(+0x176); landing on top: velocities exchanged, lower kart spins out
(FUN_8007B26C, toward the track direction for humans); already overlapping:
push apart via +0x45E/+0x462; first side contact: velocity components along the
contact line exchanged, both spin by the contact angle, the "loser" (scores:
push, angle, speed; tie = random byte) spins harder.

## Verification
- Function level: tools/oracle/fixtures.py runs the game's own MIPS
  (tools/oracle/mipsref.py, an R3000 interpreter over the oracle's RAM) for
  every tick of analysis/oracle/lap1P3 and records each call's entry/exit;
  `cargo test -p mr-sim --test fixtures`: FUN_80064B5C, FUN_8003E91C,
  FUN_80086B7C, FUN_80082708, ratan2, SquareRoot0 bit-exact on all 1543 calls
  (every owned field and the random pointer).
- Lap level: `cargo run --release --bin mr-verify` (and tests/oracle_lap.rs) on
  analysis/oracle/lap1P3 (1537 ticks): teacher forcing exact on every motion
  field on every tick except tick 821 (an AI projectile hit, FUN_80069648,
  out of scope); pure free run exact for ticks 1..821; free run with that one
  event injected exact on all 1537 ticks (`mr-compare` vs
  analysis/traces/lap1.oracle.trace.jsonl: max |diff| 0 on every field).
- Layers (physics captures of the short routes, same savestate, ram0.bin
  byte-identical to lap1P3): layer 1 analysis/oracle/c_crossP (accelerate on the
  start straight, 215 ticks) and layer 2 analysis/oracle/c_steerP (steer left /
  right, into the water at the dock edge, 215 ticks): teacher forcing exact on
  every motion field, free run exact on every tick. Layer 3 (walls, slopes,
  ramps, water, kart contacts) and layer 4 (the lap) are lap1P3 above. The
  older default-region captures (c_cross, c_steer, probe, c_triangle, c_idle)
  also run with `--base analysis/oracle/lap1P3`, but they do not record the
  water normal or the other karts, so they diverge once the kart enters the
  water (c_steer, tick 158) or is bumped (c_idle, tick 1266).

## Standalone session (session.rs; no oracle data at run time)
`Session::new(&mut disc)` builds a one-player DOCK race from the disc alone:
- World: SLUS_012.37 tables, DOCK.TER, DOCK.DAT course records by chunk id
  (grid 1, finish line 3, course length 5, triggers 0x15; the ids the .DAT
  loader FUN_8002E3DC registers at 0x800D1C48 + 4 * id).
- Vehicle template (vehicle.rs): FUN_80087EB4's stores for vehicle 0 (code
  immediates, cited per instruction) + the exe stat row 0x800ACDB0 + wheel
  pivots/radii from VEHIC_A/RACER.MDL parts 0..3 (FUN_80087B6C). Matches the
  game's own FUN_80087EB4 run on a cleared table byte for byte except the wheel
  fields, which match the RAM the model loader produced.
- Kart (init.rs): FUN_800609F4(kart 0, Kermit, RACER, grid 5, flags 0): grid
  point from DAT chunk 1 (entry 5 = the oracle's slot), heading toward the
  entry's second point, ground query, pitch/roll from the floor normal
  (FUN_8003CCBC), identity matrices, template copy, camera mount (the
  character table 0x800AC038 with its first halfword replaced by |KERMIT.MDL
  part 0 pivot y| as FUN_80087ACC does at model load), probe span, history
  ring, path position.
- Globals: the race settings of the oracle's savestate (SessionConfig).
Per tick (`Session::tick(buttons)`, dt = 2): water animation FUN_8003C5FC
(water.rs, random pointer modelled: reset to 0x8001F94C at race load, stepped
and wrapped once per frame by FUN_8007D304), the contact pass's resets, path
position FUN_8009004C (kart 0 every frame with one kart; note the game measures
point j-1 and records index j), the kart tick, body matrix FUN_80024594 via
FUN_80025D8C (body.rs), the camera (camera.rs, ground query from Ter::ground,
first tick places it), frame counter and race clock (FUN_8005C2EC).

Verification: water.rs = the game's FUN_8003C5FC on 400 random states (angles,
targets, random pointer, dt 2/3) bit-exact (tools/oracle/fx_water.py,
tests/standalone.rs); path_position = the oracle on all 256 lap frames where
the game updated kart 0's path; body matrix = the oracle on every lap tick;
Session::new after 143 idle ticks (the oracle savestate's race clock 1430)
equals the oracle's grid record in every motion field and the body matrix
(differences listed and explained in tests/standalone.rs and the ledger).
