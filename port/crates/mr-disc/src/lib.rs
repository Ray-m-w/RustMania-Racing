//! mr-disc: read files from the owner's Muppet RaceMania disc image.
//!
//! `iso` is copied from the quarry (verbatim except its first doc line and
//! the removed `pub mod fmv;`) (F:\Muppet Racemania
//! crates/mr-assets/src/lib.rs @ ed01e9ae, sha256 8b411a20...dd96), where it
//! was verified byte for byte on all 337 files of this disc. This module adds
//! a small front end: open from a .cue or .bin, look files up by path.
//! No game content is embedded; everything is read at run time.

pub mod iso;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use iso::{Error, FileEntry, Track};

pub struct Disc {
    bin: PathBuf,
    track: Track,
    files: HashMap<String, FileEntry>,
}

impl Disc {
    /// Open a disc from its .cue (single FILE line) or the .bin directly.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let bin = resolve_bin(path.as_ref())?;
        let mut track = Track::open(&bin)?;
        let lay = track.layout()?;
        let files = lay
            .files
            .into_iter()
            .map(|f| (normalize(&f.path), f))
            .collect();
        Ok(Disc { bin, track, files })
    }

    /// The raw track file this disc reads (the .bin a .cue names).
    pub fn bin_path(&self) -> &Path {
        &self.bin
    }

    /// A second, independent handle on the same image (its own file
    /// position), e.g. for a streaming thread.
    pub fn reopen(&self) -> Result<Self, Error> {
        Self::open(&self.bin)
    }

    /// Whole raw 2352-byte sectors (CD-XA audio needs the subheader and the
    /// 2324-byte Form 2 payload). Appends `count * 2352` bytes to `out`.
    pub fn read_raw(&mut self, lba: u32, count: u32, out: &mut Vec<u8>) -> Result<(), Error> {
        self.track.read_raw_sectors(lba, count, out)
    }

    /// Open the disc named by $MRM_DISC, else by the `disc = "..."` line of
    /// the repo's game.toml (searched upward from the current directory).
    pub fn open_default() -> Result<Self, Error> {
        if let Ok(p) = std::env::var("MRM_DISC") {
            return Self::open(p);
        }
        let mut dir = std::env::current_dir()?;
        loop {
            let toml = dir.join("game.toml");
            if let Ok(s) = std::fs::read_to_string(&toml) {
                for line in s.lines() {
                    let l = line.trim();
                    if let Some(rest) = l.strip_prefix("disc") {
                        if let Some(q) = rest.trim_start().strip_prefix('=') {
                            let v = q.trim().trim_matches('"').replace("\\\\", "\\");
                            return Self::open(v);
                        }
                    }
                }
            }
            if !dir.pop() {
                return Err(Error::NotTrack("no MRM_DISC and no game.toml disc line"));
            }
        }
    }

    /// Paths as on disc, without the ";1" version suffix, upper case,
    /// '/' separated: "COURSE_A/DOCK.TER".
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(|s| s.as_str())
    }

    pub fn entry(&self, path: &str) -> Option<&FileEntry> {
        self.files.get(&normalize(path))
    }

    pub fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        let e = self
            .entry(path)
            .ok_or(Error::NotTrack("no such file on disc"))?
            .clone();
        let mut out = Vec::with_capacity(e.size as usize);
        self.track.read_file(e.lba, e.size, &mut out)?;
        Ok(out)
    }
}

/// "course_a\\dock.ter;1" -> "COURSE_A/DOCK.TER"
pub fn normalize(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.split(';').next().unwrap_or("");
    p.trim_start_matches('/').to_ascii_uppercase()
}

fn resolve_bin(p: &Path) -> Result<PathBuf, Error> {
    let is_cue = p
        .extension()
        .map(|e| e.eq_ignore_ascii_case("cue"))
        .unwrap_or(false);
    if !is_cue {
        return Ok(p.to_path_buf());
    }
    let cue = std::fs::read_to_string(p)?;
    for line in cue.lines() {
        let l = line.trim();
        if l.to_ascii_uppercase().starts_with("FILE") {
            if let (Some(a), Some(b)) = (l.find('"'), l.rfind('"')) {
                if b > a {
                    let name = &l[a + 1..b];
                    return Ok(p.parent().unwrap_or(Path::new(".")).join(name));
                }
            }
        }
    }
    Err(Error::NotTrack("cue has no FILE line"))
}
