# Kart state (this repo; supersedes the quarry's kart-state.md for the recomp)

Evidence tags: [CODE addr] read from the game's code (Ghidra decomp of the
recomp's function list, analysis/ghidra/), [RAM] observed in oracle captures
(tools/oracle/capture.py, DOCK grid savestate), [INFER] unverified inference.

## Where karts live
- Kart array at **0x800C30A8**, stride **0x504** bytes, count
  `*(s16*)0x800B0DFE + *(s16*)0x800B0C98` (6 + 0 in a 1-player DOCK race) [CODE 0x8007916C] [RAM].
  Slot 0 is the human player. The quarry's "player context 0x800C30A8" is this
  record; its "entity record 0x801C4D84" is render-side data.
- Per tick, FUN_8007916C (the race frame) loops over karts and calls
  **FUN_80064B5C(kart, buttons, player_index)** [CODE]. Human karts get
  `buttons = *(u16*)(0x800B0CE8 + 2*player)`; karts with `+0x496 & 1` are AI and
  get buttons from FUN_800912F8(kart), player_index = -1 [CODE]. After each
  human kart: FUN_80086B7C(kart). After the loop: FUN_80086DF0(karts, n)
  (kart-kart), FUN_800837F8(), FUN_80025D8C(karts, n); camera
  FUN_80070418(0x800C4F40, kart0) [CODE].
- Tick = 2 VBlanks: `dt = *(u8*)0x800B0AEC` = 2 [RAM]; timers subtract dt
  [CODE 0x80064B5C]. The record changes only on even VBlanks after a restore [RAM].
- Game mode short 0x800B0D46 = 1 in the race; 0x12 forces buttons to 0 [CODE].

## Kart record fields (offsets from the record base)
| off | type | meaning | evidence |
|---|---|---|---|
| 0x000 | i32 | position x (world units, ~1.9M across DOCK) | [RAM] moves with velocity |
| 0x004 | i32 | position y; y grows DOWNWARD (gravity adds +0x60/sub-step to vy) | [CODE 0x80067790] [RAM] |
| 0x008 | i32 | position z | [RAM] |
| 0x00C | i16 | pitch (4096 = 360 deg) | [RAM] changes on bumps |
| 0x00E | i16 | **heading**, 4096 = 360 deg; forward = (sin h, cos h) in (x, z); LEFT decreases it | [RAM] matches velocity direction to 0.3 deg |
| 0x010 | i16 | roll | [RAM] |
| 0x014.. | 4 x 0x2C | per-wheel blocks (x cur/prev, z cur/prev, y?, ...) at 0x14, 0x40, 0x6C, 0x98 | [RAM] |
| 0x18C | 3x3 i16 | body rotation matrix, 4.12 | [RAM] |
| 0x214.. | i16 | wheel spin matrices (rotate about x) | [RAM] |
| 0x2E8/0x2EC/0x2F0 | i32 | previous position x/y/z | [RAM] |
| 0x2F4/0x2F8/0x2FC | i32 | velocity x/y/z (per tick; pos advances ~ v*dt) | [RAM] |
| 0x390 | i32 | forward speed in the kart frame (signed; negative = reversing) | [CODE 0x80067338] [RAM, bit-exact] |
| 0x394 | i32 | sideways speed in the kart frame (the oracle trace calls it yaw_rate; it is not) | [CODE 0x80067338] [RAM, bit-exact] |
| 0x358 | i32 | race time = sum of the lap times +0x35C[lap] (each +5*dt per tick) | [CODE 0x80082BBC] [RAM] |
| 0x44E | u16 | buttons this tick (active high) | [CODE 0x80064BAC] |
| 0x496 | u16 | flags: bit0 = AI / no human input | [CODE] |

### Fields of the kart update (mr-sim `KartState`, port/crates/mr-sim/src/kart.rs)
All [CODE] (address = where FUN_80064B5C and callees use it) and bit-exact
against the oracle over the DOCK lap unless noted. Model: docs/spec/kart-physics.md.

| off | type | name in mr-sim | meaning |
|---|---|---|---|
| 0x014+0x2C*i +0x1E/+0x20/+0x22/+0x26 | i16/i16/i16/u16 | wheels[i].spin/spin_rate/radius/flags | wheel rolling angle, rate (eases to fwd*11/radius), radius, flags (bit0 driven, 0x38 locked, 0xC0 off) [0x80067960] |
| 0x164/0x166/0x168/0x16A | i16 | body_front/rear/left/right | ground-probe corner offsets (shadow polygon only) [0x8003F540] |
| 0x170 | u8 | wheel_count | [0x80067960] |
| 0x171 | u8 | engine_audible | scrape loop (sound 0x1B) and slide sounds only when set [0x80064F28, 0x80067D9C] |
| 0x173 | u8 | vehicle_class | engine class bits (0x50: skid sound) [0x80066D00] |
| 0x174 | u8 | chassis_flags | bit1: floats on water (hover/boat) [0x80066014] |
| 0x175 | u8 | wall_bounce | wall rebound strength (along' = -along*b >> 4) [0x8003D5C0] |
| 0x176 | i16 | spark_offset | kart radius (contacts, sparks) [0x80040350] |
| 0x178..0x180 | i16 | steer_centre, accel, top_speed, steer_rate, stat_d | rebuilt every tick: 0x40 and vehicle stats b1*75/100, b2*96/100, b3*36/100, (b4<<7)/100 [0x800655E0] |
| 0x184 | i16 | ride_height | float depth (*-0x20) [0x80067640] |
| 0x2C0..0x2CC | i32 x4 | aim_x/y/z, aim_dist | look-at target (render; body sway only) [0x80068128] |
| 0x2D0..0x2D4 | i16 x3 | normal_x/y/z | smoothed ground normal [0x8003EDDC] |
| 0x2D6..0x2DA | i16 x3 | tilt_nx/ny/nz | normal the body pitch/roll follow [0x8003FD40] |
| 0x2DC | ptr | history_ptr | position-history ring (12 x {x,y,z,dt}; 0x800D4098 for kart 0) [0x8006529C] |
| 0x2E0/0x2E4 | ptr | floor_face_ptr / water_face_ptr | TER faces under the kart [0x8003EC94] |
| 0x31C/0x320 | i32 | ground_y / support_y | floor height; floor or water height [0x8003ECDC] |
| 0x324..0x32C | i32 | look_x/y/z | written by render code; contacts and sway read it |
| 0x348/0x34C/0x350 | i32 | bounce, bounce_vel, bounce_kick | suspension bounce [0x800681BC] |
| 0x354 | u32 | status | effect bits (all 0 for the player on DOCK) |
| 0x35C+4*lap | i32 | lap_time[lap] | [0x80082BDC] |
| 0x384 | i32 | best_lap | first lap time [0x80082F64] |
| 0x388 | i32 | progress | race progress (written by FUN_800837F8) |
| 0x398 | i32 | water_y | water surface height under the kart [0x8003EFE4] |
| 0x3A0..0x3E4 | i32 x18 | sway[] | body-sway springs FUN_80064738 (render side) |
| 0x400 | i32 | speed_est | ground speed |pos - prev| / dt [0x80065330] |
| 0x410/0x412 | u16 | hit_mask / own_bit | kart-kart overlap bits [0x80040F2C] |
| 0x414..0x436 | i16 x18 | shape[] | body shape lerp (cur/from/to) [0x80068384] |
| 0x438/0x43A | i16 | probe_top / probe_bottom | ground probe span (-12224 / 0 on the RACER) [0x8003EC24] |
| 0x43E | i16 | stun | > 999 disables input [0x800655A0] |
| 0x442 | i16 | engine | scale factor, eases to 0x1000 [0x80065398] |
| 0x446 | i16 | boost_count | boost ticks left [0x80066E50] |
| 0x448 | i16 | yaw_rate | heading change this tick / dt [0x800682DC] |
| 0x44A | i16 | prev_vy | [0x80065E14] |
| 0x45E/0x462 | i16 | push_x / push_z | velocity push applied next tick (slopes, contacts) [0x80067060] |
| 0x460 | i16 | gravity_mod | [0x80067790] |
| 0x464/0x466 | i16 | sink / sink_target | [0x8003F7EC] |
| 0x470/0x474 | i16 | slope_roll / slope_turn | FUN_8003CD7C [0x8003CEB0] |
| 0x472 | i16 | steer | +-8*steer_rate [0x800666A0] |
| 0x47C | i16 | steer_lag | [0x80066974] |
| 0x480 | i16 | lurch | throttle pitch wobble [0x80066B40] |
| 0x482/0x484 | i16 | spin_rate / spin_timer | spin-out yaw [0x80066484] |
| 0x486 | i16 | drift | [0x8006756C] |
| 0x48A | i16 | start_timer | turbo meter (0x1A4 full; written elsewhere) |
| 0x48E | i16 | track_dir | direction of the path ahead (FUN_80086B7C) |
| 0x498/0x49A | i16 | path_seg / path_pt | TER path position (written by FUN_8009004C) |
| 0x4AA/0x4AB/0x4AF | u8 | throttle_hold / coast_hold / brake_hold | ticks held, max 0x80 [0x80066A08] |
| 0x4B1 | u8 | vehicle | row of the vehicle table [exe 0x800ACDB0] |
| 0x4BE/0x4BF | u8 | race_pos / lap | |
| 0x4C0/0x4C1 | u8 | line_side / line_flag | finish-line side bits; crossed-backwards flag [0x80082D54] |
| 0x4C3 | u8 | catchup | rubber band 0..180: raises the speed cap from 0x800 to 0xA00 [0x80065828] |
| 0x4C4 | u8 | boosting | |
| 0x4C8 | u8 | float_state | 0 ground, 1 just above water, 2 in water [0x8003F120] |
| 0x4CC..0x4CF | u8 | latch_l1/l2/r1/r2 | shoulder-button latches |
| 0x4D0..0x4D5 | u8 x6 | weapons[] | weapon slot types and charge (0x40 = ready) [0x80065954] |
| 0x4D7 | i8 | player_index | |
| 0x4DC | i8 | nearest_kart | contact pass [0x800401A8] |
| 0x4DD | u8 | event_bits | character-voice requests, bits 0..5 (contacts, walls, finish); cleared every frame by FUN_800306D0, played by FUN_8007A7F4 (table 0x800AC232) [docs/spec/audio.md] |
| 0x4F2 | i8 | camera | the human's camera index; FUN_8007A620 measures sound distances from it [0x8007A698] |
| 0x4F5 | u8 | (voice timer) | VBlanks until the character may speak again (60) [FUN_8007A7F4]; in mr-sim `RaceGlobals::sound.voice_timer` |
| 0x4E0 | u8 | ground_contact | ticks on the ground (max 0x40), 0 airborne [0x8003F004] |
| 0x4E2 | i8 | jump_state | [0x80066380] |
| 0x4F3 | i8 | surface | TER face kind under the kart [0x8003F428] |
| 0x4F6/0x4F7 | u8 | conveyor / conveyor_prev | [0x8003F034] |
| 0x502 | u8 | on_ground | any floor found [0x8003FBDC] |

### Item fields (Session 4, docs/spec/items.md)
| off | type | name in mr-sim | meaning |
|---|---|---|---|
| 0x1AC | 3x3 i16 | vmat | vehicle matrix (render pass); weapons launch along it [0x80068F44] |
| 0x300..0x318 | i32 x7 | target_x/yaw/z, eff_30c.. | effect parameters a character special sets (FUN_8006B828) and FUN_8006C388 steps |
| 0x330 / 0x338 / 0x340 | i32 | drv_x / ref_y / drv_z | driver position: pickups and projectile hits test it [0x8007BC9C, 0x8006A8E8] |
| 0x3E8..0x3F0 | i32 | ai_tx/ty/tz | AI steering target (TER units) / special move path target (world) |
| 0x404..0x40C | i32 | sp_x/y/z | special move chase point [FUN_800913F4] |
| 0x492 / 0x494 | i16 | contacts_from_above / contacts_below | hits dealt / taken: projectiles, specials, landing on a kart [0x8006AC38] |
| 0x49C / 0x49E | i16 | sp_seg / sp_pt | special move path position |
| 0x4A4 / 0x4A5 | i8 | model_slot / char_slot | model table slots (effect 8 counts their parts) |
| 0x4D6 | u8 | special | special move state 1 / 2 (backward) / 3 (landing) |
| 0x4E6 / 0x4B5 / 0x4EF / 0x4B3 | | pickup / pickup_timer / pickup_state / wall_flag | trigger warp FUN_80061FC0 |
| 0x4F1 | i8 | effect | character special effect on this kart (-1 none) |
| 0x4F4 | u8 | token_colour | colour of the last turbo token collected |
| 0x4FC / 0x444 | i8 / i16 | flash_timer / hit_dir | HUD message (0xF hits, 0xE collection) and its value (+1 / -1 HIT) |

## Controls
The kart update decodes the pad word itself (0x80065510..0x8006559C) [CODE]:
CROSS (0x4000) accelerates in every configuration. The only configurable part
is the byte 0x800E0392 + player: 0 = standard (CIRCLE brakes/reverses, SQUARE
jumps), 1 = alternate (SQUARE brakes, CIRCLE jumps). TRIANGLE locks the camera
onto the nearest kart (FUN_800728C0), L1/L2 fire the two weapon slots, R1 uses
the turbo meter, R2 the character special, SELECT toggles the view. The pad word
0x800B0CE8 is the raw pad state (FUN_80043F90 -> FUN_80043824, the analog stick
folded into the D-pad), not remapped. The configuration bytes are set by the
options screen (FUN_800744EC from 0x800B0DD4/5) and saved with the options word
0x800E0390..93 (FUN_8008F448). On this save 0x800E0392 = 0.
NOTE: CLAUDE.md says Triangle accelerates; the game's code says CROSS. A
Triangle-accelerate feel can only come from the host input mapping (the
modern-controls plugin), not from the game.
