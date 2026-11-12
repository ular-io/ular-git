//! `ular-git <dir>`: the repository opened from the command line. A listed
//! one is selected where it is; one that is not listed is shown for this
//! session only, first in the list and marked `*`, until `+` or `ctrl+e`
//! adds it.
//!
//! The unlisted repository sits in the in-memory store at index 0, so rows,
//! scans, fetches, and the status cache treat it like any other. It is never
//! written: list writes run on the file's latest state ([`App::listed_store`]
//! for the in-memory test path), and every store accepted from the file gets
//! it attached again ([`App::attach_unlisted`]).

use super::tree::{self, RowKey};
use super::{App, Level};
use crate::store::{RepoEntry, RepoStore};
use std::path::{Path, PathBuf};

/// The folder given on the command line, resolved before the TUI starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    /// Main working tree root of the repository.
    pub repo: PathBuf,
    /// Working tree to select: `repo`, or one of its linked worktrees.
    pub tree: PathBuf,
}

impl App {
    /// Shows and selects `open` (`App::new`, before the first scan).
    pub(crate) fn open_tree(&mut self, open: OpenTarget) {
        self.launch_dir = Some(open.tree.clone());
        if !self.store.contains(&open.repo) {
            self.unlisted = Some(open.repo.clone());
            self.store.repos.insert(
                0,
                RepoEntry {
                    path: open.repo.clone(),
                    group: None,
                },
            );
        }
        if open.tree == open.repo {
            self.select_tree(&open.repo, RowKey::Repo(open.repo.clone()));
        } else if let Some(wt) = self.listed_worktree(&open.repo, &open.tree) {
            self.select_tree(&wt, RowKey::Worktree(wt.clone()));
        } else {
            // Worktree rows come from the repository's scan (or the cache);
            // `follow_open_worktree` moves there once the row exists.
            self.select_tree(&open.repo, RowKey::Repo(open.repo.clone()));
            self.open_pending = Some(open);
        }
        self.hint_unlisted();
    }

    /// Selects `key`, keeping `target` visible even inside a folded group
    /// (the same local exception a peer's fold gets).
    fn select_tree(&mut self, target: &Path, key: RowKey) {
        let before = self.current_target();
        self.rows = tree::build_rows(
            &self.store,
            &self.statuses,
            &self.filter.value,
            self.pending_only,
            Some(target),
        );
        self.place_cursor(Some(&key), None, before);
    }

    /// The linked worktree of `repo` at `tree` (canonical), as its last scan
    /// names it.
    fn listed_worktree(&self, repo: &Path, tree: &Path) -> Option<PathBuf> {
        let status = self.statuses.get(repo)?.status.as_ref()?;
        status
            .worktrees
            .iter()
            .map(|w| &w.path)
            .find(|p| p.as_path() == tree || std::fs::canonicalize(p).is_ok_and(|c| c == tree))
            .cloned()
    }

    /// After a rebuild: moves to the opened worktree once its row exists,
    /// unless the cursor has left its repository row meanwhile.
    pub(super) fn follow_open_worktree(&mut self) {
        let Some(open) = self.open_pending.clone() else {
            return;
        };
        if self.current_target().as_deref() != Some(open.repo.as_path()) {
            self.open_pending = None;
            return;
        }
        if let Some(wt) = self.listed_worktree(&open.repo, &open.tree) {
            self.open_pending = None;
            self.select_tree(&wt, RowKey::Worktree(wt.clone()));
        }
    }

    /// Attaches the unlisted repository to a store read from the file, or
    /// forgets it once the file lists it (added here or by another instance).
    pub(super) fn attach_unlisted(&mut self, store: &mut RepoStore) {
        let Some(path) = &self.unlisted else {
            return;
        };
        if store.contains(path) {
            self.unlisted = None;
            return;
        }
        store.repos.insert(
            0,
            RepoEntry {
                path: path.clone(),
                group: None,
            },
        );
    }

    /// The store as the file holds it: without the unlisted repository.
    pub(super) fn listed_store(&self) -> RepoStore {
        let mut store = self.store.clone();
        if let Some(path) = &self.unlisted {
            store.repos.retain(|r| &r.path != path);
        }
        store
    }

    /// Shown this session but not in the list (drawn with `*`).
    pub(crate) fn is_unlisted(&self, path: &Path) -> bool {
        self.unlisted.as_deref() == Some(path)
    }

    /// `ctrl+d` on the unlisted repository: the file has nothing to remove,
    /// so it leaves the screen for the rest of the session.
    pub(crate) fn hide_unlisted(&mut self) {
        let store = self.listed_store();
        let Some(path) = self.unlisted.take() else {
            return;
        };
        self.accept_store(store);
        let shown = crate::config::collapse_home(&path);
        self.notify(Level::Info, format!("Hid {shown}; it was not in the list"));
    }

    /// Footer hint while the selection is on the unlisted repository or one
    /// of its worktrees.
    pub(super) fn hint_unlisted(&mut self) {
        let on_unlisted = self
            .current_repo_index()
            .and_then(|i| self.store.repos.get(i))
            .is_some_and(|r| self.is_unlisted(&r.path));
        if on_unlisted {
            self.notify(
                Level::Info,
                "* Not in the list: + adds it, ctrl+e adds it to a group",
            );
        }
    }
}

#[cfg(test)]
#[path = "unlisted_tests.rs"]
mod tests;
