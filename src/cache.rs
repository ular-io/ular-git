//! Last known status of every working tree, persisted so the next launch shows
//! the list instantly while a fresh scan runs in the background.
//!
//! The cache is only a head start: every entry is rescanned on launch, and a
//! cache from another version is discarded instead of migrated.

use crate::git::RepoStatus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bump when the meaning of a serialized field changes.
/// 3: `Commit` lost its date field (the file list shows only the subject).
const CACHE_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedStatus {
    pub status: RepoStatus,
    /// Unix milliseconds of the scan that produced `status`.
    pub scanned_at_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: BTreeMap<PathBuf, CachedStatus>,
}

pub fn load(path: &Path) -> BTreeMap<PathBuf, CachedStatus> {
    std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<CacheFile>(&data).ok())
        .filter(|f| f.version == CACHE_VERSION)
        .map(|f| f.entries)
        .unwrap_or_default()
}

pub fn save(path: &Path, entries: &BTreeMap<PathBuf, CachedStatus>) -> anyhow::Result<()> {
    let file = CacheFile {
        version: CACHE_VERSION,
        entries: entries.clone(),
    };
    crate::config::write_atomic(path, &serde_json::to_vec(&file)?)?;
    Ok(())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_other_versions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c/status.json");
        let mut entries = BTreeMap::new();
        entries.insert(
            PathBuf::from("/r"),
            CachedStatus {
                status: RepoStatus::default(),
                scanned_at_ms: 7,
            },
        );
        save(&path, &entries).unwrap();
        assert_eq!(load(&path), entries);
        std::fs::write(&path, r#"{"version":999,"entries":{}}"#).unwrap();
        assert!(load(&path).is_empty());
    }
}
