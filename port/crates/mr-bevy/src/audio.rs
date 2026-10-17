//! Audio output (Session 4, track C): one stream through bevy_audio / rodio
//! at 44,100 Hz stereo, mixing the race music (CD-XA streamed from the disc
//! on its own thread, mr_audio::xa) with the SPU voice output track D pushes
//! (`AudioBus::push_sfx`). The mix follows the SPU: voices + CD input x CD
//! volume, clamped, x main volume (psxrecomp spu.c), then the user gains.
//!
//! Music follows the game's XA queue (mr_audio::xa::XaQueue, docs/spec/
//! audio.md): at the first race load the course's intro voice clip, then the
//! course track, looping seamlessly from the file start at the channel's end
//! marker; a restart leaves the looping track alone; the player's finish
//! clip interrupts it and the track starts again from the top after the
//! clip. Levels: music CdMix 0x6C (option table 0x800AC118[12]), voices
//! 0x60, SPU CD volume 0x7FFE and main volume 0x3FFF (oracle spu_status in
//! the DOCK race).
//!
//! Options: `--no-audio` (no device stream, the bus drops pushes),
//! `--music-volume V` / `--sfx-volume V` / `--volume V` (linear gain, 1.0 =
//! the game's level), `--no-music`, `--audio-stats` (bus counters every
//! 5 s on stderr), `--audio-dump PATH` (the mix as a 44.1 kHz WAV on exit,
//! up to 10 minutes; decoded game audio: keep it under analysis/).
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::audio::{AddAudioSource, AudioPlayer, Decodable, PlaybackSettings};
use bevy::prelude::*;
use mr_audio::xa::{self, CdMix, Step, TrackRef, XaStream};

pub const RATE: u32 = 44_100;
/// SFX latency target in frames (46 ms).
pub const SFX_TARGET: usize = 2048;
/// SFX queue cap (0.5 s): beyond it the oldest frames are dropped.
pub const SFX_CAP: usize = RATE as usize / 2;
/// Music decoded ahead of the device (0.4 s).
const MUSIC_AHEAD: usize = RATE as usize * 2 / 5;
/// Frames the device callback takes from the rings at a time.
const CHUNK: usize = 256;

/// The game's SPU levels in a race (oracle spu_status, DOCK): CD input
/// volume 0x7FFE (SsSetSerialVol 0x7F), main volume 0x3FFF.
pub const SPU_CD_VOLUME: i32 = 0x7FFE;
pub const SPU_MAIN_VOLUME: i32 = 0x3FFF;
/// Music level option table 0x800AC118 (16 x i16), default index 12
/// (FUN_8008F448 sets 0x800E0390 = 0x0C0C) = 0x6C; voices 0x800AC138[12] = 0x60.
pub const MUSIC_LEVEL: u8 = 0x6C;
pub const VOICE_LEVEL: u8 = 0x60;
/// DOCK's course index (0x800B0FAC in the oracle's DOCK race).
pub const DOCK_COURSE: u8 = 3;
/// Character voice table: 0x800AC22E, 20-byte rows: +0 bit 0 = has voice,
/// +2 = clip base (track id, 0xFF none). FUN_80060580 plays base + n.
pub const CHAR_VOICE_TABLE: u32 = 0x800A_C22E;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Master,
    Music,
    Sfx,
}

struct Shared {
    enabled: bool,
    sfx: Mutex<VecDeque<[i16; 2]>>,
    music: Mutex<VecDeque<[i16; 2]>>,
    music_paused: AtomicBool,
    /// Phase 9: a movie's XA audio (movie.rs; 44.1 kHz, before CdMix) and
    /// its CdMix level (the player's fades), mixed as CD input like music.
    movie: Mutex<VecDeque<[i16; 2]>>,
    movie_mix: AtomicU32,
    /// One-shot tracks whose end marker the music thread has read.
    clips_ended: AtomicU64,
    gains: [AtomicU32; 3],
    played: AtomicU64,
    sfx_underruns: AtomicU64,
    music_underruns: AtomicU64,
    /// --audio-dump: a copy of the mix.
    dump: Option<Mutex<Vec<[i16; 2]>>>,
}

/// The mixer bus (resource; clone = cheap handle). See docs/history/notes/SESSION-4-coord.md.
#[derive(Resource, Clone)]
pub struct AudioBus {
    sh: Arc<Shared>,
    music_tx: Option<Sender<MusicCmd>>,
}

impl AudioBus {
    fn new(enabled: bool) -> AudioBus {
        let g = |v: f32| AtomicU32::new(v.to_bits());
        AudioBus {
            sh: Arc::new(Shared {
                enabled,
                sfx: Mutex::new(VecDeque::with_capacity(SFX_CAP)),
                music: Mutex::new(VecDeque::with_capacity(MUSIC_AHEAD * 2)),
                music_paused: AtomicBool::new(false),
                movie: Mutex::new(VecDeque::new()),
                movie_mix: AtomicU32::new(0),
                clips_ended: AtomicU64::new(0),
                gains: [g(1.0), g(1.0), g(1.0)],
                played: AtomicU64::new(0),
                sfx_underruns: AtomicU64::new(0),
                music_underruns: AtomicU64::new(0),
                dump: opt_str("--audio-dump").map(|_| Mutex::new(Vec::new())),
            }),
            music_tx: None,
        }
    }

    /// False with --no-audio: pushes are dropped, sfx_wanted() is 0.
    pub fn enabled(&self) -> bool {
        self.sh.enabled
    }

    /// Append SPU voice output (44.1 kHz [L, R], before the SPU main volume).
    pub fn push_sfx(&self, frames: &[[i16; 2]]) {
        if !self.sh.enabled {
            return;
        }
        let mut q = self.sh.sfx.lock().unwrap();
        q.extend(frames.iter().copied());
        let over = q.len().saturating_sub(SFX_CAP);
        q.drain(..over);
    }

    /// SFX frames queued and not yet played.
    pub fn sfx_queued(&self) -> usize {
        self.sh.sfx.lock().unwrap().len()
    }

    /// SFX frames to render now to keep the queue at SFX_TARGET.
    pub fn sfx_wanted(&self) -> usize {
        if !self.sh.enabled {
            return 0;
        }
        SFX_TARGET.saturating_sub(self.sfx_queued())
    }

    /// Frames the device has consumed (the output clock).
    pub fn played_frames(&self) -> u64 {
        self.sh.played.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, bus: Bus, gain: f32) {
        self.sh.gains[bus as usize].store(gain.max(0.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self, bus: Bus) -> f32 {
        f32::from_bits(self.sh.gains[bus as usize].load(Ordering::Relaxed))
    }

    /// (sfx, music) frames the device wanted but the queue did not have.
    pub fn underruns(&self) -> (u64, u64) {
        (self.sh.sfx_underruns.load(Ordering::Relaxed), self.sh.music_underruns.load(Ordering::Relaxed))
    }

    /// Phase 9: append a movie's audio (44.1 kHz, before CdMix).
    pub fn push_movie(&self, frames: &[[i16; 2]]) {
        if self.sh.enabled {
            self.sh.movie.lock().unwrap().extend(frames.iter().copied());
        }
    }

    /// Movie frames queued and not yet played.
    pub fn movie_queued(&self) -> usize {
        self.sh.movie.lock().unwrap().len()
    }

    /// Drop the movie audio still queued.
    pub fn clear_movie(&self) {
        self.sh.movie.lock().unwrap().clear();
    }

    /// The movie's CdMix level (0x80 = unity; the player's fades).
    pub fn set_movie_mix(&self, level: u8) {
        self.sh.movie_mix.store(level as u32, Ordering::Relaxed);
    }

    /// Drive the music thread directly (MusicDirector does this from the
    /// game's queue; other callers should go through the director).
    pub fn music(&self, cmd: MusicCmd) {
        if let Some(tx) = &self.music_tx {
            let _ = tx.send(cmd);
        }
    }
}

/// Requests to the music thread (the drive side of FUN_800602C4 & co).
#[derive(Clone, Debug)]
pub enum MusicCmd {
    /// CdlSetfilter + CdlReadS from the track's file start; `level` is the
    /// CdMix byte. Replaces whatever plays (the old track's decoded-ahead
    /// audio is dropped).
    Play { track: u16, looping: bool, level: u8 },
    /// CdlPause + CdMix 0 (FUN_8005F9AC, game pause): keeps the position.
    Pause,
    /// FUN_8005FAA4: CdMix back, CdlReadS from the paused position.
    Resume,
    /// FUN_8005FA18: stop.
    Stop,
}

// ---------------------------------------------------------------- music thread

/// The game's XA file table as named in the exe (0x800ABF78: 12-byte rows
/// {name pointer, first sector, end sector}, the sectors filled by
/// CdSearchFile at boot), resolved against the disc directory.
struct XaFiles {
    files: Vec<Option<(u32, u32)>>,
}

impl XaFiles {
    fn load(disc: &mut mr_disc::Disc) -> XaFiles {
        let exe = disc.read("SLUS_012.37").unwrap_or_default();
        let files = xa::exe_xa_file_names(&exe)
            .into_iter()
            .map(|n| disc.entry(&n).map(|e| (e.lba, (e.size / 2048) as u32)))
            .collect();
        XaFiles { files }
    }

    fn stream(&self, track: u16, looping: bool) -> Option<XaStream> {
        let t = TrackRef::from_track_id(track);
        let (lba, n) = (*self.files.get(t.file_index as usize)?)?;
        Some(XaStream::new(lba, lba + n, t.channel, looping))
    }
}

fn music_thread(mut disc: mr_disc::Disc, sh: Arc<Shared>, rx: Receiver<MusicCmd>) {
    let files = XaFiles::load(&mut disc);
    let mut cur: Option<(XaStream, CdMix)> = None;
    let mut paused = false;
    let mut buf: Vec<[i16; 2]> = Vec::with_capacity(8192);
    loop {
        let have = sh.music.lock().unwrap().len();
        let busy = cur.is_some() && !paused && have < MUSIC_AHEAD;
        let cmd = if busy {
            match rx.try_recv() {
                Ok(c) => Some(c),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match rx.recv_timeout(Duration::from_millis(5)) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        if let Some(c) = cmd {
            match c {
                MusicCmd::Play { track, looping, level } => {
                    sh.music.lock().unwrap().clear();
                    cur = files.stream(track, looping).map(|s| (s, CdMix::level(level)));
                    if cur.is_none() {
                        warn!("music: track {track:#x} has no file");
                        sh.clips_ended.fetch_add(1, Ordering::Relaxed);
                    }
                    paused = false;
                    sh.music_paused.store(false, Ordering::Relaxed);
                }
                MusicCmd::Pause => {
                    paused = true;
                    sh.music_paused.store(true, Ordering::Relaxed);
                }
                MusicCmd::Resume => {
                    paused = false;
                    sh.music_paused.store(false, Ordering::Relaxed);
                }
                MusicCmd::Stop => {
                    sh.music.lock().unwrap().clear();
                    cur = None;
                }
            }
            continue;
        }
        if !busy {
            continue;
        }
        let Some((st, mix)) = cur.as_mut() else { continue };
        buf.clear();
        let step = st.fill(&mut disc, &mut buf, 4096);
        let mix = *mix;
        // CdMix (after resampling, as the drive does), then the SPU CD
        // input volume; the main volume is applied in the mixer.
        let mut q = sh.music.lock().unwrap();
        q.extend(buf.iter().map(|&f| {
            let m = mix.apply(f);
            [((m[0] as i32 * SPU_CD_VOLUME) >> 15) as i16, ((m[1] as i32 * SPU_CD_VOLUME) >> 15) as i16]
        }));
        drop(q);
        if step == Step::Ended {
            // A one-shot's end marker (FUN_80060160): the director moves on
            // once the device has played what was decoded.
            cur = None;
            sh.clips_ended.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------- the stream

/// The bevy_audio asset whose decoder is the endless mixer stream.
#[derive(Asset, TypePath, Clone)]
pub struct MixerSource {
    sh: Arc<Shared>,
}

pub struct MixerStream {
    sh: Arc<Shared>,
    out: Vec<f32>,
    pos: usize,
    sfx: Vec<[i16; 2]>,
    mus: Vec<[i16; 2]>,
}

impl MixerStream {
    fn refill(&mut self) {
        let sh = &self.sh;
        self.sfx.clear();
        self.mus.clear();
        {
            let mut q = sh.sfx.lock().unwrap();
            let n = q.len().min(CHUNK);
            self.sfx.extend(q.drain(..n));
        }
        if self.sfx.len() < CHUNK {
            sh.sfx_underruns.fetch_add((CHUNK - self.sfx.len()) as u64, Ordering::Relaxed);
        }
        if !sh.music_paused.load(Ordering::Relaxed) {
            let mut q = sh.music.lock().unwrap();
            let n = q.len().min(CHUNK);
            self.mus.extend(q.drain(..n));
            if n < CHUNK && n > 0 {
                sh.music_underruns.fetch_add((CHUNK - n) as u64, Ordering::Relaxed);
            }
        }
        let mut mov = [[0i16; 2]; CHUNK];
        let nmov = {
            let mut q = sh.movie.lock().unwrap();
            let n = q.len().min(CHUNK);
            for (d, s) in mov.iter_mut().zip(q.drain(..n)) {
                *d = s;
            }
            n
        };
        let mix = CdMix::level(sh.movie_mix.load(Ordering::Relaxed).min(0xFF) as u8);
        for f in mov.iter_mut().take(nmov) {
            let m = mix.apply(*f);
            *f = [((m[0] as i32 * SPU_CD_VOLUME) >> 15) as i16, ((m[1] as i32 * SPU_CD_VOLUME) >> 15) as i16];
        }
        let g = |b: Bus| f32::from_bits(sh.gains[b as usize].load(Ordering::Relaxed));
        let (gm, gs, gmaster) = (g(Bus::Music), g(Bus::Sfx), g(Bus::Master));
        self.out.clear();
        for i in 0..CHUNK {
            let s = self.sfx.get(i).copied().unwrap_or([0, 0]);
            let mu = self.mus.get(i).copied().unwrap_or([0, 0]);
            // the CD input: the music track or a movie's audio (never both
            // in the game; summed and clamped here)
            let m = [0, 1].map(|c| (mu[c] as i32 + mov[i][c] as i32).clamp(-32768, 32767) as i16);
            for c in 0..2 {
                // SPU: voices + CD, clamp16, x main volume >> 15.
                let v = (s[c] as f32 * gs + m[c] as f32 * gm).clamp(-32768.0, 32767.0);
                let v = v * SPU_MAIN_VOLUME as f32 / 32768.0 * gmaster;
                self.out.push((v / 32768.0).clamp(-1.0, 1.0));
            }
        }
        sh.played.fetch_add(CHUNK as u64, Ordering::Relaxed);
        if let Some(d) = &sh.dump {
            let mut d = d.lock().unwrap();
            if d.len() < RATE as usize * 600 {
                d.extend(self.out.chunks_exact(2).map(|f| [(f[0] * 32768.0) as i16, (f[1] * 32768.0) as i16]));
            }
        }
        self.pos = 0;
    }
}

impl Iterator for MixerStream {
    type Item = bevy::audio::Sample;
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.out.len() {
            self.refill();
        }
        let v = self.out[self.pos];
        self.pos += 1;
        Some(v as bevy::audio::Sample)
    }
}

impl bevy::audio::Source for MixerStream {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> bevy::audio::ChannelCount {
        std::num::NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> bevy::audio::SampleRate {
        std::num::NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for MixerSource {
    type Decoder = MixerStream;
    fn decoder(&self) -> MixerStream {
        MixerStream {
            sh: self.sh.clone(),
            out: Vec::with_capacity(CHUNK * 2),
            pos: 0,
            sfx: Vec::with_capacity(CHUNK),
            mus: Vec::with_capacity(CHUNK),
        }
    }
}

// ---------------------------------------------------------------- plugin

pub struct AudioOutPlugin;

fn flag(k: &str) -> bool {
    crate::settings::args().into_iter().any(|a| a == k)
}

fn opt_str(k: &str) -> Option<String> {
    let a: Vec<String> = crate::settings::args();
    a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned()
}

fn opt_f32(k: &str) -> Option<f32> {
    opt_str(k).and_then(|v| v.parse().ok())
}

impl Plugin for AudioOutPlugin {
    fn build(&self, app: &mut App) {
        let enabled = !flag("--no-audio");
        let mut bus = AudioBus::new(enabled);
        if let Some(v) = opt_f32("--volume") {
            bus.set_volume(Bus::Master, v);
        }
        if let Some(v) = opt_f32("--music-volume") {
            bus.set_volume(Bus::Music, v);
        }
        if let Some(v) = opt_f32("--sfx-volume") {
            bus.set_volume(Bus::Sfx, v);
        }
        if enabled && !flag("--no-music") {
            match mr_disc::Disc::open_default() {
                Ok(disc) => {
                    let (tx, rx) = mpsc::channel();
                    let sh = bus.sh.clone();
                    std::thread::Builder::new()
                        .name("mr-music".into())
                        .spawn(move || music_thread(disc, sh, rx))
                        .expect("spawn the music thread");
                    bus.music_tx = Some(tx);
                }
                Err(e) => warn!("music off: cannot open the disc ({e})"),
            }
        }
        app.insert_resource(bus);
        // row 12: a new director per course (the game's tables per course);
        // the drawn tracks and the taunt channel are RAM state that lives on
        crate::boot::on_boot(app, |w| {
            let mut d = MusicDirector::default();
            if let Some(old) = w.remove_resource::<MusicDirector>() {
                d.recent = old.recent;
                d.taunt_next = old.taunt_next;
                if let Some(bus) = w.get_resource::<AudioBus>() {
                    bus.music(MusicCmd::Stop);
                }
            }
            w.insert_resource(d);
        });
        app.add_audio_source::<MixerSource>();
        if enabled {
            app.add_systems(Startup, start_stream);
        }
        app.add_systems(Update, direct_music);
        if flag("--audio-stats") {
            app.add_systems(Update, audio_stats);
        }
        app.add_systems(Last, dump_on_exit);
    }
}

fn start_stream(mut c: Commands, bus: Res<AudioBus>, mut assets: ResMut<Assets<MixerSource>>) {
    let h = assets.add(MixerSource { sh: bus.sh.clone() });
    c.spawn((AudioPlayer::<MixerSource>(h), PlaybackSettings::ONCE));
}

fn audio_stats(bus: Res<AudioBus>, dir: Res<MusicDirector>, p: Res<crate::Player>, time: Res<Time<Real>>, mut next: Local<f64>) {
    let t = time.elapsed_secs_f64();
    if t < *next {
        return;
    }
    *next = t + 5.0;
    let (su, mu) = bus.underruns();
    let mq = bus.sh.music.lock().unwrap().len();
    if let crate::Driver::Sim(run) = &p.driver {
        eprintln!("audio: sim tick {} phase {:?}", run.session.ticks, run.session.phase());
    }
    eprintln!(
        "audio: t {:.1} s played {:.2} s | xa cur {:?} queue {:?} | music queued {} underrun {} | sfx queued {} underrun {}",
        t,
        bus.played_frames() as f64 / RATE as f64,
        dir.queue.cur.map(|c| format!("{c:#06x}")),
        dir.queue.q,
        mq,
        mu,
        bus.sfx_queued(),
        su
    );
}

fn dump_on_exit(mut ev: MessageReader<AppExit>, bus: Res<AudioBus>) {
    if ev.read().next().is_none() {
        return;
    }
    let (Some(path), Some(d)) = (opt_str("--audio-dump"), &bus.sh.dump) else { return };
    let d = d.lock().unwrap();
    let n = (d.len() * 4) as u32;
    let mut b = Vec::with_capacity(44 + d.len() * 4);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&n.to_le_bytes());
    for f in d.iter() {
        b.extend_from_slice(&f[0].to_le_bytes());
        b.extend_from_slice(&f[1].to_le_bytes());
    }
    let _ = std::fs::write(&path, b);
    eprintln!("audio: wrote {} frames to {path}", d.len());
}

/// ADJUST SOUND (menus.rs): the game's volume steps as gains on the music
/// and SFX buses (1.0 = step 12, the game's default), times the
/// command line's `--music-volume` / `--sfx-volume`.
pub fn apply_game_volumes(bus: &AudioBus, music: f32, sound: f32) {
    bus.set_volume(Bus::Music, opt_f32("--music-volume").unwrap_or(1.0) * music);
    bus.set_volume(Bus::Sfx, opt_f32("--sfx-volume").unwrap_or(1.0) * sound);
}

// ---------------------------------------------------------------- race phases

/// The game's XA queue driven from the race (docs/spec/audio.md): the race
/// loads, the race code's requests (`mr_sim::sound::XaReq`: the voice clips
/// of FUN_80060580 and the results taunt FUN_800606D0), MEET THE MUPPETS'
/// clips and TEST DRIVE's tracks (phase 11).
#[derive(Resource, Default)]
pub struct MusicDirector {
    pub queue: xa::XaQueue,
    started: bool,
    last_ticks: u64,
    intro_played: bool,
    /// Phase 8: THE END's track drawn.
    started_end: bool,
    clips_seen: u64,
    /// A clip ended on the thread; the device still plays its tail.
    pending_end: bool,
    /// Set by menus.rs on the course menu: its course track plays without
    /// the intro clip, which waits for the race load.
    pub menu: bool,
    /// Phase 11: MEET THE MUPPETS (game mode 0xD) is open (menus.rs): the
    /// music stops on entry (FUN_8005FA18) and a clip queues no course
    /// track behind it (FUN_80060580 in mode 0xD).
    pub gallery: bool,
    in_gallery: bool,
    /// Phase 11: TEST DRIVE (game mode 0xE) runs (menus.rs): whenever
    /// nothing plays, a random open course's track once (FUN_8005FB4C).
    pub test_drive: bool,
    /// 0x800B0A18..1A: the last three tracks FUN_80089B98 drew (None:
    /// the executable's initial -1s).
    recent: Option<[i8; 3]>,
    /// 0x800B074C: the next results taunt's WIN.XA channel (0..6).
    taunt_next: u8,
    tables: Option<Tables>,
}

#[derive(Clone, Default)]
struct Tables {
    music: Option<u16>,
    intro: Option<u16>,
    /// Character rows (flags, clip base): the 25 racers and the bosses.
    chars: Vec<(u8, u8)>,
}

fn load_tables() -> Tables {
    let Ok(mut d) = mr_disc::Disc::open_default() else { return Tables::default() };
    let exe = d.read("SLUS_012.37").unwrap_or_default();
    let at = |a: u32| exe.get((a - xa::EXE_T_ADDR) as usize + 0x800).copied();
    let chars = (0..0x30u32)
        .map_while(|c| Some((at(CHAR_VOICE_TABLE + c * 20)?, at(CHAR_VOICE_TABLE + c * 20 + 2)?)))
        .collect();
    // The course's row of 0x800ACB34 (+3 music track, +4 intro clip).
    let course = crate::courses::id() as u8;
    Tables { music: xa::exe_course_music(&exe, course), intro: xa::exe_course_voice(&exe, course), chars }
}

/// The track of the results taunt: VOICES/WIN.XA channel n is track 0xD0 +
/// n, queued with flags 0xC000 (FUN_800606D0).
pub const TAUNT_BASE: u16 = 0xD0;

impl MusicDirector {
    fn tables(&mut self) -> Tables {
        self.tables.get_or_insert_with(load_tables).clone()
    }

    /// FUN_80060580(character, n): the character's voice clip (base + n)
    /// once, then the course track again from its start (not in MEET THE
    /// MUPPETS). Ignored for characters without voice (row +0 bit 0; the
    /// bosses). Character = kart record +0x4B2.
    pub fn clip(&mut self, character: u8, n: u8) {
        let t = self.tables();
        let Some(&(flags, base)) = t.chars.get(character as usize) else { return };
        if flags & 1 == 0 || base == 0xFF {
            return;
        }
        let music = if self.gallery { None } else { t.music };
        self.queue.request_clip(base as u16 + n as u16, music);
    }

    /// FUN_800606D0: the results taunt, VOICES/WIN.XA channel 0x800B074C
    /// (which then steps through 0..6), flags 0xC000, then the course track.
    pub fn taunt(&mut self) {
        let t = self.tables();
        let n = self.taunt_next;
        self.taunt_next = (n + 1) % 7;
        self.queue.request_entry((TAUNT_BASE + n as u16) | xa::ONE_SHOT | xa::INTRO, t.music);
    }

    /// Row 12: FUN_80060468 from the course select's MUSIC: this track
    /// now (from its start, looping), the queue emptied.
    pub fn play_now(&mut self, track: u16) {
        self.queue.stop();
        self.pending_end = false;
        self.queue.q = [Some(track), None, None];
    }

    /// 0x800B0760 bit 0: a track or clip is playing.
    pub fn xa_playing(&self) -> bool {
        self.queue.playing
    }

    /// FUN_8004CEA0's test: the current entry has flags 0xC000 (the results
    /// taunt; also the course intro) and plays.
    pub fn taunt_playing(&self) -> bool {
        self.queue.playing && self.queue.cur.is_some_and(|c| c & 0xC000 == 0xC000)
    }
}

/// Phase 11: the results taunt plays (FUN_8004CEA0 draws its two faces);
/// read by hud.rs.
pub static TAUNT_PLAYING: AtomicBool = AtomicBool::new(false);
/// Phase 11: 0x800B0760 bit 0 and 0x800B074A's character: what the gallery
/// (main.rs) reads; bit 8 = playing, low byte = the current clip's track.
pub static XA_STATE: AtomicU32 = AtomicU32::new(0);

/// Race load / restart / the race's requests -> the queue; the queue -> the
/// music thread.
fn direct_music(mut p: ResMut<crate::Player>, bus: Res<AudioBus>, mut dir: ResMut<MusicDirector>) {
    // Phase 9: no music decisions while a movie plays (the game's player
    // owns the drive; the scene is frozen)
    if crate::movie::playing() || crate::loading::holding() {
        return;
    }
    // Phase 11: MEET THE MUPPETS stops the music (FUN_8008C4AC entering game
    // mode 0xD: FUN_8005FA18).
    if dir.gallery && !dir.in_gallery {
        dir.queue.stop();
        dir.pending_end = false;
        bus.music(MusicCmd::Stop);
    }
    dir.in_gallery = dir.gallery;
    // (the race's requests are drained without marking the player changed)
    let p = p.bypass_change_detection();
    if let crate::Driver::Sim(run) = &mut p.driver {
        let s = &mut run.session;
        let ticks = s.ticks;
        let fresh = !dir.started || ticks < dir.last_ticks;
        dir.last_ticks = ticks;
        if fresh && !dir.gallery && !dir.test_drive {
            // Race load (a restart in the port = the game's menu round trip).
            dir.started = true;
            // Phase 8: THE END plays a random open course's track
            // (FUN_80089B98; course 0x1D's row has none).
            if crate::ending::on() && !dir.started_end {
                dir.started_end = true;
                let mut rng = s.globals.rng;
                let open = |w: usize, t: usize| crate::save::with(|st| st.block.course_open(w, t));
                let tr = mr_sim::title::credits_track(&s.world, &mut rng, &open, &mut [-1; 3]);
                let mut t = dir.tables();
                t.music = (tr >= 0).then_some(tr as u16);
                dir.tables = Some(t);
            }
            let t = dir.tables();
            // (the attract demo loads straight into the race: no intro clip,
            // FUN_8008C4AC plays it in game mode 0 only)
            let intro = if dir.intro_played || dir.menu || crate::demo::active() { None } else { t.intro };
            dir.intro_played |= !dir.menu;
            dir.queue.race_load(t.music, intro);
            // Phase 11: a BOSS CHALLENGE (game mode 7, FUN_8008C4AC
            // @0x8008D1xx): the first boss's clip base (0x800AC230[0x800B0C7E
            // * 0x14], no voice-flag test: VOICES/WIN.XA channels 8..13)
            // before the course track.
            if s.globals.mode == 7 {
                let boss = mr_sim::secrets::bosses(s.config().course).0.first().map(|b| b.0);
                if let Some(&(_, base)) = boss.and_then(|b| t.chars.get(b as usize)) {
                    if base != 0xFF {
                        dir.queue.request_entry(base as u16 | xa::ONE_SHOT, t.music);
                    }
                }
            }
        } else if fresh {
            dir.started = true;
        }
        // The race code's requests (FUN_80060580 / FUN_800606D0).
        for r in std::mem::take(&mut s.globals.sound.xa) {
            match r {
                mr_sim::sound::XaReq::Clip { character, n } => dir.clip(character, n),
                mr_sim::sound::XaReq::Taunt => dir.taunt(),
            }
        }
        // Phase 11: TEST DRIVE (FUN_8005FB4C in game mode 0xE): nothing
        // playing -> FUN_80089B98's track (a random open course of worlds
        // 0..5, not one of the last three), once.
        if dir.test_drive && !dir.queue.playing && !dir.pending_end {
            let open = |w: usize, t: usize| crate::save::with(|st| st.block.course_open(w, t));
            let mut recent = dir.recent.unwrap_or([-1; 3]);
            let tr = mr_sim::title::credits_track(&s.world, &mut s.globals.rng, &open, &mut recent);
            dir.recent = Some(recent);
            if tr >= 0 {
                dir.queue.q = [Some(tr as u16 | xa::ONE_SHOT), None, None];
            }
        }
    }
    // FUN_80060160: a clip's end marker; the device first plays out what
    // was decoded before it, then the queue moves on.
    let ended = bus.sh.clips_ended.load(Ordering::Relaxed);
    if ended != dir.clips_seen {
        dir.clips_seen = ended;
        dir.pending_end = true;
    }
    if dir.pending_end && bus.sh.music.lock().unwrap().is_empty() {
        dir.pending_end = false;
        dir.queue.clip_ended();
    }
    if let Some((track, looping)) = dir.queue.frame() {
        // FUN_800602C4: CdMix = the music level for tracks < 0x20, else the
        // voice level. Observed exception: the course intro clip at race
        // load plays at the music level (oracle cd_in tap: gain exactly
        // 0x6C/0x60), so some CdMix(music) runs during the load after it
        // starts (not located); the results taunt (also flags 0xC000, but
        // queued in the race) keeps the voice level.
        let taunt = (TAUNT_BASE..TAUNT_BASE + 7).contains(&track);
        let intro = dir.queue.cur.is_some_and(|c| c & xa::INTRO != 0) && !taunt;
        let level = if track < 0x20 || intro { MUSIC_LEVEL } else { VOICE_LEVEL };
        bus.music(MusicCmd::Play { track, looping, level });
    }
    TAUNT_PLAYING.store(dir.taunt_playing(), Ordering::Relaxed);
    let cur = dir.queue.cur.unwrap_or(0) & 0xFF;
    XA_STATE.store(((dir.queue.playing as u32) << 8) | cur as u32, Ordering::Relaxed);
}
