# Session 4: coordination between the parallel tracks

Brief: docs/SESSION-4.md. Lead: the main session. Four agents work in the SAME
working tree at the same time. This file is the contract between them; each
agent appends to its own section ("Posted APIs") when it publishes something
another track uses. Append only; never rewrite another track's section.

## Ownership (edit only what you own; small hook edits elsewhere are listed)
| track | owns | may add one-line hooks in |
|---|---|---|
| A items sim | mr-sim: new item modules (pickups, turbo, weapons, projectiles, specials, status effects), update.rs, progress.rs, collide.rs, ai.rs, race.rs, session.rs, harness.rs, oracle.rs, bin/mr-verify; mr-sim tests; tools/oracle (item routes, capture tooling); docs/spec/items.md; kart-physics-ledger.md / ai.md item sections | (none; it is the owner of the sim core) |
| B item HUD + effects | mr-bevy: hud.rs and new files (minimap, item HUD, banners, popups, effects/particles, pickup + projectile drawing); mr-assets hud.rs; mr-sim/src/particles.rs (the particle list 0x800DF688 / FUN_80011E3C, its spawners); docs/spec/hud.md, render-ledger.md, docs/spec/effects.md | mr-sim spawn sites (ask A through this file first, or post the exact call you need A to place); mr-bevy main.rs plugin registration |
| C music + audio output | mr-audio: xa.rs (CD-XA, self-contained); mr-disc (raw 2352-byte sector reads if needed); mr-bevy: audio.rs (device output via bevy_audio, the mixer bus that SFX and music feed), mr-bevy Cargo.toml (bevy_audio feature); BINLIST.toml audio entry; docs/spec/audio.md (music + output sections) | mr-bevy main.rs plugin registration |
| D SFX + engine | mr-audio: adpcm.rs (SPU ADPCM blocks), vab.rs, spu.rs; mr-sim/src/sound.rs (sound-event queue: ports of the game's sound calls' argument logic, engine-sound state); mr-bevy: sfx.rs (drains sim sound events into the SPU model, feeds C's bus); docs/spec/audio.md (SFX/engine sections) | mr-sim call sites of the skipped sound calls (FUN_8007A620, FUN_8005EECC, ...): one-line `sound::...` calls; coordinate with A here since A owns update.rs |

mr-bevy/src/main.rs is shared: only add a `mod x;` and an `add_plugins(...)` /
system registration line; keep your logic in your own file.

## Mechanics
- Cargo target dirs (avoid lock contention): A = port/target (default),
  B = port/target-b, C = port/target-c, D = port/target-d. Set
  `CARGO_TARGET_DIR` for every cargo command. Build offline (`--offline`).
- Commits: commit only your own paths (`git add <paths>`, never `-A` / `.`),
  with
  `git -c user.name=Yev3n -c user.email=253437350+Ray-m-w@users.noreply.github.com commit -m "..."`
  and the trailer line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  If `.git/index.lock` exists, wait a few seconds and retry. Do not commit a
  tree that breaks `cargo build --workspace` (if another track broke it, say so
  here and commit only your crate after `cargo build -p <yours>`).
- Oracle: ONE psxrecomp debug build is running (TCP 4370, bridge v4, savestate
  slot 2 = DOCK grid). Any run that loads a savestate or needs a known race
  point holds `capture.OracleLock` (tools/oracle/capture.py) for its whole
  duration. Never save over slots 1 or 2. Do not restart or stop the oracle; if
  it dies, post here and tell the lead. Read-only TCP queries without a load
  may run without the lock but expect another agent's run to be in progress.
- No game data committed (decoded audio, captures, images stay in analysis/,
  which is gitignored). Constants from the exe/disc at run time or from the
  oracle, each with its address.
- Log your track's work in docs/notes/2026-10-12.md under a heading
  "Session 4, <track>" (append only; date the entries).
- Stop and report to the lead (finish your run with the question) when: a
  tolerance cannot be met after three attempts; the recomp would need a
  change; you need a new crate (only bevy_audio is approved, track C).

## Posted APIs
### A items sim
(2026-10-12, A, draft: names fixed, fields may grow) Item state lives in
`mr_sim::items` and rides in `RaceGlobals::items` (so the kart update can
fire / the race frame can move it); a Session exposes it read-only:
```rust
let it: &mr_sim::items::ItemState = session.items();
// Pickup objects 0x800E1B70 (FUN_8007B448 init, FUN_8007BBF0 per frame),
// 192 slots: item boxes 0..0x30 (DOCK: 44), challenge objects 0x30..0x40
// (hidden in a race), turbo tokens 0x40.. (DOCK: 92).
for p in it.pickups.iter().filter(|p| p.state != 0) {
    p.kind;            // PickupKind::{Box, Challenge, Token}
    p.pos();           // [x, y, z] in kart/world units (the table keeps x, z in TER units: pos() shifts them)
    p.state;           // 1 present, 3 collected (shrinking), 2 gone (respawn timer), 4 growing back
    p.scale;           // 0..=0x80 draw scale (the game's render byte 0x800C8B61 + 0x30 * (i + 6))
    p.token_colour;    // tokens: 0..3 (+0x1F), the colour / sparkle set
}
// Projectiles 0x800DDC50 (FUN_80068C6C fires, FUN_80069648 moves / hits), 4 slots.
for pr in it.projectiles.iter().filter(|p| p.active) {
    pr.pos;            // [x, y, z] world units
    pr.vel;            // per VBlank
    pr.flags;          // weapon flags: 0x1D, 0x280, 0x588, 0x78 for weapon types 0..3
    pr.owner;          // kart slot
    pr.spin;           // 12-bit draw angle (+0x26 of the slot)
}
// What happened this tick (cleared at the start of each Session::tick):
for e in &it.events { match e {
    ItemEvent::Pickup { kart, index, kind } => {}
    ItemEvent::Fired { kart, slot, weapon, projectile } => {}
    ItemEvent::Hit { victim, shooter, projectile } => {}   // "-1 HIT" popup
    ItemEvent::Explode { projectile, pos } => {}
    ItemEvent::Special { kart, kind } => {}                // R2 character special (kind = character's type 0..24)
    ItemEvent::SpecialMove { kart } => {}                  // full-meter R1 (+0x4D6)
} }
```
Per kart (`KartState`, record offsets): `weapons[0..6]` (+0x4D0 L1 cur/next
type, +0x4D2 L2 cur/next, +0x4D4/+0x4D5 charge, 0x40 = ready, 0xFF =
empty), `start_timer` = turbo meter +0x48A (0..=0x1A4 full; alias
`turbo_meter()`), `boosting` +0x4C4, `special` +0x4D6 (special move state
1..3), `effect` +0x4F1 (-1 none, else the effect type a special put on this
kart), `status` +0x354, `homing_timer` +0x468 (effect clock, 0..300 VBlanks).
B: particle spawns in the item code (FUN_800870E8 / FUN_800326B4 /
FUN_800861F0 calls) keep their random-byte draws; post the spawner signature
you want and I will place one-line calls. D: the item sound calls are
FUN_8007A620 (3D: id, pitch, vol, x, y, z) and FUN_8005EECC (2D); post the
`sound::` call you want and I place it, or I will emit `ItemEvent::Sound`.
### B item HUD + effects
### C music + audio output
(2026-10-12, C) The output bus, in mr-bevy/src/audio.rs (`AudioOutPlugin`,
registered in main.rs by C). One rodio stream at 44,100 Hz stereo; rodio/cpal
convert to the device rate. Track D feeds the SPU voice mix into it:

```rust
use crate::audio::{AudioBus, Bus};
// Resource, inserted by AudioOutPlugin (also with --no-audio: then pushes are
// dropped and sfx_wanted() returns 0). Clone is a cheap Arc handle.
fn feed(bus: Res<AudioBus>, ...) {
    let n = bus.sfx_wanted();          // frames to render now (keeps ~50 ms queued)
    let frames: Vec<[i16; 2]> = spu.render(n); // 44.1 kHz [L, R]
    bus.push_sfx(&frames);             // non-blocking (short mutex), never stalls the device
}
bus.sfx_queued();                      // frames buffered, not yet played
bus.played_frames();                   // device clock (frames consumed), u64
bus.set_volume(Bus::Sfx, 1.0);         // Bus::{Master, Music, Sfx}, linear gain
```
- Push the SPU **voice sum before the SPU main volume** (voices x their own
  volumes, x the SsSetVoice/master-of-voice levels the game sets, clamped
  to i16). The bus applies the SPU main volume the game programs
  (0x3FFF/0x8000 L/R, read from the oracle's spu_status in a race) to the sum
  of voices + CD (music) like the SPU does, then the user volumes.
- Pacing: render `sfx_wanted()` frames per Update (target latency 2,048
  frames, 46 ms; the queue caps at 0.5 s and drops the oldest beyond it). An
  underrun plays silence for the missing frames; it never blocks.
- Music is C's (XA from TUNES/*.XA, streamed on its own thread) and enters
  the same mix as the SPU's CD input: XA decode -> CdMix (game 0x800B0A50)
  -> SPU CD volume (0x7FFE) -> + voices -> main volume.
- (2026-10-12, C, later) Character voice clips (VOICES/*.XA through the
  game's XA queue; they interrupt the music, which restarts after them):
  `fn x(mut dir: ResMut<crate::audio::MusicDirector>) { dir.clip(character, n); }`
  = FUN_80060580(character = kart +0x4B2, n). The player's finish clip is
  already wired in audio.rs from `RacePhase::Finished`. Other call sites of
  FUN_80060580 / FUN_800606D0 (hits, items: FUN_80081DD8, FUN_8006EC4C,
  FUN_80074A94, FUN_8007916C) are A's to surface as sim events; post the
  event and C (or D in sfx.rs) forwards it to `clip`.
### D SFX + engine
(2026-10-12, D) Sound calls are events in `mr_sim::sound` (committed 071372d).
`RaceGlobals::sound: SoundState` carries the listener (camera eye) and the
per-frame call lists; the front end drains them:
```rust
for f in session.globals.sound.drain() {   // Vec<SoundFrame { dt, calls }>
    for c in &f.calls { /* SoundCall::Play {id,pitch,vol_l,vol_r} | Loop {owner,id,pitch,vol,glide} */ }
}
```
The driver that plays them is `mr_audio::driver::Driver` (game sound driver +
libsnd + `mr_audio::spu::Spu`); mr-bevy/src/sfx.rs (`SfxPlugin`) does that
and feeds C's `AudioBus::push_sfx` (voice sum, `spu.apply_main = false`).
Unless drained, the last 256 frames are kept (bounded).

Hooks I placed (one line each, re-read before editing; all in HEAD now):
- session.rs `frame()`: `crate::sound::begin_frame(g, ks, dt, self.camera.pos);`
  first line (new call list, listener, and FUN_800306D0's clear of every
  kart's voice-request bits +0x4DD); `crate::sound::character_voices(ks, g, w);`
  after the items block, before the record write-back (FUN_8007A7F4).
- update.rs: engine (`crate::sound::engine(k, g, w)` replaces the engine's
  `g.rng += 1`: it draws that byte itself), SELECT click `view_toggle`,
  `not_ready` (plays sound 9 when `g.sound_event != 0`; keep it right before
  "0x80065E00"), `jump` (end of take-off), `skid` (in the skid branch),
  `impact` (replaces its `g.rng += 1`), `slide` (after the 0x4B rumble).
- ground.rs: `wall_hit` after the wall rumble, `splash` (replaces the splash
  `g.rng += 1`; condition is now `sound_check && float_state != 0`, the
  function draws only when +0x4DB == 0, as before), `landing`.
- collide.rs: `kart_land` (after obj_e5 = 1), `kart_bump` (after the rumbles).
- race.rs frame_clock: `crate::sound::countdown(g);` before `g.countdown -= g.dt`.
- progress.rs lap_completed: `crate::sound::lap(k, g, w);` after `k.lap += 1`.
A: please keep these lines when you rework those blocks.

Item sound calls for A to place in the item code (exact game args; use
`crate::sound::at(g, w, id, pitch, vol, [x, y, z], -1)` for FUN_8007A620 with
owner -1, `crate::sound::play(g, id, pitch, vol_l, vol_r)` for FUN_8005EECC):
- FUN_80061FC0 pickup touch: `at(0x1E, 0x1600, 0x60, kart pos)` when +0x4B5 == 0.
- FUN_80068C6C fire: `at(0x14, 0x1600, 0x80, slot pos)`, `at(weapon sound, 0x1600, 0x60, ...)`.
- FUN_80069334: `at(0x14, 0x1600, 0x80, pos)`. FUN_80069648 projectiles:
  0x14 / 0x15 (0x80), 0xB (0xA0), 0x21 (0x80), 10 (0x80), weapon sound (0x80).
- FUN_8006B828 special: `play(id, 0x1500, 0xA0, 0xA0)`, `play(0x24, 5000, 0x80, 0x80)`.
- FUN_8006C388 effects: `play(0x57, 0x1400, 0xA0, 0xA0)`, `at(id, pitch, 0xA0, kart)`,
  `at(0x18, 0x1538, 0x70, kart)`, `play(0x23, 0x157C, 0x80, 0x80)`.
- FUN_8007BBF0 pickups / tokens: `at(0x16, 0x1600, 0x70, pos)`, `at(0x18, 0x1538, 0x70, pos)`,
  `play(0x24, 5000, 0x80, 0x80)`. FUN_8008650C: `at(id, 0x157C, 0x40, pos)`.
- FUN_8006EC4C (race flag 0x40): `at(0xB, 0x1600, 0xA0, ...)`, `at(0x14, 0x1600, 0x80, ...)`.
Order matters only within a frame (call order = key-on order), so place them
where the game calls them.
