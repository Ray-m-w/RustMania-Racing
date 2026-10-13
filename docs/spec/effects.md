# Effects: particles, sparks, pickups, projectiles, animated textures

Session 4, track B. Rust: `port/crates/mr-sim/src/particles.rs` (the
simulation), `port/crates/mr-bevy/src/effects.rs` (drawing),
`port/crates/mr-bevy/src/animtex.rs` (animated textures). Checks:
`mr-particles CAPDIR` (sim vs oracle), `tools/oracle/fx_capture.py` (RAM +
GP0 per run), `tools/oracle/fx_shots.py` + `fx_sbs.py` (screenshots).

Tags: **[CODE addr]** read from the game's code, **[RAM]** matched against the
running recomp, **[INFER]** unverified.

## What the race render pass draws (FUN_80029F40)

After the scenery, in this order [CODE 80029f40]: FUN_80034AE4, the
start banner FUN_800351E4, FUN_80030438 (clears the billboard batch count),
the kart shadows FUN_8002ADA8, FUN_800342B8, the spark streaks
FUN_80032928, the world billboards FUN_8007E21C (pickups, kart sparkles),
the kart status effects FUN_80037B80, the projectiles FUN_80030D78, and
FUN_80030444 (flushes the batch). The sprite particles are drawn by
FUN_80010E90 from the same function.

**The 0x800DF688 buffer is not a particle list.** It is a 64-entry batch
(0x1C bytes each) of camera-facing billboards: FUN_80030474 (and the inlined
copies in FUN_80030D78 / FUN_80037B80) append one, FUN_80011E3C projects and
emits a batch as POLY_FT4 (flushed at 64 and at the end, FUN_80030444). It
is rebuilt every frame from the lists below. [CODE] [RAM: 0x800B0A8C count]

## Sprite particles: 0x800E0C20 (FUN_800870E8 / FUN_8008727C / FUN_80010E90)

64 entries x 0x3C bytes + a side table 0x800D73D0 x 8 bytes [CODE]:

| Offset | Field |
|---|---|
| +0x00..+0x08 | position x, y, z (world) |
| +0x0C..+0x14 | velocity per VBlank |
| +0x18 | gravity (added to vel.y each VBlank before y moves) |
| +0x1C | -1 at spawn |
| +0x24 | life, VBlanks (<= 0 free; -1 for the rest of the dying frame) |
| +0x26 / +0x28 | half width / height, view units (world >> 5) |
| +0x2C | sprite definition index (0 = not drawn) |
| +0x2D | particle type |
| +0x2F | behaviour (fade / size mode) |
| +0x30 / +0x31 | frame count / current frame |
| +0x32 | flags: 0x60 blend (0x60 opaque), 0x10 flat on the ground, 0x200 on screen (the drawer's) |
| +0x34 / +0x36 | rotation (12 bit) / rotation per VBlank |
| +0x38..+0x3A, +0x3B | colour now, GPU code (0x2C, or 0x2E semi-transparent) |
| side +0..+2, +3, +4, +6 | colour at spawn, life at spawn (byte), width, height at spawn |

- **Spawn** FUN_800870E8(x, y, z, vx, vy, vz, gravity, angle, spin, type,
  r, g, b, w, h, life): nothing when w or h is 0; else the ring index
  DAT_800B0C60 is pre-incremented (wraps at 64) and the slot overwritten;
  type table 0x800ACFFC (4 bytes: sprite, frames, blend flags, behaviour),
  flags = table | 0x200. [CODE 800870e8] [RAM]
- **Update** FUN_8008727C, once per race frame (FUN_8007916C), dt VBlanks:
  life -= dt (clamped at 0); dt sub-steps of x += vx, z += vz, vy += g,
  y += vy; frame = (frames - 1) * (life0 - life) / life0; angle += spin * dt;
  behaviour 1..10 -> (fade, size): 1 (2,1), 2/5 (1,0), 3 (2,0), 4 (1,1),
  6 (0,1), 7 (0,2), 8 (2,3), 9 (1,4), 10 (0,6). Fade 1: colour = base *
  life / q below q = life0 >> 2; fade 2: also fades in over the first
  quarter. Size 1: grow to double; 2: shrink over the last quarter;
  3: +8 dt; 4: wobble from the sine table at two phases (+0xA0 / +0x80 per
  slot); 5: +dt; 6: shrink over the life. Life 0 -> -1, and after the loop
  every -1 -> 0 with the sprite index cleared. [CODE 8008727c, jump tables
  0x80023638 / 0x80023660] [RAM]
- **Draw** FUN_80010E90: sprite definition *0x800D1B40[sprite] (exe
  pointers 0x800ACF40): +0 texture slot, +2 / +3 u / v span, +8 frame
  (u, v) pairs; tpage from the slot (4-bit pages take the CLUT offset
  ((v0 & 0xC0) >> 4) + (u0 >> 6) of frame 0, FUN_8005CC94); a quad of half
  size (w, h) view units around the view-space position, x stretched by
  1.6 (0x1999, the 512-pixel screen's aspect), rotated by the angle, or
  flat on the ground (flag 0x10); colour modulated, blend from the flags.
  [CODE 80010e90, 8005cc94]

**Verification** (`mr-particles analysis/fx/race3b`, oracle race3 with
items on, 8,690 VBlanks): teacher-forced one race frame at a time over
4,323 frames, **271,530 / 271,530 slot-frames exact** (61,945 of them live),
the drawer's 0x200 bit masked; 5,142 spawns seen, all with the type table's
fields. Behaviours exercised: 0, 3, 4, 7 (types 1, 2, 8, 9, 12, 13, 15, 17,
20, 24, 25, 26); 1, 2, 5, 6, 8, 9, 10 are [CODE] only. Unit tests:
`cargo test -p mr-sim --test particles`.

The wobble's two phases are two stack shorts FUN_8008727C never
initialises (left by the previous call at that depth); the port starts
them at 0 each frame [INFER]. Only behaviour 9 (type 19, not seen on DOCK)
reads them.

## Spark streaks: 0x800B4540 (FUN_800326B4 / FUN_800327D0 / FUN_80032928)

32 x 0x20: position, velocity per VBlank (+0x0C), life byte +0x18 (0xFF
free), length +0x19, colour at spawn +0x1A..+0x1C, colour now +0x1D..+0x1F.
Spawn: the next ring slot (DAT_800B07EB, wraps at 32). Update every race
frame for every slot: life -= dt, any result >= 0x80 -> 0xFF; colour =
base * life / 8 (toward zero) while (signed) life < 8; x += vx dt,
z += vz dt, y += vy dt (no gravity). Draw: a gouraud LineG2, additive
(abr 1), black at the spark to its colour at pos + vel * len. [CODE 800326b4,
800327d0, 80032928, 80032120] [RAM]

Verification: **137,447 / 137,447 slot-frames exact** over the same 4,323
frames; 889 spawns.

Spawners (track A's item code and the kart code; each keeps its own random
draws): particles from FUN_80068C6C, FUN_80069334, FUN_80069648,
FUN_8006EA2C, FUN_8007BBF0, FUN_8007F03C, FUN_80082708, FUN_8008650C and
the item cases 0x8006CE58..0x8006E1D4; sparks from FUN_8003E91C,
FUN_80040428, FUN_80069648, FUN_8007F03C. [CODE: callers]

## Pickups and projectiles (drawn from track A's item state)

- **Turbo tokens** (FUN_8007E21C): DAT_800B0A52 entries at 0x800E2370
  (stride 0x20; x, z in world >> 5, y world; scale +0x16; colour +0x1F)
  in 4 groups (i % 4), each spun by DAT_800B0B90[group] + the camera's
  screen roll (camera +0x54). Table 0x800AC1B8 (8 bytes: u, v, uw, vh,
  slot, clut, divisor): half size = scale * uw / div, scale * vh / div.
  Opaque, wide cull (0x160). [CODE]
- **Item boxes**: *DAT_800D1C94 entries at 0x800E1B70 (when DAT_800B0B60):
  the star sprite, slot 0x15 clut 0xB uv (0xC0, 0x80) 0x37 x 0x37, half
  size 2 * scale, rotated with the camera roll. [CODE]
- **Projectiles** (FUN_80030D78): 4 x 0x48 at 0x800DDC50, drawn while
  +0x2E: flags 0x40 a wobbling star (0x90 x 0xC0, uv (0x40, 0x82) 0x27 x
  0x3D, slot 0x14 clut 9, standing on the point), flags 1 uv (0x40, 0x56)
  0x29 square slot 0x15 clut 5 (0xC0), 0x400 a 3D model (FUN_80030AD8, not
  drawn yet), else uv (0xC0, 0xC0) 0x13 x 0x37 slot 0x15 clut 0xF (0x64 x
  0x118); angle +0x3A. [CODE]
- Not drawn yet: kart status effects FUN_80037B80 (+0x599 cases), the
  special-move glow (+0x4D6, two sparkles per kart in FUN_8007E21C), the
  kart flames FUN_8007D660 (+0x4B4 > 0xF), FUN_800342B8, the course
  ambient list 0x800D75D0 (DAT_800B0ACA, off on DOCK).

## Animated textures (RAW type 0x22): FUN_8007A9DC -> FUN_800311AC

Every race frame FUN_8007A9DC calls FUN_800311AC(slot, 0, frame), which
LoadImages frame `frame` of the 0x22 strip DAT_800BBB20[slot] (4-bit; or
DAT_800CB680[slot], 8-bit) to the VRAM rectangle of sprite definition
*0x800D1B40[slot]: x = page x + u / 4, y = page y + v, w = strip w / 4,
h = strip h. Frames on DOCK (course 3): 0xA and 0xD (f & 0x3F) >> 2,
0xB (f % 0x36) / 2, 0xC (f & 0x3F) >> 3, 0x19 and 0x27 (f & 0x1F) >> 1,
of the frame counter DAT_800B0D90; 0x1E: 7 - ((c & 0xF) >> 1) of
DAT_800B07D4 (FUN_8005C2EC). DOCK has strips 0xA, 0xC (water), 0x19,
0x27 (the HUD stars on texture slot 0x15) and 0x1E. [CODE 8007a9dc,
800311ac, 8005c2ec] [RAM: mr-raw finds those frames in the oracle VRAM]

mr-bevy animtex.rs does the same from the session's frame counter and
rewrites the affected texels of every cached page image
(`VramTextures::write_rect`).
