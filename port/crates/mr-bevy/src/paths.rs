//! Where the port keeps the player's files (phase 10): `save.bin`,
//! `settings.txt`, `crash.txt` and the disc check's `disc-check.txt`.
//!
//! The data folder, first match wins:
//! 1. `--data-dir PATH` on the command line;
//! 2. portable mode: a file `portable.txt` beside the executable keeps
//!    everything beside the executable (the layout before phase 10);
//! 3. the per-user folder: `%APPDATA%\MuppetRaceMania\` on Windows
//!    (`$XDG_DATA_HOME/MuppetRaceMania` or `~/.local/share/MuppetRaceMania`
//!    elsewhere), created on first use.
//!
//! One-time migration: when the data folder is not the executable's folder
//! and has no `save.bin` (or no `settings.txt`), the one beside the
//! executable (e.g. `port/target/release/save.bin` from before phase 10) is
//! COPIED in. Nothing is ever deleted or moved.
//!
//! Tests use no data folder (no file is read or written).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static DATA: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf()))
}

/// The data folder (None in tests or when no folder can be made).
pub fn data_dir() -> Option<&'static Path> {
    DATA.get_or_init(init).as_deref()
}

/// A file in the data folder.
pub fn data_file(name: &str) -> Option<PathBuf> {
    data_dir().map(|d| d.join(name))
}

/// Pick the folder from the process's arguments and environment.
pub fn choose(args: &[String], exe_dir: Option<&Path>, appdata: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(d) = args.iter().position(|a| a == "--data-dir").and_then(|i| args.get(i + 1)) {
        return Some(PathBuf::from(d));
    }
    if let Some(e) = exe_dir.filter(|e| e.join("portable.txt").exists()) {
        return Some(e.to_path_buf());
    }
    appdata.map(|a| a.join("MuppetRaceMania")).or_else(|| exe_dir.map(Path::to_path_buf))
}

fn user_base() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(PathBuf::from);
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
}

fn init() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let args: Vec<String> = crate::settings::args();
    let exe = exe_dir();
    let dir = choose(&args, exe.as_deref(), user_base())?;
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("data folder: cannot create {}: {e}; using the executable's folder", dir.display());
        return exe;
    }
    if let Some(e) = exe.as_deref() {
        migrate(e, &dir);
    }
    Some(dir)
}

/// Copy save.bin / settings.txt from `from` into `to` when `to` has none.
/// Returns the files copied.
pub fn migrate(from: &Path, to: &Path) -> Vec<&'static str> {
    let mut done = Vec::new();
    let same = match (from.canonicalize(), to.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => from == to,
    };
    if same {
        return done;
    }
    for name in ["save.bin", "settings.txt"] {
        let (src, dst) = (from.join(name), to.join(name));
        if src.is_file() && !dst.exists() {
            match std::fs::copy(&src, &dst) {
                Ok(_) => {
                    eprintln!("data folder: copied {} to {} (the original stays)", src.display(), dst.display());
                    done.push(name);
                }
                Err(e) => eprintln!("data folder: cannot copy {}: {e}", src.display()),
            }
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_and_migrate() {
        let tmp = std::env::temp_dir().join(format!("mrm-paths-{}", std::process::id()));
        let (exe, app) = (tmp.join("exe"), tmp.join("appdata"));
        std::fs::create_dir_all(&exe).unwrap();
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(choose(&a(&["x", "--data-dir", "D"]), Some(&exe), Some(app.clone())), Some(PathBuf::from("D")));
        assert_eq!(choose(&a(&["x"]), Some(&exe), Some(app.clone())), Some(app.join("MuppetRaceMania")));
        std::fs::write(exe.join("portable.txt"), "").unwrap();
        assert_eq!(choose(&a(&["x"]), Some(&exe), Some(app.clone())), Some(exe.clone()));
        let to = app.join("MuppetRaceMania");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(exe.join("save.bin"), b"old").unwrap();
        std::fs::write(exe.join("settings.txt"), b"msaa = 8\n").unwrap();
        std::fs::write(to.join("settings.txt"), b"msaa = 2\n").unwrap();
        assert_eq!(migrate(&exe, &to), vec!["save.bin"]);
        assert_eq!(std::fs::read(to.join("save.bin")).unwrap(), b"old");
        assert_eq!(std::fs::read(to.join("settings.txt")).unwrap(), b"msaa = 2\n");
        assert!(exe.join("save.bin").exists(), "the original stays");
        assert!(migrate(&exe, &to).is_empty(), "once");
        assert!(migrate(&exe, &exe).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
