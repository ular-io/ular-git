//! Cooperative transactions and directory notifications for the shared list.

use super::RepoStore;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

fn acquire_lock(lock: &std::fs::File, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Timed out waiting for repository list lock",
                    )
                    .into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(std::fs::TryLockError::Error(err)) => return Err(err.into()),
        }
    }
}

impl RepoStore {
    /// The separate lock survives atomic replacement of the JSON file. Always
    /// replay the requested operation on disk state, never on a stale UI copy.
    pub fn update<T>(
        path: &Path,
        allow_create: bool,
        edit: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<(Self, T)> {
        Self::update_with_timeout(path, allow_create, Duration::from_secs(5), edit)
    }

    pub fn update_with_timeout<T>(
        path: &Path,
        allow_create: bool,
        timeout: Duration,
        edit: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<(Self, T)> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
        std::fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        acquire_lock(&lock, timeout)?;
        let mut store = if allow_create {
            Self::load(path)?
        } else {
            Self::read_existing(path)?
        };
        let before = store.clone();
        let value = edit(&mut store)?;
        if store != before || !path.exists() {
            store.save(path)?;
        }
        Ok((store, value))
    }

    pub fn require_group(&self, group: Option<&str>) -> anyhow::Result<()> {
        if let Some(name) = group {
            anyhow::ensure!(self.group_exists(name), "Group \"{name}\" no longer exists");
        }
        Ok(())
    }

    pub fn require_repo(&self, path: &Path) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.contains(path),
            "Repository is no longer listed: {}",
            path.display()
        );
        Ok(())
    }
}

/// Watch the parent, because writers and editors replace the file on save.
/// Access events are ignored so a reload cannot trigger another reload.
pub struct StoreWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
    target: PathBuf,
}

impl StoreWatcher {
    pub fn new(path: &Path) -> anyhow::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
        std::fs::create_dir_all(parent)?;
        let parent = std::fs::canonicalize(parent)?;
        let target = parent.join(
            path.file_name()
                .ok_or_else(|| anyhow::anyhow!("No filename"))?,
        );
        let (tx, events) = channel();
        let mut watcher = notify::recommended_watcher(tx)?;
        watcher.watch(&parent, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: watcher,
            events,
            target,
        })
    }

    /// Coalesce queued changes; directory-only events and overflow signals
    /// also require a reload, even when no individual file path is supplied.
    pub fn changed(&self) -> anyhow::Result<bool> {
        let mut changed = false;
        for result in self.events.try_iter() {
            let event = result?;
            if !matches!(event.kind, EventKind::Access(_))
                && (event.need_rescan()
                    || event.paths.is_empty()
                    || event
                        .paths
                        .iter()
                        .any(|p| p == &self.target || Some(p.as_path()) == self.target.parent()))
            {
                changed = true;
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests;
