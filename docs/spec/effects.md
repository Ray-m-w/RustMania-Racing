# Effects: particles, sparks, pickups, projectiles, animated textures

Session 4, track B; Session 5, track F. Rust: `port/crates/mr-sim/src/particles.rs` (the
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
FUN_8007F03C's kart particles (dust, splash, spray, exhaust, light
points, turbo puff, sparkles) and FUN_80040428's contact sparks are in
the session's lists since 2026-10-13 (track A, docs/spec/items.md "Kart
particles"); effects.rs draws them with the rest.

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
  0x29 square slot 0x15 clut 5 (0xC0), flags 0x400 FUN_80030AD8(x, y, z,
  vx, vz, slot): one of the 16 view frames 0x800A8848 (u, v, uw; vh 0x1F;
  slot 0x14 clut 7; 0xC0 square) picked by ((ratan2(vx, vz) - camera yaw
  +0x52, wrapped) + 0x92) / 0x125 and wobbling with the slot's phase, else
  uv (0xC0, 0xC0) 0x13 x 0x37 slot 0x15 clut 0xF (0x64 x 0x118); angle
  +0x3A. Weapon types 0..3 fire flags 0x1D, 0x280, 0x588, 0x78 (track A).
  [CODE 80030d78, 80030ad8]
- **Kart effects** (mr-bevy `kart_effects`, from each kart record): the
  special-move glow (+0x4D6: two additive sparkles spinning both ways,
  size (0x1000 - +0x442) * 384 / 240 >> 12, FUN_8007E21C) and the status
  effects of FUN_80037B80 by +0x4F1 that it draws inline (cases 4, 0xF,
  0x10, 0x13, 0x14, 0x17: billboards above the kart from the effect's
  record fields +0x300..+0x340, wobbling with the kart's phase +0x4D7).
  [CODE 8007e21c, 80037b80]
- **Status shapes** (Session 5, mr-bevy kartfx.rs): FUN_80037B80's
  function cases build quads in FUN_800313BC's list (0x800C2668, 0x4C
  bytes each: corners v0 +0, v1 +0xC, v2 +0x18, v3 +0x24 in view units,
  uv / colour per corner at +0x30 + 5k, slot +0x44, CLUT add +0x46, tpage
  add +0x48 (0x60 opaque), +0x4A nclip, +0x4B quad (else a triangle);
  the GPU gets v0, v1, v3, v2; FUN_800313BC culls the list at 0x2000 view
  units from the eye and sorts at (OTZ + bias) * 4). FUN_8007D58C(x, y, z,
  M) = M * (-x, y, -z) >> 7. [CODE 800313bc, 8007d58c]
  - 0 FUN_80036A34: a 3 x 3 grid in the kart's plane (matrix +0x18C),
    x in {(+0x164 - 0x80) t / 16, 0, (+0x166 + 0x80) t / 16}, z in
    {(+0x168 + 0x80) t / 16, 0, (+0x16A - 0x80) t / 16}, t = +0x468 (from
    0x10 on: min(300 - t, 0x10)); four quads (index table 0x80020F54) with
    the uv corners (0x40..0x7F, 0xC0..0xFF) rotated one step per quad, slot
    0x14 CLUT 0xD, opaque, drawn front (bias +0x20) and back (the same
    quad reversed, bias -0x20, second list 0x800C2798).
  - 0xB FUN_800365C0: an additive box at the driver point +0x330 (matrix
    +0x1CC): half width (+0x188 + +0x18A) / 2 * t / 32, height
    (+0x188 + +0x18A + 0x40) * t / 32, t = min(+0x468, 0x20); six quads
    (index table 0x80020F3C), uv (0, 0xC0)..(0x1F, 0xDF) slot 0x15 CLUT
    0xC, grey min(0x2C - +0x468, 0x20) * 4, nclip, bias -0x30.
  - 0xC FUN_800376A4: two additive triangles from the kart (matrix +0x1AC,
    along its length and its width at height -+0x16E) to the point +0x300,
    uv (0x6F, 0xAF) (0x79, 0xAF) (0x74, 0xB9) slot 0x14 CLUT 9, grey
    +0x310 / 4 at the kart and +0x310 at the point.
  - 0xE FUN_80037018 / 0x15 FUN_80037364: one opaque quad at the look
    point +0x324 (matrix +0x1EC), 0xC0 in front, width 4t / 6.25t, uv
    (0, 0x80)..(0x1F, 0xAF) slot 0x14 CLUT 8 / (0x40, 0xAD)..(0x71, 0xBF)
    slot 0x15 CLUT 9 (t: +0x468, from 0x20 on min(300 - t, 0x20)).
  The inline billboard cases sort with OT bias -0x20 (0x10: -0x60) in
  FUN_80011E3C (index = OTZ * 4 + bias * 4); the port keeps that as a depth
  pull of the occlusion test. [CODE 80036a34, 800365c0, 800376a4,
  80037018, 80037364, 80011e3c] Not seen in any oracle capture (the
  recorded races have no character specials); [CODE] only.
  **Verified (Session 6, H)** against oracle captures made for it
  (tools/oracle/status_routes.py: from slot 2, AI kart 1 fires a character
  special at the player with bridge v4 pokes: kart 1 meter +0x48A = 0x1A4,
  flags +0x496 = 5 (AI + R2 preference), character +0x4B2 = the wanted type
  (the type is the shooter's character table byte 0x800AC231 + 0x14 c = c);
  the player's flags = 3, so every AI kart is "behind kart 0"). All five
  shapes appeared on the player and match the port's drawing from the
  captured records (analysis/s6/status/sbs_status_<type>.png, VB 30..280):
  0 the lily-pad grid, 0xB the ice box, 0xC the beam, 0xE the mask, 0x15
  the glasses. Differences found and their state:
  - fixed: the karts' colour factors +0x414 / +0x418 / +0x41C (vehicle,
    FUN_80026F3C) and +0x416 / +0x41A / +0x41E (driver, FUN_800276E8), 4.12,
    were ignored by the models; 0xB sets r, g 0x400 / 0x800 and b 0x2000
    (the frozen kart turns blue), 0xE all 0x400 (dark). mr-bevy karts.rs
    `tint_rigs` gives each rig its own materials and tints them from the
    record (the gamma-space factor converted at a typical texel, 0.35).
    Kart body means at VB 280, oracle vs port: 0xB 13/32/49 vs 13/25/42,
    0xE 20/21/1 vs 16/12/1, untinted (0x15) 75/88/4 vs 80/91/3.
  - open: 0xC also draws a pink additive glow on the ground at +0x300 (GP0:
    FT4 uv (0x68, 0xA8) 0x18 x 0x18, slot 0x14 CLUT 9, every other frame).
    It is an entry of FUN_800861F0's list 0x800CB058 (32 x 0x30, count
    DAT_800B0DCD cleared by FUN_800306D0 each frame; flags 2 = this glow,
    0 = a 0x20 shadow tile (0xA0, 0) subtractive, 1 = slot 0x15 (0x80,
    0xC0)), filled by FUN_8006C388's effect animations (0xC: at +0x300,
    size 5, colour +0x310) and by the weapons (projectile shadows). Its
    drawer was not located; mr-sim marks those calls "render" and nothing
    draws them. [CODE 800861f0, 8006c388] [GP0 analysis/s6/status/v2/gpu_c]
  - not a port error: the "SPECIAL MOVE" banner and icons (FUN_80049440)
    sat ~4 VBlanks ahead in the first side by sides. The oracle's
    `screenshot` at VBlank n shows the frame drawn from the RAM of n - 4
    (GP0: the icon x of the frame drawn at vbt + 1 is the RAM row's own,
    x = (+0x468 << 10) / 300 - 0x100); the side by sides take the port's
    replay at n - 4 (`--fx-shots` VB - 4) and then agree.
- **Far-kart impostor** FUN_8007D660 (Session 5): not an exhaust flame. The
  kart LOD byte +0x4B4 is (|kart - eye| >> 10)^2 / 0x1200 capped at 0x20
  (FUN_8002976C, recomputed each frame; the port's formula matches the
  oracle's byte for all six karts at VB 3650 / 4420); at 0x10 and above the
  game draws no model (FUN_80026F3C / FUN_800276E8 only below 0x10) but,
  from FUN_8007E21C (within 640000 of the eye on x and z), seven ball
  sprites (u 0xE2 v 0x40, 0x10 x 0x10, slot 0x15 CLUT 7; constants
  DAT_800B07B4..07B7): one per wheel (+0x171 of them: black, red 0xC6 0x20
  0x18 for vehicle 4), the body (vehicle colour table 0x800AC5E4 by +0x4B1
  times the depth-cue factors +0x414 / +0x418 / +0x41C, sized from the
  body rectangle and the view angle to the camera yaw), and the driver's
  two colours (table 0x800AC224, stride 0x14, by +0x4B2; at +0x330 and
  +0x324). The oracle's GP0 at VB 3650 draws exactly those seven sprites
  for karts 2 and 4 (colours (89, 49, 40) / (32, 32, 40) / (255, 150, 0)
  and (239, 247, 247) / (255, 178, 121) / (215, 215, 215) = the tables).
  mr-bevy draws it only with `faithful_draw_distance` (and hides the model);
  otherwise full models at every distance. [CODE 8007d660, 8002976c,
  8007e21c] [GP0]
- **Pickup shadows** (Session 5, mr-bevy pickupshadow.rs): the shadow list
  0x800C8B38 has 230 entries: karts 0..5 (FUN_8003E3A0 by +0x4D7) and the
  pickups at 6 + pickup slot, built once by FUN_8007B800 at race load: for
  item boxes (size 8), challenge objects (7, when shown) and tokens (4) at
  the pickup's x, z: the ground under it (FUN_8003DDB0, skipped when none),
  the ground matrix FUN_800252A4(FUN_8003CCBC(face normal)), corners table
  0x800AC168 (+-384) * size / 16 rotated (view units), tile slot 0x14 (0xA0,
  0) 32 x 32, intensity 0x80, tpage add 0x40 (B - F); the scale byte +0x29
  is the pickup's draw scale (FUN_8007BBF0). FUN_8002ADA8 draws every entry
  with count and scale non-zero, within 0x30000 of the eye, faded from view
  z 0x1000 to 0x1800, back faces culled (RotAverageNclip4). [CODE 8007b800,
  8002ada8, 8007bbf0] [RAM: 76 / 76 records exact vs the capture's list
  (44 boxes, 32 tokens), test `pickup_shadow_records_match_the_oracle`]
- **Flat quads 0x800C84B8** (FUN_800342B8; Session 5, mr-sim particles.rs
  `Flats`, mr-bevy `flat_prims`). Not an explosion core: 32 entries of 0x34
  bytes (position, four corners in view units +0xC.., uv +0x24.., tpage
  add +0x28, life +0x2A, slot +0x2C, CLUT +0x2D, scale +0x2E, flags +0x2F,
  growth +0x30, colour +0x31..), ring DAT_800B07EC, moved by FUN_80033B98
  (life -= dt, a quad whose life runs out is drawn once more with life
  0x8000; scale += growth * dt up to 0xFF). Two spawners, both in the race
  frame FUN_8007916C for each human kart:
  - FUN_800330D4: a splash ring on the water (x, +0x398, z), size
    +0x176 * +0x442 >> 12, when +0x4DB, or afloat (+0x4C8, or under the
    ground in a special move) and the random byte is < 0x20; flat (the
    default matrix 0x800A87B0 is the identity), slot 0x15 CLUT 0xD
    (0x60, 0xE0) 32 x 32, additive, scale 0x40 growing 2 per VBlank,
    life 0x20 with the last 16 VBlanks faded, split 2 x 2 with the quarter
    ring mirrored into each quarter. FUN_8008650C (the ambient list) is the
    other caller, off on DOCK.
  - FUN_80033440: per wheel a skid intensity (+0x3F + 0x2C * wheel, +5 per
    VBlank to 0x55 while sliding on the ground, -4 otherwise) and, when it
    is non-zero and the wheel moved at least +0x38 per VBlank, a one-frame
    B - F quad (slot 0x14 CLUT 0, 2 x 2 texels at (0xF, 0xF), colour
    intensity * 3 fading to black at its far edge) from the wheel's last
    position to its new one, as wide as +0x38: tyre streaks under a
    sliding kart, not persistent marks.
  [CODE 800342b8, 80033c44, 800330d4, 80033440, 80033b98, 8007916c] [RAM:
  `mr-particles analysis/s5/fx/race3d` teacher-forced per race frame
  (skids from the frame's record and last frame's intensities; splashes
  where the game spawned one, its random decision not replayed): 126,272 /
  126,272 slot-frames exact over 3,946 frames, 1,801 skid quads and 10
  splashes]
- Not drawn: the course ambient list 0x800D75D0 (DAT_800B0ACA, off on
  DOCK). The three 256 x 5 "sprites" at (256, 193) are not visible: each is
  drawn through a DrawEnv switch (drawing area 512..1023, offset 512) and
  copies a 15-bit band of VRAM rows 211 / 205 / 199 (x 768..1023) to rows
  193..197, at three places in the ordering table (a mid-frame texture
  swap); the code that queues them was not found. [GP0]

Sources in mr-bevy: `--replay-capture DIR` of an fx capture (everything
read from the captured RAM at the replayed VBlank, the HUD too), or the live
session: `Session::particles()` / `sparks()` / `flats()` (in the race
state; the item code spawns into them) and `Session::items()` written into
a scratch game-layout memory with the kart records, so both sources share
one conversion. Screenshots against the oracle: `mr-bevy --replay-capture
DIR --fx-shots OUT VB,VB,...` (window shots b_<VB>.png at the replayed
VBlanks) next to `tools/oracle/fx_capture.py --shots` / `fx_shots.py`
(o_<VB>.png), combined by `fx_sbs.py`; Session 5 images in analysis/s5/fx.

## Drawing: the PS1 effects pass (Session 5, mr-bevy psxfx.rs)

Everything above (particles, sparks, pickups, projectiles, status
billboards and shapes, impostors, flat quads, kart and pickup shadows) is
drawn by one pass after Bevy's main transparent pass, for the chase camera:

- **Blending in the game's colour space.** The PS1 blends the 15-bit
  framebuffer values themselves (gamma-encoded); Bevy blends linear light
  on an sRGB target. The pass copies the resolved frame to a non-sRGB
  texture of the same bytes, draws the primitives back to front by view
  depth (the ordering table's order) with fixed-function blend states that
  act on those bytes (ABR 0 = src alpha 0.5, ABR 1 = one + one, ABR 2 =
  reverse subtract, ABR 3 = one + one with F / 4), and writes the result
  back into every MSAA sample. Texels: modulation texel * rgb / 128
  clamped; 0x0000 transparent; on a semi-transparent primitive a texel with
  STP clear is drawn opaque. 15-bit store (Session 6): the pass marks the
  pixels it writes (alpha 0) and the composite truncates them to 5 bits
  per channel on the display lattice c * 255 / 31, as the GPU stores a
  blended / modulated result (`fx_blend = psx` only). Not modelled:
  dithering (the GPU's 4 x 4 offsets before the truncation move the mean
  by < 0.1 of a 5-bit step; at window resolution the pattern would be
  fine-grain noise); the main pass's scenery stays 8-bit.
- **Occlusion.** Each fragment is tested against the scene depth (the
  sprite's own depth; camera-facing quads are flat in view space). The
  game has no depth buffer: its ordering table draws the "ground" list
  first, so a sprite sitting in or on the ground is never cut by it, while
  near-list geometry (walls, docks, karts) in front of it covers it. The
  pass exempts an occluder that is an upward-facing surface below the
  sprite's centre (ground-flat sprites and shadows: + half their size) and
  within twice the sprite's size of its centre; any other nearer surface
  hides it. Status billboards keep their OT bias as a pull toward the eye
  (OTZ = view z / 4 assumed: the PSYQ default ZSF4 [INFER]).
- Options: `fx_blend = psx | linear` (linear: the same equations in
  linear light; covers the scenery and HUD primitives below too),
  `fx_depth_test = true | false` (effects only: scenery and HUD
  primitives are always depth-tested).
- **Scenery, kart models and the HUD (Session 5, finishing).** Every
  semi-transparent polygon goes through the same pass, not only effects:
  DAT scenery (DOCK: 877 layer-0 polygons, nearly all ABR 1 = additive:
  the water, glows; ABR 0 and 2 on a few), the far-pass layer-1 stand-ins
  (`faithful_draw_distance`: a second instance of the pass after the far
  camera's own main pass, `VIEW_FAR`), vehicle models (canopies etc.,
  15 MDLs have 1..6 such polygons) and the HUD's semi-transparent SPRT /
  FT4 (`VIEW_HUD`: the HUD camera's pass, orthographic depth). psxmesh
  keeps them out of the meshes (`psxmesh::semi_prims` -> `FxSemi`
  component, pushed every frame while the entity is visible, placed by its
  transform). Order: the ground list's polygons in a bucket drawn first,
  everything else (near-list scenery, kart polygons, effects) far to near
  by centre depth; depth-tested against the opaque scene (a 0.01-unit
  tolerance for coplanar decals). Back faces are culled as the game does
  (NCLIP: front = counter-clockwise in a y-up view, GPU order v0 v1 v2;
  DAT kind bit 1 / MDL flag bit 7 = two-sided): an additive back face would
  add twice (the first version, two-sided, made the water ~1.6x too
  bright). Scenery samples colour through the `texture_filter` look
  (bilinear + mips from VramTextures when smooth, in the game's space),
  STP and transparency from the raw texel. The HUD's semi primitives now
  honour STP and are depth-tested against the opaque HUD primitives, so
  the GPU's drawing order holds both ways.
  The frame clear is the game's: GP0 fill 0x025F1300 = rgb (0, 19, 95) on
  DOCK (the port had a made-up lavender; additive water over it was far too
  pale). Water region means (512x240, `--faithful-view`, race3e):
  VB 800 oracle 45/22/140, port 48/28/151 (before: 42/24/70; linear
  fallback with the new clear 52/24/113); VB 3350 oracle 42/22/139, port
  48/27/150 (before 40/24/68). Side by side:
  analysis/s5/semi/sbs_water.png (VB 800, 1350, 3350 with 2x crops);
  default look analysis/s5/semi/default/b_*.png. Residual: 8-bit vs
  15-bit, and the water's animated strips (0xA / 0xC; the decode
  alignment open above).
  Session 6 (H): three causes, two fixed. (1) The clear is a GP0 fill into
  the 15-bit framebuffer: (0, 19, 95) is stored (0, 2, 11) and shown
  (0, 16, 90) (`psxfx::psx_fill`); the port showed the 8-bit value. (2)
  The GPU truncates each result to 5 bits: now done in the pass (above).
  (3) Measurement: the oracle's VRAM screenshots expand 5 -> 8 bits as
  c << 3 (max 248), the port as c * 255 / 31 (the DAC's full scale), a
  3 % difference in the comparison itself; compare on the 5-bit scale.
  The animated strips were not a cause (next section). Open water means,
  5-bit scale (analysis/s6/water/means.py: mask b > 100, r < 70, g < 60
  at 512 x 240, `--faithful-view`, race3e): VB 800 oracle 4.59/3.36/16.34,
  before 5.29/3.47/17.55 (+7.4 % blue), now 4.97/2.90/16.65 (+1.9 %);
  VB 3350 oracle 4.86/2.95/16.96, before 5.29/3.34/17.99 (+6.1 %), now
  4.96/2.75/17.04 (+0.5 %). Side by side analysis/s6/water/sbs_water_q5.png.

- **Display timing (Session 5, finishing).** The effects are drawn at the
  karts' presentation state, not at the last tick: effects.rs keeps an
  `FxSource` (particles, sparks, flat quads, item state, kart records,
  frame counter, camera) per tick and interpolates between the same two
  endpoints as the karts at `Time<Fixed>`'s overstep: the last two ticks
  (classic), or the last tick and the coming one (`low_latency`: pacing.rs
  `predict` also takes the `FxSource` of its throw-away tick:
  `FxLookahead`). Whatever continues from one endpoint to the other (a
  particle / spark / flat in the same slot with its life running down, a
  projectile in flight, a kart and its status effect's points) is drawn
  at the interpolated position; counters, animation frames, colours and
  what spawns or dies come from the nearer endpoint. The simulation is
  only read. `MRM_FX_LAG=1` prints, per frame, the player kart's displayed
  position minus the effects' position: autopilot race to tick 900, mean /
  p99 / max world units, low_latency on: now 0 / 1 / 1, the old way
  (last real tick) 3137 / 6318 / 7804; off: 0 / 1 / 1 vs 1764 / 5533 /
  8129 (the effects were up to a tick behind, resp. ahead of, the karts).
  Visual: analysis/s5/fxinterp/exhaust_old_new.png (left old, right new;
  the exhaust flames at 0:11.9 sit at the pipes).

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
(`VramTextures::write_rect`, and the effects pass's own raw-colour pages,
`FxPages::refresh`). Timing (Session 5): the counter steps after the
frame's draw, so the strips are shown for the counter one race frame (dt)
earlier. Evidence (analysis/s5/fx/race3e, 26 `vram_peek` reads of strip
0x19's rectangle at the moment of each oracle screenshot): the uploaded
frame is the one of the RAM row's counter 16 times, one strip frame
earlier 9 times, two earlier once; the item-box stars in the side-by-sides
(VB 2431, 2510, 3010) match within one frame. Open: no read matches a
decoded frame exactly (best 112 of 896 halfwords differ), so the RAW strip
decode is slightly off (alignment) though the pictures match.
Closed (Session 6, H): the decode is exact. `vram_peek`'s hex holds each
halfword high byte first; read that way, all 26 reads of strip 0x19 in
race3e equal a decoded frame halfword for halfword (0 of 896 differ; the
"112" were the non-zero halfwords, columns 6 and 7, byte-swapped).
FUN_800311AC's pointer (DAT_800BBB20[slot] = chunk + 8 by FUN_800441D8:
w, h, then frame k at + 4 + k * w * h / 2) agrees with mr-assets.
