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
- Measured: 5 min autopilot race, 0 music underruns.

## SFX, sample banks and engine (track D)
