# SESSION 4: ITEMS AND AUDIO

## Goal of the whole effort (unchanged)
A 100% Rust + Bevy reimplementation of Muppet RaceMania that feels the same as
the original, using the original assets from the owner's disc. The psxrecomp
build stays the oracle until nothing depends on it.

## Where we are (read first)
docs/notes/SESSION-3-handoff.md (state, how to play, gaps), docs/SESSION-3.md,
docs/notes/SESSION-2-oracle.md (oracle harness: bridge v4, routes, captures,
lock, warm-up), docs/spec/ (kart-state, kart-physics + ledger with the revisit
list, ai, camera, hud, render-ledger, formats/*). The port: port/ (README).

## Owner decisions (2026-10-12)
- Items: the PLAYER stays tick-exact against the oracle with items on (pickups,
  firing, being hit, turbo, specials). AI item CHOICES only within a tolerance
  set from oracle races (as Session 3 did for driving).
- Audio: engine and sound effects in scope; in-race MUSIC is a very high
  priority for this title (streamed from the disc while racing).
- Session 3 felt very good to the owner; without items the AI interaction is
  thin, so items are what makes the comparison meaningful.
- AI personalities (owner idea, NOT in the original): drivers preferring the
  inside line, the outside line, alternating, the middle; aggressive/dirty vs
  clean. This is an enhancement, so it must be an option, OFF by default, with
  the faithful AI as the baseline. This session: a short design note
  (docs/notes/ai-personalities.md: how it would hook into the ported AI's
  racing-line target and item use without touching the faithful path) and, only
  if time remains, a small prototype behind a flag. Not a blocker.

## Done when
In the Bevy window the owner plays a full 3-lap DOCK race with items and sound:
1. Pickups on the track (placement, collection, respawn) and every item/weapon/
   special the 1-player DOCK race uses: firing, projectiles, hits and their
   effects, turbo meter and R1 turbo, R2 character specials, and the AI using
   them. Player tick-exact vs the oracle over recorded routes with items ON and
   nothing injected (the Session 3 harness injected AI hits; that goes away).
   AI item use within a tolerance from 5+ oracle races (what is used, when,
   how often, hit rate), set from the oracle's own spread.
2. HUD that comes with items: item slots, turbo meter, minimap, "LAP 2" banner,
   hit popups ("-1 HIT"), against the game's GPU packets as before.
3. Effects needed to read items: projectiles, explosions, star trail, sparks,
   flames (the particle list 0x800DF688 / FUN_80011E3C), pickup objects.
4. Audio: SPU sample banks decoded (VAG/ADPCM) from the disc; engine sound per
   kart (pitch from the ported engine state); sound effects at the game's own
   trigger points (the skipped sound calls in mr-sim, FUN_8007A620 /
   FUN_8005EECC and friends); the race music streamed from the disc (XA ADPCM
   or whatever the course uses: find it from the recomp's cd_read_log / XA
   reads during a race). Verified against the oracle where possible (SPU voice
   state / key-on events over TCP: spu_voices, spu_events, audio_wav).
   Needs an audio output crate for Bevy (bevy_audio or a lower-level one):
   that is a new dependency -> owner approval + BINLIST.
5. Comparison report + the owner races with items and sound in both.
Carry-overs if cheap: animated textures (RAW type 0x22, e.g. water), results
"YOU BEAT x" messages.

## Rules
As SESSION-2/3: oracle is the truth; constants from code/data or fitted to
oracle traces; fixed point in the sim; generated C is the spec, never copied;
no game data committed; new crates need approval and a BINLIST line; parallel
agents for independent tracks (items sim, item HUD/effects, audio, music) with
the oracle shared through capture.py's lock; ledgers updated; commits local.

## Stop and ask the owner when
A tolerance cannot be met after three attempts; a crate is needed (audio);
the recomp needs a change; the AI-item tolerance numbers look wrong.
