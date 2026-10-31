//! Temporary files for Open With's past versions: one folder per running
//! ular-git (`<tmp>/ular-git-<pid>`), removed on exit. Folders of earlier
//! runs that did not exit cleanly are removed a day later.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const PREFIX: &str = "ular-git-";
/// Age after which another run's folder counts as left behind.
const STALE: Duration = Duration::from_secs(24 * 60 * 60);

/// This run's folder (not created until a version is written).
pub fn session_dir() -> PathBuf {
    std::env::temp_dir().join(format!("{PREFIX}{}", std::process::id()))
}

/// Writes `data` to `dir/<label>/<path>` read-only (editing it changes
/// nothing in the repository), replacing an earlier copy; returns the file.
pub fn write_version(dir: &Path, label: &str, path: &str, data: &[u8]) -> std::io::Result<PathBuf> {
    let file = dir.join(label).join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    std::fs::write(&file, data)?;
    let mut perms = std::fs::metadata(&file)?.permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&file, perms)?;
    Ok(file)
}

/// Removes this run's folder.
pub fn remove(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Removes other runs' folders older than a day.
pub fn sweep_stale() {
    let own = session_dir();
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let named = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(PREFIX))
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > STALE);
        if named && old && path != own {
            remove(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_version_is_written_read_only_and_can_be_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let file = super::write_version(dir.path(), "HEAD", "src/a.txt", b"one").unwrap();
        assert_eq!(file, dir.path().join("HEAD/src/a.txt"));
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
        let again = super::write_version(dir.path(), "HEAD", "src/a.txt", b"two").unwrap();
        assert_eq!(std::fs::read(again).unwrap(), b"two");
        super::remove(dir.path());
        assert!(!dir.path().exists());
    }
}
