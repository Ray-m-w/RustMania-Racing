# Audio

Sections: race music (CD-XA) and output: track C, Session 4. SFX, sample
banks and engine: track D (append below their heading; each track edits only
its own sections).

Tags as elsewhere: [CODE addr] from the generated C / Ghidra decomp, [ORACLE]
measured on the psxrecomp build over TCP, [DISC] read from the owner's disc,
[INFER] inference.

## Music: what plays and when (track C)

### The XA player (FUN_800602C4) [CODE 800602c4, 80060160, 8005fb4c]
- A track id `n` names a CD-XA channel. `n < 0x20`: XA file table row
  `n >> 3` (TUNES/MUSIC1..4.XA), channel `n & 7`, CdMix level = music
  (0x800B0A50). `n >= 0x20`: row `((n - 0x20) >> 4) + 4` (VOICES/*.XA),
  channel `(n - 0x20) & 0xF`, CdMix level = voices (0x800B0E70).
- XA file table 0x800ABF78, 12-byte rows {name pointer, first sector, end
  sector}; the exe holds the names, CdSearchFile fills the sectors at boot
  [CODE, ORACLE read_ram]. Rows: 0-3 TUNES/MUSIC1-4, 4-10 VOICES/CHAR_01-07,
  11-13 FILM_01-03, 14 CRDIT_01, 15 WIN.
- Start: CdlSetmode 0xE8 (double speed, XA-ADPCM on, whole sector, filter),
  CdMix {L->L = R->R = level}, CdlDemute, SsSetSerialVol(0, 0x7F, 0x7F),
  CdlSetfilter(file 1, channel), CdlSetloc(first sector), CdlReadS. The drive
  reads every sector of the interleaved file; only the filtered channel's
  audio sectors reach the decoder.
- End of a channel: a data sector (submode 0x48) whose payload starts with
  u16 0x0160 and whose next u16 carries the channel in bits 10..14 (8 of them
  follow each channel's EOF audio sector) [DISC]. The read callback
  FUN_80060160 (CdGetSector of the first 8 words) then: loop flag
  0x800B0758 = 1 -> Setloc + ReadS from the first sector again; else CdMix 0,
  CdlPause, SsSetSerialVol 0, playing bit 0x800B0760.0 cleared.
- The loop is seamless in the oracle: the track's last frame is followed by
  its frame 0 with no gap [ORACLE cd_in tap, race VB ~6095].

### The queue [CODE 8005fb4c, 800604c8, 80060580, 800606d0, 8008c4ac]
0x800B0740 = current entry, 0x800B0744/46/48 = three pending entries.
Bit 0x8000 = one-shot (voice clip, no loop), 0xC000 = the course intro clip.
- FUN_8005FB4C (every frame): the head starts when nothing plays or the
  current entry loops; a playing clip is never interrupted. Entries shift.
- FUN_80060580(character, n) (clips: the finish, being hit, items ...):
  queues `0x800AC230[character * 0x14] + n | 0x8000` after the last pending
  clip, and the course track behind it -> after every clip the course track
  starts again from its first sector [ORACLE: finish VB 7043..7320].
  0x800AC22E bit 0 must be set (voice on). Three clips pending = dropped.
- Race load (FUN_8008C4AC): in mode 0 (from the menus) a clip in progress is
  dropped; the course intro clip (course row +4, + row +0xA bit 0 which
  toggles; skipped once row +0xA bit 7 is set, i.e. once per course per
  power-on) is queued, then the course track unless that track is already
  the looping current one (a restart keeps the music playing) [CODE, ORACLE].
- The player's finish (FUN_80082708, human players only): clip + 3 for first
  place, + 2 otherwise (Kermit, base 0x50: 0x52 seen in the oracle).
- Pause (START in race): FUN_8005F9AC (CdMix 0, CdlPause); unpause
  FUN_8005FAA4 (CdMix back, ReadS) [CODE, ORACLE VB 10802].

### Course table 0x800ACB34 (12-byte rows, index = 0x800B0FAC) [CODE]
+3 music track (i8, -1 none), +4 intro clip base (i16), +0xA intro toggle /
played bits. DOCK = course 3: music **16 = TUNES/MUSIC3.XA channel 0**
(2,251 audio sectors, 120.05 s, end marker 18,008 sectors into the file),
intro clip 0xB0/0xB1 = VOICES/FILM_03.XA channel 0/1 [DISC, ORACLE].
All 32 music channels (mr-xa info): 58 s to 150 s each.

### DOCK in the oracle (tools/audio/music_probe.py, analysis/audio/probe1)
| VB (from savestate) | event |
|---|---|
| slot 1 load, VB 1-70 | intro clip 0xC0B0 (FILM_03 ch 0) playing |
| VB 72 | track 16 from LBA 148321 (MUSIC3.XA start), loop flag 1; race mode 1 at VB 157 |
| slot 2 grid .. | track 16 continues through countdown, GO and the race |
| ~VB 6095 | end marker -> seamless loop to LBA 148321 |
| VB 7043 (finish) | queue 0x8052, 0x0010; VB 7046 Kermit clip 0x52 (CHAR_04 ch 2, 41 sectors) |
| VB 7314-7320 | clip ends; track 16 restarts from its first sector |
| results, START (mode 0), CROSS -> race again (mode 1) | music keeps playing, not restarted |
| START in the race (VB 10802) | paused |

### Levels [CODE 8004321c, 8008f448, 8005f71c] [ORACLE spu_status]
- Music level option: table 0x800AC118 (16 x i16, 0..128 step 9), voices
  0x800AC138 (0..120 step 8); default index 12/12 (FUN_8008F448 sets
  0x800E0390 = 0x0C0C) -> CdMix 0x6C music, 0x60 voices.
- SPU in the race: CD input volume 0x7FFE (L/R), main volume 0x3FFF, reverb
  off. SPU mix: (voices + CD x CD volume >> 15), clamp16, x main >> 15.
- [ORACLE, unexplained] the race-load intro clip plays at 0x6C, not 0x60
  (gain exactly 108/96 vs the decode); some CdMix(music) during the load.
  The finish clip plays at 0x60 (sample-exact).

## CD-XA decoding (mr_audio::xa) [DISC, ORACLE]
- Raw 2352-byte sectors (mr_disc::Disc::read_raw); subheader at 16..20,
  payload 18 sound groups x 128 bytes at 24. Music: coding 0x01 (stereo,
  37,800 Hz, 4-bit), 8 channels interleaved one sector each (not strictly
  in order near the ends: demux by the subheader channel). Voices: coding
  0x00 (mono 37,800 Hz), 16 channels.
- ADPCM: per sound unit, range = param & 0xF (13..15 act as 9), filter =
  (param >> 4) & 3, sample = (s4 << 12 >> range) + ((k0*s1 + k1*s2 + 32)
  >> 6), clamp16, k0/k1 = 0/0, 60/0, 115/-52, 98/-55. History persists
  across sectors (and across the loop).
- To 44,100 Hz: the oracle's resampler (psxrecomp cdrom.c): linear, per
  sector, phase restarted at 0, the last frame held; 2,016 -> 2,352 frames.
  Then CdMix (x level >> 7, clamp), then SPU CD volume.
- Verification (mr-xa compare/segments vs the cd_in tap, which is the
  decoder output after CdMix): DOCK race from the grid savestate, 80.2 s:
  3,537,408 of 3,537,408 frames after the restore sector **sample-exact**;
  the next 80.0 s: the track end, the seamless loop and the restart after
  the finish clip all exact, 3,292,800 / 3,528,000 frames matched by the
  music decode and the remaining 235,200 by the clip / its silence (clip
  0x52: 192,864 frames exact at level 0x60); race load: 914,928 frames
  exact after the intro clip. Max |diff| 0 on every matched run.

## Output (mr-bevy audio.rs)
- bevy_audio (rodio 0.22 / cpal 0.17) plays one endless 44.1 kHz stereo
  source (`MixerSource`); rodio converts to the device rate.
- Music thread `mr-music`: owns its own disc handle, streams 32 raw sectors
  per read, decodes ~0.4 s ahead into a ring (never the whole file);
  commands Play / Pause / Resume / Stop; a Play drops the old track's
  decoded-ahead audio.
- Mixer: per 256-frame chunk, SFX ring (track D, SPU voice sum) + music
  ring, clamp16, x 0x3FFF/0x8000, x user gains. Underruns are silence.
- Director (`MusicDirector`): runs the XaQueue each frame from the mr-sim
  session: race load (first: intro clip then track; restarts: nothing new),
  the player's finish clip; `clip(character, n)` for other clip sites.
- Options: --no-audio, --no-music, --volume / --music-volume /
  --sfx-volume V, --audio-stats, --audio-dump PATH.
- Measured (mr-bevy --autopilot --audio-stats --audio-dump, 160-250 s
  runs): music underruns 64 frames total (the first chunk at the intro's
  start), none after; the dumped mix correlates 1.0000 with the mr-xa decode
  at gain 0.4999 (= main volume 0x3FFF): intro clip 0xB0, track 16 from
  frame 0, the seamless loop at 120.05 s; a full race (tick 4358, 4th
  place) queued the finish clip 0x8052, then track 16 again.

## SFX, sample banks and engine (track D)

Code: `mr_audio::{adpcm, vab, spu, driver}`, `mr_sim::sound`, mr-bevy `sfx.rs`.
Verification: `tools/oracle/capture_sfx.py ROUTE OUT` + `tools/oracle/sfx_events.py OUT`,
then `cargo run --release -p mr-audio --example sfx_verify -- OUT --all-karts`.
Listening test: `cargo run --release -p mr-audio --example sfx_render -- analysis/audio/x.wav`.

### The bank: SFX/SFX_6.VH + .VB [DISC] [CODE 8004321c, 8005ea94, 8009e110]
- The only sample bank on the disc (no course has its own; the race loads
  nothing else into SPU RAM). Loaded once by FUN_8005EA94("sfx\sfx_6"): VH
  (5,664 bytes) to RAM 0x800D21D0 and SsVabOpenHead; VB (513,504 bytes) by
  SsVabTransBody to SpuMalloc's address **0x1010** (0x800E37E8).
- VAB version 6: 6 programs with tones (16, 16, 16, 16, 16, 8 = 88 VagAtr),
  88 VAGs, sizes in 8-byte units; ProgAtr mvol 0x7F, mpan 0x40.
- The game always plays program 0: sound id n (1..88) is tone n-1 of
  program 0; libsnd's lookup `tone + 16 * program index` runs on through
  the later programs' VagAtr blocks, so id n = VagAtr n-1 (= VAG n here).
- [ORACLE spu_ram] SPU RAM 0x1010..0x7E5F0 equals the VB byte for byte in a
  race (0 mismatches); below 0x1010 only the SPU's capture buffers.

### SPU ADPCM (adpcm.rs) [psx-spx]
16-byte blocks: shift/filter byte, flag byte (1 end, 2 repeat, 4 loop
start), 28 nibbles: `(nibble << 12) >> shift` (shift > 12 acts as 9) +
`(old * f0 + older * f1 + 32) >> 6`, filters (0,0) (60,0) (115,-52)
(98,-55) (122,-60), clamped. Every VAG of the bank decodes to its END block.

### The SPU model (spu.rs)
24 voices: start / repeat address (repeat latched from loop-start blocks),
pitch counter (0x1000 = 44.1 kHz, capped 0x4000), 4-point interpolation,
ADSR per psx-spx (attack lin/exp, decay exp, sustain, release; wait cycles
and step from shift / step), fixed L/R volumes (bits 0-14 = volume / 2),
END without REPEAT = voice off. 44.1 kHz stereo; `apply_main = false` gives
the voice sum (C's bus applies the SPU main volume to voices + CD).
Not modelled (ledger section 2): reverb (off in a race: SPUCNT 0xC001,
reverb master clear), noise and pitch modulation (NON = PMON = 0), volume
sweeps (libsnd writes fixed volumes), SPU IRQ, capture buffers; the
interpolation uses a Gaussian kernel of the hardware's shape (sigma 0.57,
taps summing to 0x7F80) instead of the SPU ROM table, which is neither in
the executable nor on the disc.

### The game's sound driver (driver.rs) [CODE]
Two entry points, both through libsnd's utility calls on VAB 0:
- **FUN_8005EECC(id, pitch, vol_l, vol_r)**, a one-shot: SsUtKeyOn(prog =
  (id-1)/128, tone = (id-1)&0x7F, note = pitch/128, fine = pitch&0x7F,
  vol * master >> 7 capped at 160); master = 0x800B0E70 (the options'
  "voices" level, 0x60 by default, FUN_8005F71C; also the XA voice CdMix
  level). Then FUN_8005ED54.
- **FUN_8005F168(owner, id, pitch, vol, glide)**, a looping channel: 8
  channels at 0x800D8AB0 (0x18 bytes: owner, sound, cur / target / from
  pitch, glide, fade, timer, voice, vol L / R). The owner's channel, else
  the last free one; a new sound on a live channel silences the old one
  first. Volume clamped 0..150 and stored, timer = 6. No voice: SsUtKeyOn
  as above but with the master applied twice. Live voice:
  SsUtSetDetVVol((vol * master >> 7) << 6) and **SpuSetVoicePitch(voice,
  cur >> 2)** at once: the engine's per-frame pitch is the raw register,
  unlike the key-on's note / fine. The glide (from -> target over `glide`
  ticks) is never used in a race (glide 0).
- **FUN_8005EFD8**, the libsnd tick callback (SsSetTickMode(1): every
  VBlank): per live channel, timer <= 0 -> SsUtKeyOnV(voice, prog 0, tone
  0, note 34, volume 0) (a silent re-key: tone 0 sustains at volume 0 and
  the voice stays busy; 77 of a race's 729 key-ons are these) and voice =
  -1; else the fade steps (inert: fade starts at 0, the timer never
  exceeds 12) and timer - 1. A channel stops 6 VBlanks after its last
  refresh. Then SsSeqCalledTbyT -> _SsVmFlush.
- **FUN_8005ED54(voice, except)**: a key-on that took a voice from a looping
  channel re-keys that channel (note / fine from its current pitch) on a
  new voice, repeatedly.

libsnd [CODE 8009b3f0, 8009b890, 8009be30, 8009c0a0, 8009cb14, 8009d2b0, 8009c280]:
- SsUtKeyOn: pan from vol_l / vol_r (equal: 64). _SsVmAlloc: a voice not
  keyed with read-back envelope 0, else among priorities <= the tone's the
  lowest priority, ties to the lowest envelope, then the oldest (every
  allocation ages all voices); none: fail (-1).
- Pitch: SsPitchFromNote(note, fine, tone centre, tone shift) from the exe
  tables 0x800AF1C8 (12 semitones) x 0x800AF1E0 (128 fine steps) >> 16,
  octave shift with rounding; 0x3FFF from two octaves above the centre.
- Volume (_SsVmKeyOnNow): (s8)vol x bank mvol x 0x3FFF / 0x3F01 x prog mvol
  x tone vol / 0x3F01 (32-bit MIPS arithmetic; the key volume is read as a
  signed byte), then tone pan, program pan, key pan (< 64 scales the right
  by pan / 63, else the left by (127 - pan) / 63). Address from
  SsVabOpenHead's VAG table, ADSR = tone adsr1 / adsr2 (+ release adjust
  0x800C5460 = 0).
- The shadow 0x800D8B70 (dirty flags 0x800C1E50) and the key masks reach
  the SPU at the next VBlank's flush: envelopes read back (a voice is
  released after 15 flushes at envelope 0), attributes written, key-off,
  key-on. [ORACLE] key-ons land 2 VBlanks after the start of the frame
  that called (tick k: VBlank 2k + 2), 98.8% of matched events exactly.

### Trigger points (mr_sim::sound, called from the race code)
Most calls go through **FUN_8007A620(id, pitch, vol, x, y, z, owner)**: the
squared distance (1024-unit steps) to the nearest human player's camera
eye (camera +0x28, the previous frame's: the cameras move after the kart
loop) must be <= 0xFFFF; vol' = vol * (0x2000 - 32 * sqrt(d)) >> 13; owner
-1 = a one-shot (vol' both sides), else the looping channel `owner`.

| where | game code | call |
|---|---|---|
| kart update, humans | 0x80064D40 engine | loop owner = player; id by engine class +0x173 (default 2; classes 2, 4, 8, 0x10, 0x20, 0x40: 1, 3, 5, 6 (pitch x 10/16), 7, 6 (pitch / 2)); pitch 0x1400 + abs(lurch +0x480) x 8 (+0x80 in the air on the throttle) + rev timer +0x4DE x 1024/60 + (s8)random / 2, halved on water; vol 0x60 |
| kart update, humans | 0x80064F18 scrape | loop player+2, id 0x1B, 5500 (4000 on water), min(+0x400 >> 5, 64) x 7/4, on the ground with +0x171 set |
| kart update | 0x80065118 SELECT | one-shot 0xF (0x1800, 0x60) |
| kart update | 0x80065DB8 not ready | id 9 (0x157C / 4000, 0x60) when 0x800B0EC4 != 0 |
| kart update | 0x800663B0 jump | id 0x21 (0x157C / 4000, 0x80) |
| kart update | 0x80066D2C skid | id 8 (0x1100 / 0xCC0, 0x80) on the random draw |
| kart update, humans | 0x80067834 impact | loop player+6, id 0xD, 5000 + 2 x random byte, 0x60, when +0x440 > 800 |
| kart update, humans | 0x80067D9C slide | loop player+4, id 0x1C (surfaces 0, 6, 9, 0xC..0xE) / 0x1A, 5000, min(+0x400 >> 5, 64) |
| FUN_8003E91C ground | wall, water, landing | 0x1D (rumble strength, <= 0x80); 0x1F (random < 0x10) or 0x20 (+0x4DB set) on water, 0x60; 10 (+ 0x12 on surface 0xD) by vertical speed |
| FUN_80040018 contacts | land on, bump | 0x21 (0x60) / 0xC (strength) at the midpoint |
| FUN_8005C2EC | countdown | one-shot 9 (0x1194, 0x80) as the countdown below 120 crosses a multiple of 30; 0x23 (0x157C) at GO |
| FUN_80082708 | lap | 0x22 (0x1500, 0xA0) |
| FUN_8007A7F4 | character voices | per kart, the lowest set request bit of +0x4DD (set by contacts, walls, the finish; cleared at each frame start by FUN_800306D0) with a voice in the character's row of 0x800AC232 (6 ids, -1 passes the request to the next bit) plays it (0x1800, 0x1500 for character 0x11; 0xA0) when the voice timer +0x4F5 is 0, which then holds for 60 VBlanks |

The item code (track A) makes the rest: pickups 0x16 / 0x18 / 0x1E,
firing 0x14, projectiles 0x14 / 0x15 / 0xB / 0x21 / 10, effects 0x57 /
0x18 / 0x23, specials; the calls are listed in docs/history/notes/SESSION-4-coord.md.
Only human karts have an engine sound (AI karts are heard through their
one-shots: jumps, landings, walls, contacts, voices).

### Verification [ORACLE] (route race3: 3 laps from slot 2, items on)
- Loops against the channel table 0x800D8AB0, teacher-forced per tick:
  engine pitch exact on 3,582 of the 3,585 ticks whose random pointer the
  harness recovers (93% of all 4,323 ticks: the others lack only the
  pointer), volume 4,323/4,323; scrape 3,769/3,769 and slide 387/387
  exact in pitch and volume.
- SPU key-ons: the port's calls (every kart teacher-forced, countdown,
  voices) through driver + SPU model against the oracle's KEYON events:
  425 of 729 matched by sample and VBlank (+-1); of those 98.8% on the same
  VBlank, pitch 100%, volumes 99.5%, ADSR 100%. Unmatched: 285 item sounds
  (not yet ported, track A) and 19 others (two lap sounds after the finish,
  ten landings around item hits, re-keys whose random byte differs).
  Voice numbers agree while the key-on sequence agrees (the first 14), then
  drift with the missing item sounds (12% overall).

## Phase 8 (Session 7): the demo, THE END, the cheats [CODE]
- The attract demo (frontend.md "Phase 8"): no course intro clip (the
  demo loads the race straight from the title, not through game mode 0
  where FUN_8008C4AC queues it), no countdown beeps (FUN_8005C2EC checks
  0x800B0BD2), no voice clips (FUN_80060580 / FUN_800606D0 check it); the
  course's music as in a race. The port: audio.rs skips the intro and the
  finish clip in the demo.
- THE END (course 0x1D): its course row has no track (+3 = -1); FUN_8008C4AC
  queues FUN_80089B98's pick instead (a random open course of worlds 0..5,
  its track not one of the last three, 0x800B0A18..1A), as for game mode
  0xE. Port: audio.rs draws it with the session's random pointer at the
  load (mr-sim title::credits_track). VOICES/CRDIT_01.XA (file row 14) is
  not requested by any path found (not the course row, not FUN_8008C4AC):
  not played.
- A cheat code plays sound 0x24 (5000, 0x80, 0x80), the "new vehicle"
  sound.
- FUN_80060580 call sites (character voice clips): MEET THE MUPPETS (game
  mode 0xD, not ported), FUN_80082708 (the finish: ported; two other calls),
  FUN_80081DD8, FUN_8007916C (a battle won), FUN_8006EC4C (CHICKEN CHASE
  catches), FUN_8008C4AC (league standings, game mode 0x12): not ported.

## Phase 11 (Session 8): every voice clip, MEET THE MUPPETS, CRDIT_01 [CODE][ORACLE]
Port: mr_sim::sound `XaReq` (the race code's requests, drained by mr-bevy
audio.rs `MusicDirector`), mr_audio::xa `XaQueue::request_entry`.

### Who writes the queue (0x800B0744..48) - all of it
| writer | entry | when |
|---|---|---|
| FUN_80060580(c, n) | 0x8000 \| base(c) + n, then the course track (not in game mode 0xD) | voice flag (0x800AC22E bit 0) set, not the demo (0x800B0BD2); three clips pending = dropped |
| FUN_800606D0 | 0xC000 \| 0xD0 + 0x800B074C (then 0x800B074C + 1 mod 7), then the course track | the results of a 1-player race won (mode 1, 0x800B0CA6 = 0, kart 0 place < 2, not knocked out under race flag 8), not the demo |
| FUN_8008C4AC, mode 0 | 0xC000 \| the course's intro clip (+0x4 + toggle) | once per course per power-on (Session 4) |
| FUN_8008C4AC, mode 7 | 0x8000 \| base(0x800B0C7E[0]) (no voice-flag test) | a BOSS CHALLENGE's load |
| FUN_8008C4AC, mode 0x12 | FUN_80060580(winner, 3) when the league's winner (0x800C1E30) is human | the league's trophy presentation (course 0x1A / 0x1B; row 12: menus.rs `start_presentation`) |
| FUN_8008C4AC / FUN_80060524 | the course track (FUN_80089B98's pick on course 0x1D and in mode 0xE) | every load |
| FUN_8005FB4C, mode 0xE | FUN_80089B98's pick, played once | TEST DRIVE, whenever nothing plays |
| FUN_80074A94, course select | the course track (o MUSIC, FUN_80060468): played at once, the queue emptied, unless it plays already (0x800B0740) | row 12: menus.rs + audio.rs `MusicDirector::play_now` |

base(c) = 0x800AC230[c * 0x14]: the 25 racers 0x20..0x80 (VOICES/CHAR_01..07,
16 channels each), the bosses 0xD8..0xDD (VOICES/WIN.XA channels 8..13; voice
flag clear, so only the mode-7 load plays them). Intro clips 0x90..0xBF
(FILM_01..03), the taunt 0xD0..0xD6 (WIN.XA 0..6).

**VOICES/CRDIT_01.XA (file row 14 = ids 0xC0..0xCF) is played by nothing**:
no character base, intro clip (course rows 0x800ACB34 +4: 0x90..0xBE + 1) or
music byte (< 0x20) lies in 0xC0..0xCF; the taunt adds at most 6 to 0xD0;
FUN_80089B98 returns course music bytes; the only other XA starts are
FUN_80060468 / FUN_80060490 (from FUN_8005FB4C and the course select's
MUSIC, a course track); FUN_8005FDF4 (load by name) reads data files, not
XA. The file is on the disc unused.

### The race's clips (FUN_80060580 call sites; humans only = kart +0x4D7 < 0x800B0E28)
| site | clip | port |
|---|---|---|
| FUN_80082708 @0x80082B60 (finish, the last lap or any lap once another finished) | 3 first, else 2 | progress.rs `race_finished` |
| FUN_80082708 @0x80082858 (knocked out: damage >= 1000, battle or race flag 8; not on course 0x1E) | 2 | progress.rs `knocked_out` |
| FUN_80081DD8 (stunt course finish gate) | 3 first, else 2 | stunt.rs |
| FUN_8007916C @0x800791F8 (modes 2 / 7: one kart standing, each human still in) | 3 | session.rs |
| FUN_8006EC4C (CHICKEN CHASE's winning catch) | the other player's 2, then the winner's 3 | chicken.rs |
| FUN_8007916C @0x80079850 FUN_800606D0 (the results of a won 1-player race) | WIN.XA taunt | session.rs |
| FUN_8008C4AC (mode 7 load) | the first boss's base | audio.rs (race load) |
| FUN_8008C4AC (mode 0x12) | the league winner's 3 | phase 12 (the presentation is not ported) |
Items, hits and the countdown make no XA requests (their voices are SPU
sounds: FUN_8007A7F4, "Trigger points" above).

- The results taunt: while the current entry has flags 0xC000 and plays,
  FUN_8004CEA0 (1 player) draws two faces (FUN_8002FD04 at (0x120 / 0xE0,
  0x1E, or 0x3C under the rival / NEW COURSE line), u = 0x80 + 0x20 * the
  blink toggle 0x800B07B0 / 0x800B07B2, v 0xC0 / 0xE0, page slot 0x14,
  CLUT +0xE): Statler and Waldorf heckling (mr-assets `taunt_faces`).
- Level [INFER]: FUN_800602C4 gives tracks >= 0x20 the voice level; the
  course intro's observed music level (above) is kept for the intro only;
  the taunt (also 0xC000) keeps the voice level (no oracle tap of it).
- MEET THE MUPPETS (frontend.md): entering stops the music; its clips queue
  no course track; nothing plays between clips. TEST DRIVE: random course
  tracks, each once.

### Checks [ORACLE] (tools/oracle/xa_probe.py; analysis/s8/meet, local)
| run | queue | first sector |
|---|---|---|
| gallery, Kermit resting (title state) | VB 234 0x8050 | CHAR_04.XA ch 0, LBA 192238 |
| gallery, RIGHT to Miss Piggy | VB 669 0x805C | CHAR_04.XA ch 12, LBA 192250 |
| TEST DRIVE | 0x0009 / 0x000A (random) | MUSIC2.XA ch 1 / 2, LBA 127210 / 127211 |
| LAGOON battle won (LAGOON_grid + battle_win_pokes) | VB 244 [0x8053, 0x0012]; the track again at VB 482 | CHAR_04.XA ch 3, LBA 192241 |
| LAGOON BOSS CHALLENGE load (route of boss_lagoon.json) | VB 285 [0x80DC, 0x0012] | WIN.XA ch 12, LBA 202266 |
| DOCK won (slot 2 + tri_finish route and poke) | VB 2670 [0x8053, 0x0010], then 0xC0D0 at VB 2908 | WIN.XA ch 0, LBA 202254; the two faces on the results (screenshot) |
The port: the same requests in the same order (mr-sim tests: battle.rs,
race.rs `a_win_queues_the_finish_clip_and_the_taunt` /
`finished_player_is_driven_by_the_ai`, stunt.rs, two_players.rs,
secrets.rs `boss_challenge_clip`), the same files / channels / first
sectors (mr-audio tests/xa_disc.rs).

## Phase 13 (Session 8): item key-ons over the whole race frame [ORACLE]
- `sfx_verify --session` runs the full race frame (Session::load_mem from
  the oracle's state, one tick with its pad words and dt) instead of the
  kart update alone: every kart, the items (pickups, projectiles,
  specials), the objects, the cameras' listeners. Capture
  analysis/oracle/sfx_items1 (`capture_sfx.py routes/items1.json
  --course --end 3300`: DOCK slot 2, items fired, R1 specials; the course
  regions + the voice dirty flags): SPU key-ons 308 / 308 matched (address,
  same VBlank, pitch, volumes, ADSR all 100 %); the looping channels'
  pitch and volume 100 %. The voice number agrees on 98.4 %: five key-ons
  pick another free voice (_SsVmAlloc reads the envelopes, which the SPU
  sampled at a sub-VBlank moment; not audible).
- Fix: `Session::load_mem` now loads the character voices' timers (kart
  +0x4F5): teacher forcing played four extra voice clips without them.
- The listeners: FUN_8007A620 takes the nearest human camera (two in split
  screen, `RaceGlobals::sound.listeners`, phase 7); volumes only, mono, as
  the game.
