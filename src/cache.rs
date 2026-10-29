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
/// 2: `Commit::when` (relative date) became `Commit::date` (`yyyy-mm-dd`).
const CACHE_VERSION: u32 = 2;

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

/// Compact age label: `now`, `42s`, `5m`, `3h`, `2d`.
pub fn age_label(scanned_at_ms: u64, now_ms: u64) -> String {
    let secs = now_ms.saturating_sub(scanned_at_ms) / 1000;
    match secs {
        0..=4 => "now".to_string(),
        5..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86_399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
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

    #[test]
    fn age_labels_step_through_units() {
        assert_eq!(age_label(0, 3_000), "now");
        assert_eq!(age_label(0, 42_000), "42s");
        assert_eq!(age_label(0, 300_000), "5m");
        assert_eq!(age_label(0, 3 * 3_600_000), "3h");
        assert_eq!(age_label(10, 0), "now");
    }
}
