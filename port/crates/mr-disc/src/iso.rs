//! ISO 9660 reader (copied from the quarry, see ../lib.rs). Original header:
//! mr-assets — disc-side asset access for Muppet RaceMania.
//!
//! Session 2 scope (MILESTONES item 1): decode the ISO 9660 layout of the
//! owner's Redump-style raw track (`extract/muppet-usa.bin`, MODE2/2352,
//! derived from the archival CHD) and read member bytes by (lba, size).
//!
//! Sector geometry [T1, validated against this disc]: every 2352-byte sector
//! carries 2048 bytes of user data at byte offset 24; the primary volume
//! descriptor sits at logical sector 16 and carries the id `CD001` at byte 1
//! of its user data (ECMA-119 9.1 [T5], confirmed for this disc by
//! tools/extract/iso_inventory.py, which this module mirrors 1:1).
//!
//! No game content is embedded in this crate; everything is read at runtime.



use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SECTOR_RAW: u64 = 2352;
pub const SECTOR_DATA: usize = 2048;
/// User-data offset inside a MODE2/2352 sector. The fallback offset of 16
/// (MODE1/2352) is probed at open, like the Python reference does.
/// [T1: this disc answers at 24; tools/extract/iso_inventory.py:11-13]
const DATA_OFFSETS: [u64; 2] = [24, 16];

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    NotTrack(&'static str),
    NoVolumeDescriptor,
    ShortRead,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::NotTrack(s) => write!(f, "track: {s}"),
            Error::NoVolumeDescriptor => write!(f, "no CD001 volume descriptor at sector 16"),
            Error::ShortRead => write!(f, "short read"),
        }
    }
}

impl std::error::Error for Error {}

type R<T> = Result<T, Error>;

/// A raw CD track image (single data track, Redump layout).
pub struct Track {
    file: File,
    nsect: u64,
    data_off: u64,
}

impl Track {
    pub fn open(path: impl AsRef<Path>) -> R<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len % SECTOR_RAW != 0 {
            return Err(Error::NotTrack("track size is not a multiple of 2352"));
        }
        let mut probe = Track {
            file,
            nsect: len / SECTOR_RAW,
            data_off: DATA_OFFSETS[0],
        };
        let mut found = None;
        for off in DATA_OFFSETS {
            if let Some(d) = probe.read_raw(16, 1, off) {
                if d[0] == 1 && &d[1..6] == b"CD001" {
                    found = Some(off);
                    break;
                }
            }
        }
        match found {
            Some(off) => {
                probe.data_off = off;
                Ok(probe)
            }
            None => Err(Error::NoVolumeDescriptor),
        }
    }

    fn read_raw(&mut self, lba: u32, count: u32, off: u64) -> Option<Vec<u8>> {
        let lba = lba as u64;
        let count = count as u64;
        if lba + count > self.nsect {
            return None;
        }
        self.file
            .seek(SeekFrom::Start(lba * SECTOR_RAW + off))
            .ok()?;
        let mut raw = vec![0u8; (count * SECTOR_RAW) as usize];
        self.file.read_exact(&mut raw).ok()?;
        let mut out = Vec::with_capacity((count as usize) * SECTOR_DATA);
        for chunk in raw.chunks_exact(SECTOR_RAW as usize) {
            out.extend_from_slice(&chunk[..SECTOR_DATA]);
        }
        Some(out)
    }

    /// User data of `count` logical sectors starting at `lba`.
    pub fn read(&mut self, lba: u32, count: u32) -> Option<Vec<u8>> {
        let o = self.data_off;
        self.read_raw(lba, count, o)
    }

    /// Number of 2352-byte sectors in the image.
    pub fn sectors(&self) -> u64 {
        self.nsect
    }

    /// `count` whole raw 2352-byte sectors starting at `lba` (sync, header,
    /// subheader, data, EDC: the bytes a CD-XA Mode 2 Form 2 audio sector
    /// needs, which the 2048-byte user-data view cuts off). Session 4 C.
    pub fn read_raw_sectors(&mut self, lba: u32, count: u32, out: &mut Vec<u8>) -> R<()> {
        let (lba, count) = (lba as u64, count as u64);
        if lba + count > self.nsect {
            return Err(Error::ShortRead);
        }
        self.file.seek(SeekFrom::Start(lba * SECTOR_RAW))?;
        let start = out.len();
        out.resize(start + (count * SECTOR_RAW) as usize, 0);
        self.file.read_exact(&mut out[start..])?;
        Ok(())
    }

    /// Stream one file's bytes (exact `size` bytes across ceil-like sector
    /// coverage) into `sink` without materializing it; returns bytes written.
    pub fn read_file(&mut self, lba: u32, size: u64, sink: &mut dyn io::Write) -> R<u64> {
        let nsect = (size + SECTOR_DATA as u64 - 1) / SECTOR_DATA as u64;
        let mut left = size;
        let mut written = 0u64;
        for s in 0..nsect {
            let Some(d) = self.read(lba + s as u32, 1) else {
                return Err(Error::ShortRead);
            };
            let take = std::cmp::min(left, SECTOR_DATA as u64) as usize;
            sink.write_all(&d[..take])?;
            written += take as u64;
            left -= take as u64;
        }
        Ok(written)
    }
}

fn rd_u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn dec_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// One directory record (ECMA-119 9.1) [T5 layout; validated byte-for-byte
/// on this disc by the reference tool, docs/history/notes/SESSION-1-toolchain.md].
pub struct DirRec {
    pub name: String,
    pub lba: u32,
    pub size: u32,
    pub is_dir: bool,
    entry_len: usize,
}

impl DirRec {
    fn parse(buf: &[u8], o: usize) -> Option<DirRec> {
        let len_a = buf[o] as usize;
        if len_a == 0 {
            return None;
        }
        let e = &buf[o..o + len_a];
        let lba = rd_u32_le(e, 2);
        let size = rd_u32_le(e, 10);
        let is_dir = e[25] & 0x02 != 0;
        let fver = e[32] as usize;
        let raw = if e.len() > 33 {
            &e[33..33 + fver.min(e.len().saturating_sub(33))]
        } else {
            &[]
        };
        let name = match raw.first() {
            Some(0) => ".".to_string(),
            Some(1) => "..".to_string(),
            _ => dec_str(raw),
        };
        Some(DirRec {
            name,
            lba,
            size,
            is_dir,
            entry_len: len_a,
        })
    }
}

/// One enumerated regular file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileEntry {
    pub path: String,
    pub lba: u32,
    pub size: u64,
}

pub struct IsoLayout {
    pub volume_name: String,
    pub files: Vec<FileEntry>,
    pub dirs: usize,
}

impl Track {
    /// Walk the directory tree of the disc. Recursion depth is bounded by
    /// ISO 9660's own limit (8); plain recursion is fine here.
    pub fn layout(&mut self) -> R<IsoLayout> {
        let pvd = self.read(16, 1).ok_or(Error::NoVolumeDescriptor)?;
        if !(pvd[0] == 1 && &pvd[1..6] == b"CD001") {
            return Err(Error::NoVolumeDescriptor);
        }
        let volume_name = dec_str(&pvd[40..72]);
        let root = DirRec::parse(&pvd, 156).ok_or(Error::NoVolumeDescriptor)?;
        let mut files = Vec::new();
        let mut dirs = 0usize;
        self.walk(root.lba, root.size, "", &mut files, &mut dirs)?;
        files.sort();
        Ok(IsoLayout {
            volume_name,
            files,
            dirs,
        })
    }

    fn walk(
        &mut self,
        lba: u32,
        size: u32,
        prefix: &str,
        out: &mut Vec<FileEntry>,
        dirs: &mut usize,
    ) -> R<()> {
        let nsect = (size as u64 + SECTOR_DATA as u64 - 1) / SECTOR_DATA as u64;
        let data = self.read(lba, nsect as u32).ok_or(Error::ShortRead)?;
        let mut o = 0usize;
        while o < data.len() {
            if data[o] == 0 {
                o = (o / SECTOR_DATA + 1) * SECTOR_DATA; // skip to sector boundary
                continue;
            }
            let Some(rec) = DirRec::parse(&data, o) else {
                break;
            };
            o += rec.entry_len;
            if rec.name == "." || rec.name == ".." {
                continue;
            }
            let path = if prefix.is_empty() {
                rec.name.clone()
            } else {
                format!("{prefix}/{}", rec.name)
            };
            if rec.is_dir {
                *dirs += 1;
                self.walk(rec.lba, rec.size, &path, out, dirs)?;
            } else {
                out.push(FileEntry {
                    path,
                    lba: rec.lba,
                    size: rec.size as u64,
                });
            }
        }
        Ok(())
    }
}

/// Write every regular file to `outdir/<path>` (names as on disc, e.g.
/// `SLUS_012.37;1`), like the reference extractor. Returns count written.
pub fn extract_all(bin: &Path, outdir: &Path) -> R<usize> {
    let mut t = Track::open(bin)?;
    let lay = t.layout()?;
    let mut n = 0;
    for f in &lay.files {
        let dest = outdir.join(&f.path);
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut out = File::create(&dest)?;
        t.read_file(f.lba, f.size, &mut out)?;
        n += 1;
    }
    Ok(n)
}

/// Verify files on disk against the track byte for byte (streaming, cheap
/// per-sector). Returns list of mismatched paths (empty = perfect).
pub fn verify_against_dir(bin: &Path, dir: &Path) -> R<Vec<String>> {
    let mut t = Track::open(bin)?;
    let lay = t.layout()?;
    let mut bad = Vec::new();
    let mut buf = vec![0u8; SECTOR_DATA];
    let mut sbuf = Vec::with_capacity(SECTOR_DATA);
    for f in &lay.files {
        let mut on = match File::open(dir.join(&f.path)) {
            Ok(x) => x,
            Err(_) => {
                bad.push(f.path.clone());
                continue;
            }
        };
        if on.metadata()?.len() != f.size {
            bad.push(f.path.clone());
            continue;
        }
        let mut off = 0u64;
        let mut mismatch = false;
        while off < f.size {
            let take = std::cmp::min(f.size - off, SECTOR_DATA as u64) as usize;
            on.read_exact(&mut buf[..take])?;
            sbuf.clear();
            // re-read the corresponding track slice
            let sect = (off / SECTOR_DATA as u64) as u32;
            let frac = (off % SECTOR_DATA as u64) as usize;
            let d = t.read(f.lba + sect, 1).ok_or(Error::ShortRead)?;
            sbuf.extend_from_slice(&d[frac..frac + take]);
            if sbuf[..take] != buf[..take] {
                mismatch = true;
                break;
            }
            off += take as u64;
        }
        if mismatch {
            bad.push(f.path.clone());
        }
    }
    Ok(bad)
}

/// Default artifact locations relative to the repo root (documented in
/// docs/history/notes/SESSION-2-assets.md; overridable from the CLI).
pub fn default_paths(repo: &Path) -> (PathBuf, PathBuf) {
    (
        repo.join("extract/muppet-usa.bin"),
        repo.join("extract/files"),
    )
}
