//! Scan scheduling and the status cache.
//!
//! - Launch shows the cached status of every working tree, then rescans all.
//! - A rescan of everything repeats every `refresh_interval_secs` and on
//!   `ctrl+u` or terminal focus.
//! - Settling the cursor on a repository rescans just that one, ahead of the
//!   queue, when its status is older than [`FOCUS_STALE`].
//! - One scan per path is in flight at a time; a request while it runs is
//!   remembered and replayed once it finishes.

use super::{App, Level};
use crate::cache::{self, CachedStatus};
use crate::git::{self, RepoStatus};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Cursor rest time before the focused repository is rescanned.
const FOCUS_DEBOUNCE: Duration = Duration::from_millis(300);
/// Focused status younger than this is not rescanned.
const FOCUS_STALE: Duration = Duration::from_secs(3);
/// Terminal focus within this long after a full scan does not start another.
const REFOCUS_MIN: Duration = Duration::from_secs(5);

/// What is known about one working tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeState {
    pub status: Option<RepoStatus>,
    /// Unix ms of the scan behind `status` (0 = never scanned).
    pub scanned_at_ms: u64,
    /// Error of the latest scan; the previous status stays shown.
    pub error: Option<String>,
    /// `status` came from the cache and this session has not rescanned it yet.
    pub from_cache: bool,
}

impl TreeState {
    pub fn loaded(status: RepoStatus, scanned_at_ms: u64) -> Self {
        TreeState {
            status: Some(status),
            scanned_at_ms,
            error: None,
            from_cache: false,
        }
    }
}

#[derive(Debug)]
pub struct ScanState {
    in_flight: HashSet<PathBuf>,
    again: HashSet<PathBuf>,
    last_full: Option<Instant>,
    focus_due: Option<Instant>,
    cache_dirty: bool,
}

impl ScanState {
    pub fn new() -> Self {
        ScanState {
            in_flight: HashSet::new(),
            again: HashSet::new(),
            last_full: None,
            focus_due: None,
            cache_dirty: false,
        }
    }

    pub fn is_scanning(&self, path: &Path) -> bool {
        self.in_flight.contains(path)
    }

    pub fn busy(&self) -> bool {
        !self.in_flight.is_empty()
    }
}

impl Default for ScanState {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Rescans every repository (their worktrees follow as each finishes).
    pub fn scan_all(&mut self) {
        self.scan.last_full = Some(Instant::now());
        let paths: Vec<PathBuf> = self.store.repos.iter().map(|r| r.path.clone()).collect();
        for path in paths {
            self.request_scan(path, false);
        }
    }

    /// Queues a scan of one working tree unless one is already running.
    pub fn request_scan(&mut self, path: PathBuf, urgent: bool) {
        if self.scan.in_flight.contains(&path) {
            self.scan.again.insert(path);
            return;
        }
        self.scan.in_flight.insert(path.clone());
        let main = self.store.contains(&path);
        self.bg.submit(urgent, move || {
            let result = git::scan(&path, main).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| app.apply_scan(path, main, result))
        });
    }

    fn apply_scan(&mut self, path: PathBuf, main: bool, result: Result<RepoStatus, String>) {
        self.scan.in_flight.remove(&path);
        // The repository was removed from the list, or the worktree was
        // removed, while this scan ran.
        if !self.is_known_tree(&path) {
            self.scan.again.remove(&path);
            return;
        }
        let entry = self.statuses.entry(path.clone()).or_default();
        let old_worktrees: Vec<PathBuf> = entry
            .status
            .as_ref()
            .map(|s| s.worktrees.iter().map(|w| w.path.clone()).collect())
            .unwrap_or_default();
        match result {
            Ok(status) => {
                *entry = TreeState::loaded(status, cache::now_ms());
            }
            Err(err) => {
                entry.error = Some(err);
                entry.from_cache = false;
                entry.scanned_at_ms = cache::now_ms();
            }
        }
        self.scan.cache_dirty = true;
        if main {
            let new_worktrees: Vec<PathBuf> = self.statuses[&path]
                .status
                .as_ref()
                .map(|s| s.worktrees.iter().map(|w| w.path.clone()).collect())
                .unwrap_or_default();
            for gone in old_worktrees.iter().filter(|p| !new_worktrees.contains(p)) {
                self.statuses.remove(gone);
            }
            for wt in new_worktrees {
                self.request_scan(wt, false);
            }
        }
        if self.scan.again.remove(&path) {
            self.request_scan(path.clone(), false);
        }
        self.rebuild_rows();
        if self.current_target().as_deref() == Some(path.as_path()) {
            self.refresh_files();
        }
    }

    /// A listed repository, or a worktree its last scan reported.
    pub(crate) fn is_known_tree(&self, path: &Path) -> bool {
        self.store.contains(path)
            || self.store.repos.iter().any(|r| {
                self.statuses
                    .get(&r.path)
                    .and_then(|t| t.status.as_ref())
                    .is_some_and(|s| s.worktrees.iter().any(|w| w.path == path))
            })
    }

    /// Runs after each batch of applied patches.
    pub(crate) fn after_background(&mut self) {
        if !self.scan.busy() && self.scan.cache_dirty {
            self.save_status_cache();
        }
    }

    /// Periodic work driven by the loop: focus rescans and the auto rescan.
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut changed = self.expire_notice(now);
        if self.scan.focus_due.is_some_and(|due| due <= now) {
            self.scan.focus_due = None;
            if let Some(target) = self.current_target() {
                let stale = self.statuses.get(&target).is_none_or(|t| {
                    t.from_cache
                        || cache::now_ms().saturating_sub(t.scanned_at_ms)
                            > FOCUS_STALE.as_millis() as u64
                });
                if stale && !self.scan.is_scanning(&target) {
                    self.request_scan(target, true);
                    changed = true;
                }
            }
        }
        if let Some(interval) = self.auto_interval() {
            let due = self.scan.last_full.is_none_or(|t| now >= t + interval);
            if due && !self.scan.busy() {
                self.scan_all();
                changed = true;
            }
        }
        changed
    }

    fn auto_interval(&self) -> Option<Duration> {
        (self.cfg.refresh_interval_secs > 0)
            .then(|| Duration::from_secs(self.cfg.refresh_interval_secs))
    }

    /// Earliest moment [`App::tick`] has something to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        let auto = self
            .auto_interval()
            .and_then(|i| self.scan.last_full.map(|t| t + i));
        [
            self.notice.as_ref().map(|n| n.until),
            self.scan.focus_due,
            auto,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// The selected working tree changed: reset the file and diff panes and
    /// schedule the focus rescan.
    pub(crate) fn on_target_changed(&mut self) {
        self.commit_view = None;
        self.file_sel = 0;
        self.file_key = None;
        *self.file_list.borrow_mut().offset_mut() = 0;
        self.diff.clear();
        self.scan.focus_due = Some(Instant::now() + FOCUS_DEBOUNCE);
        self.refresh_files();
    }

    /// `ctrl+u` / terminal focus: rescan everything.
    pub fn refresh_all(&mut self, from_focus: bool) {
        if from_focus
            && self
                .scan
                .last_full
                .is_some_and(|t| t.elapsed() < REFOCUS_MIN)
        {
            return;
        }
        self.scan_all();
        if !from_focus {
            let n = self.store.repos.len();
            self.notify(Level::Info, format!("Rescanning {n} repositories"));
        }
    }

    pub(crate) fn load_status_cache(&mut self) {
        for (path, cached) in cache::load(&crate::config::status_cache_path()) {
            self.statuses.insert(
                path,
                TreeState {
                    status: Some(cached.status),
                    scanned_at_ms: cached.scanned_at_ms,
                    error: None,
                    from_cache: true,
                },
            );
        }
    }

    /// Saves the status of listed repositories and their worktrees.
    pub(crate) fn save_status_cache(&mut self) {
        self.scan.cache_dirty = false;
        if cfg!(test) {
            return;
        }
        let mut keep: HashSet<&Path> = HashSet::new();
        for repo in &self.store.repos {
            keep.insert(&repo.path);
            if let Some(s) = self
                .statuses
                .get(&repo.path)
                .and_then(|t| t.status.as_ref())
            {
                keep.extend(s.worktrees.iter().map(|w| w.path.as_path()));
            }
        }
        let entries: BTreeMap<PathBuf, CachedStatus> = self
            .statuses
            .iter()
            .filter(|(p, _)| keep.contains(p.as_path()))
            .filter_map(|(p, t)| {
                Some((
                    p.clone(),
                    CachedStatus {
                        status: t.status.clone()?,
                        scanned_at_ms: t.scanned_at_ms,
                    },
                ))
            })
            .collect();
        let _ = cache::save(&crate::config::status_cache_path(), &entries);
    }
}
