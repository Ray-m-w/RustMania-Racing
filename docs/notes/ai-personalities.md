# AI personalities: design note (Session 4, owner idea; NOT in the original)

Status: design only. An optional enhancement, OFF by default. The faithful AI
(docs/spec/ai.md, port/crates/mr-sim/src/ai.rs) stays the baseline and the
thing every oracle tolerance is measured on.

## What the owner asked for
Drivers with a style: preferring the inside line, the outside line, the
middle, or alternating; and aggressive/dirty vs clean in contact and item use.

## Where the faithful AI decides (the only two hook points)
1. **Line**: FUN_80090D00 aims at a TER path point a few points ahead, moved
   across the racing line by `off = -2 * k.lateral` (+0x3FC, the kart's
   lateral offset from the path point; only when +0x500 == 0 and the look-ahead
   n > 2). That mirror is what pulls the kart back onto the line. The track's
   half-widths at the kart are +0x3F4 / +0x3F8 (FUN_8009004C), the curvature
   ahead is +0x4A0 (sum of heading steps walked).
2. **Buttons**: FUN_80090914 returns the pad word: steer, CROSS, CIRCLE
   brake-steer, SQUARE jump, R1 turbo / special, and FUN_80090D00 adds L1/L2
   weapons with `rand() < 0x400` per tick.

A personality changes the aim point's lateral target in (1) and filters the
pad word in (2). Nothing else: the kart update, contacts, items and physics are
the same code for every kart, so a personality kart still drives the game's
car with the game's handling.

## Design
```
pub struct Personality { line: LineStyle, conduct: Conduct, strength: u8 /*0..16*/ }
enum LineStyle { Faithful, Inside, Outside, Middle, Alternate { period_segments: u8 } }
enum Conduct   { Faithful, Clean, Aggressive }
SessionConfig.personalities: [Option<Personality>; 6]   // all None by default
```
- `ai.rs` gets two calls, both behind `if let Some(p) = k_personality`:
  `off = personality::line_offset(p, k, off, walk.turn)` right after the
  faithful `off` is computed, and `b = personality::filter_buttons(p, k, b, ctx)`
  before FUN_80090D00 returns. With `None` the faithful code runs unchanged,
  instruction for instruction: no extra branches inside the arithmetic, no
  extra state in the kart record (the personality state lives in a side table
  in `Session`, not in the 0x504-byte record that is compared with the oracle).
- **No shared randomness.** Personality choices never call BIOS rand() or the
  game's byte pointer 0x800B0BCC: those streams are shared with the camera,
  contacts, race load and the faithful AI. A personality has its own small LCG
  seeded from the session seed + slot. (A personality kart still changes the
  race for everyone by driving differently; that is the point. But a session
  with personalities off is bit-identical to today's.)
- **Line styles** (target lateral offset `L`, in the same units as +0x3FC,
  clamped inside the half-width minus a margin):
  - Inside: `L = sign(turn) * w * strength/16`, where `turn` = +0x4A0 (the
    curvature ahead) and `w` the half-width on that side; on straights
    (|turn| < 0x180, the faithful AI's own straight test) L eases to 0.
  - Outside: the opposite sign.
  - Middle: L = the track centre's offset from the path point
    ((+0x3F8 - +0x3F4) / 2), i.e. ignore the racing line.
  - Alternate: switch Inside/Outside every `period_segments` path segments
    (+0x498 changes) or once per lap.
  The faithful mirror becomes `off = -2 * (k.lateral - L)`: same controller,
  different set point, so the steering stays as stable as the game's.
- **Conduct**:
  - Clean: drop L1/L2 when the nearest kart (+0x4DC) is within a cone behind
    (no shots at someone you are passing); bias L away from a close kart
    (+0x501) to avoid contact; never R1 into a kart's back.
  - Aggressive / dirty: hold weapons until a kart is in the firing cone ahead
    (instead of the faithful 1/32-per-tick roll), bias L toward a kart
    alongside (bump), fire R2 specials when a kart is close.
  Cones and ranges come from the item code track A ports (projectile speed
  and lifetime), not invented numbers.
- **Options**: mr-bevy `--ai-personalities` (random per opponent from the
  session's own LCG) and `--ai-personality SLOT=inside,aggressive` for tests;
  later a settings entry. Default: off. The results/HUD are unchanged.

## Verification plan
- With personalities off: the existing oracle tests (race load + 372 frames
  exact, the AI tolerance races, the player's items-on routes) must stay
  bit-identical — a CI-style test runs one race with `[None; 6]` and compares
  the session hash with the stored baseline.
- With a style on: measure what it claims (mean lateral offset vs curvature
  sign over a lap, hit rate and shots per lap), and that lap times stay within
  a few % of the faithful AI so the field stays competitive.

## Cost
About 150 lines in a new `mr-sim/src/personality.rs`, two guarded call sites
in ai.rs, a side table in Session and two CLI flags. A prototype is only worth
building after track A lands items (Conduct depends on the weapon code).
