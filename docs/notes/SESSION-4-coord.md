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
### D SFX + engine
