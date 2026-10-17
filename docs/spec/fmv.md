# FMV: the STR movies (phase 9)

Code: `port/crates/mr-video` (std only: `str` demux in lib.rs, `vlc`, `mdec`,
`movies`), the player in mr-bevy `movie.rs`, the oracle probe
`tools/oracle/fmv_check.py`, the CLI `mr-str` (info / png / wav / vram).

Tags as elsewhere: [CODE addr] generated C / Ghidra / disassembly of the
FMV.BIN overlay (tools/oracle/mipsdis.py on the exe + FMV.BIN placed at
0x800E3DD8), [DISC] read from the owner's disc, [ORACLE] measured on the
psxrecomp build over TCP, [INFER] inference.

## The files [DISC]
- 30 movies, FMV/*.STR (15) and FMV2/*.STR (15). `mr-str info` lists them:
  320 x 240 at 15 frames a second (10 sectors a frame), except FMV2/TT_15
  (the Traveller's Tales logo, 320 x 192, 6 sectors a frame = 25 a second).
  116 to 684 frames (7.9 s to 45.7 s).
- Sectors: video = Mode 2 Form 1 data, submode 0x48; user data starts with
  the 32-byte STR header {u16 0x0160, u16 0x8001, u16 sector in frame,
  u16 sectors in frame, u32 frame number (from 1), u32 bitstream bytes / 4,
  u16 width, u16 height, then the first bitstream header copy}; 2,016
  payload bytes follow. Audio = CD-XA Form 2, submode 0x64, one stream per
  movie (file 1, channel 1): 4-bit mono at 18,900 Hz (1 sector in 32) or,
  for TT_15, at 37,800 Hz (1 in 16). A few trailing data sectors are padding.
- Timing: the drive reads at double speed (150 sectors a second); each
  movie's audio lasts exactly its sectors / 150 (e.g. GMC1_15: 6,848
  sectors, 214 audio sectors x 4,032 samples / 18,900 = 45.65 s).
- Bitstream: version 2 on every frame (header {u16 MDEC words, 0x3800, u16
  quantiser scale (1 here), u16 2}).

## Decoding (mr-video)
1. Demux (`StrReader`): reads 16 raw sectors at a time (never the whole
   file), joins a frame's payloads by their sector index, decodes audio
   sectors with mr_audio::xa (ADPCM as the music) and resamples them to
   44,100 Hz with the oracle's per-sector linear resampler.
2. Bitstream -> MDEC halfwords (`vlc`, libpress DecDCTvlc): 16-bit LE
   words read MSB first; per block a 10-bit DC (`qscale << 10 | dc`, no
   prediction in v2), AC codes from MPEG-1's dct_coeff_next table + sign,
   escape `000001` + 6-bit run + 10-bit level, `10` = end of block (0xFE00).
   Block order Cr, Cb, Y0..Y3; macroblocks column by column.
3. MDEC (`mdec`; psx-spx "MDEC", bit-exact as Mednafen/Beetle and the
   oracle's runtime/src/mdec.c): dequantise in x16 fixed point (DC `qt[0]`,
   AC `(level * qscale * qt[k]) >> 3`, rounding bias -8 / +8, clamp
   [-0x4000, 0x3FFF]) into the column-major zig-zag position; two 8-point
   IDCT passes with the uploaded matrix (stored transposed, >> 3; `(sum +
   0x4000) >> 15`, the second pass wrapped to 9 bits and clamped to i8);
   YCbCr -> RGB `R = Y + ((359 Cr + 0x80) >> 8)`, `G = Y + (((-88 Cb) &
   !0x1F) + ((-183 Cr) & !7) + 0x80) >> 8`, `B = Y + ((454 Cb + 0x80) >>
   8)`, each wrapped to 9 bits, clamped, `^ 0x80` -> 24-bit unsigned (the
   game's mode: DecDCTin mode 3, 24 bpp) [CODE overlay 0x800E4EB0:
   0x80160084 = 3].
4. Tables (libpress, uploaded by DecDCTPutEnv) [DISC FMV.BIN]: the IQ
   table at 0x3E0C (luma) and 0x3E4C (chroma), identical: MPEG-1's default
   intra matrix in zig-zag order with DC 2; the IDCT matrix at 0x3E90,
   whose negative entries are the one's complement of the positive ones
   (0xE707 = !0x18F8), one below psx-spx's listing. mr-video holds the
   values; `movies::tests::the_disc_tables` checks them against the disc.

Checks: every frame of the 30 movies decodes (`mr-str info`: 0 bad; every
block ends inside the frame and the halfword count fits the header's MDEC
word count, `tests::every_frame_of_a_movie_decodes` on TT_15); ~1,600
frames a second on one core.

### Against the oracle [ORACLE]
`fmv_check.py OUT 0 BOG` / `0x12 LONDON 192`: a course-menu savestate
(scratch slot 11), the course's movie-object bit poked into RAM, COURSE
SECRETS -> PLAY MOVIE by the game's input, then `vram_peek` 128 x 128
halfwords in both display buffers ((x, 0) and (x, 240): the 24-bit picture,
480 halfwords a line) with the frame number 0x800E7BAC; `mr-str vram`
compares with the decode at each picture level.
- The oracle runs with the skip-fmv package on (build-dev mods/state.toml):
  the runtime presses the game's skip input as soon as a movie starts, so
  only frames 1..16 play, most in the skip fade. That exercised the fade
  as well.
- MM1_15 (BOG), picture x 0..85: frames 2 (level 8), 5 (6), 6 (6), 9 (4),
  10 (3), 13 (1): **32,768 / 32,768 bytes equal** each.
- MCC5_15 (LONDON), picture x 128..213: frames 2 (8), 5 (6), 6 (6), 9 (4),
  12 (2): **32,768 / 32,768** each.
- The remaining reads were torn (the next frame landing during the read).
- vram_peek's reply holds each pixel as a big-endian 4-digit hex value
  (fmv_check.py stores little-endian halfwords).
- Audio: not compared with the oracle's cd_in tap (the skip cut every
  movie to ~1 s); the XA decoder is the music's (sample-exact, audio.md).
  The port's mix (`--audio-dump`) of TT_15 correlates 1.0000 with
  `mr-str wav 0x1d` at gain 0.4374 = CdMix 0x70 / 0x80 x SPU main 0x3FFF.

## The movie player (FMV.BIN) [CODE]
FUN_8005CE60(movie) [CODE 8005ce60]: saves the RAM the overlay will
cover, loads `fmv\fmv.bin` to 0x800E3DD8 (FUN_8005FDF4), sets the volumes,
calls FUN_800E4618(movie), restores the volumes and the RAM.
- FUN_800E4618: name = 0x800E7AF0[movie] + ".str", end frame =
  0x800E7B6C[movie] (i16), then FUN_800E4EB0(name, end, 3, x 0, y (0x18 for
  0x1D, else 0), 0, skip_after (75 for 0x1C..0x1E, else 0)); clears VRAM
  after.
- The table (31 rows; `mr_video::movies::table` reads it from the disc):
  0 MM1, 1 MM2, 2 MM3, 3 MM5, 4 MM6, 5..10 GMC1/2/3/5/6/7, 11..14
  MTM2..5 (FMV/); 15..18 MCC1/2/3/5, 19..22 MTI2..5, 23..27 MFS1/2/3/4/6,
  0x1C HI, 0x1D TT, 0x1E MIDWAY (FMV2/). FMV2/MIDWAY.STR is not on this
  disc (the loader prints "File not found"; nothing asks for 0x1E on the
  paths found). The name strings at the file's start are the same list in
  reverse order (the quarry's manifest reading).
- The end frame equals each file's frame count.
- FUN_800E4EB0 per displayed frame (frame number f = 0x800E7BAC):
  - CdMix (0x800A1A68, {l, l, l, l}): f * 7 while f < 17 (fade in to 0x70);
    from end - 16 on (end - f) * 7; the SPU CD volume = the music level
    0x800B0A50 (SsSetSerialVol);
  - picture level 0x800F8DB0 (DecDCTout callback 0x800E47A8, jump table
    0x800E3F8C): 8 = unchanged; n < 8 keeps each byte's top n bits shifted
    down (`v >> (8 - n)`); from end - 16 on n = (end - f) / 2;
  - skip: START or CROSS held (0x800B0DA8 & 0x4008, the raw pad) once f >
    skip_after and f + 16 < end: FUN_8005EECC(0xF, 0x1800, 0x60, 0x60) and
    a count from 1; then each frame CdMix (12 - i) * 112 / 12 and level
    (12 - (i + 1)) * 8 / 12 (i = 1..11), the player returns at 12;
  - it also returns on a CD error (0x800F8DA0) or 0x800B0E84 = 1.
  `mr_video::movies::Player` is this state machine (tests: player_rules).

## Which movie plays when [CODE]
main (0x800786B4, the scene loop at 0x80078800..0x80078CF8; Ghidra drops
the loop as unreachable, the disassembly has it):
- **Power-on**: 0x800B0804 = 0x1C before the first scene; the loop plays
  0x1C (HI_15) then 0x1D (TT_15) (0x1E -> 0x1C -> 0x1D -> none), then the
  title. After 0x1D: FUN_8008A6FC(-7, 5), (-8, 5).
- **A course's first load since power-on**: before each scene load, with no
  movie asked for: if the course's cup / slot (FUN_8008AB5C: 0x800B0E38,
  0x800B0E32) is open (save flags & 1) and the course row's bit 0x800
  (0x800ACB3A + 0xC * course, RAM only) is clear, the course's movie
  (row + 9, 0x800ACB3D) plays and the bit is set. The oracle's BOG
  course-menu state has the bit on exactly the courses visited (1, 3, 8,
  15, 21) [ORACLE]. The attract demo's courses take this path too
  [CODE; not seen in the oracle].
- **Course select, SQUARE (MOVIE)** (FUN_80074A94 sub-state 0xC): the
  cursor's course has a movie and its save flags & 0x80 (the ADVENTURE
  movie object): sound 0xF, 0x800B0804 = movie, 0x800B080C = 0xC, the
  course select reloads after. The "s MOVIE" label is bright (0x60) when
  the flag is set, dim (0) otherwise (FUN_80051698).
- **COURSE SECRETS -> PLAY MOVIE** (FUN_80074A94 cases 8 / 9 / 0xA, row 3;
  the same bit): 0x800B0804 = the current course's movie, 0x800B0AF4 = the
  course (its course menu reloads).
- Course movies (row + 9): 28 rows of 37 have one (none: 0 SPEEDWAY, 10,
  29, 30 and 32..36 RALLY .. WHATEVER), each of movies 0..27 once; 0x1C..0x1E
  are the boot movies only.

## The port (mr-bevy movie.rs)
- A 4:3 picture as large as the window allows, black around (camera order
  40 over everything); the scene behind is frozen (virtual time paused),
  the music paused (resumed after); the menus, the title's START / cheat
  codes and the demo's START ignore input meanwhile.
- A worker thread streams the file (`StrReader`), decodes up to 4 frames
  ahead and pushes the audio to the bus (`AudioBus::push_movie`, ~0.33 s
  ahead; mixed as CD input with the player's CdMix level, at the music
  volume); each frame shows at its last sector's time; the player's rules
  as above (skip with START / CROSS = keyboard Enter / X, the confirm
  sound, fades).
- Power-on = the process (row 12: course changes happen in place, boot.rs):
  the boot movies play once at start-up; the courses whose first-load
  check ran are kept in memory (`SEEN`); loading.rs plays a course's
  first-load movie after the fade-out and before the LOADING picture, as
  main() does before FUN_8008ACE4.
- Row 12, the scene reload after an in-menu movie: MOVIE (course select)
  and PLAY MOVIE (COURSE SECRETS) fade the scene out, play the movie, then
  load the scene again behind the LOADING picture (main() between two
  course sessions: 0x800B0804 = movie, 0x800B080C = 0xC / 0x800B0AF4 =
  the course): the course select comes back at its cursor (`--open-page
  courses:W,C`), PLAY MOVIE's course on its course menu.
- `skip_intro_movies` (settings.txt / PORT SETTINGS, default false = as
  the game): skips the boot movies and the courses' first-load movies;
  MOVIE and PLAY MOVIE always play. The recomp package's skip-fmv
  (mods/preloaded, off by default) skips every movie.
- Tests / tools: `--movie M` plays movie M at start; `--movie-shot PATH
  --movie-shot-at N` captures the window at frame N and exits. Tool runs
  (`--route`, `--shot`, ...) play no movies.

## Not done / open
- Audio vs the oracle's cd_in tap (needs skip-fmv off in the oracle, an
  owner change of build-dev mods/state.toml and a restart).
- 0x800B0E84 / CD-error exits, and the overlay's VRAM clear after a movie
  (the port shows black then the scene).
- MIDWAY (0x1E) has no file; nothing found that asks for it.
