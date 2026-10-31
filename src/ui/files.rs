//! File pane items — changed files, the working-tree row, then the history
//! commits of the graph, then `… more` — and their selection, which drives
//! the diff pane.

use super::App;
use crate::git::diff::DiffSource;
use crate::git::{Commit, FileEntry};

/// A history commit's relation to the upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// On both.
    Shared,
    /// Local only: push sends it (`↑`).
    Unpushed,
    /// Upstream only: pull brings it in (`↓`).
    Incoming,
}

#[derive(Debug, Clone, Copy)]
pub enum FileItem<'a> {
    File(&'a FileEntry),
    /// The working tree above HEAD: a summary of every uncommitted change.
    WorkTree,
    /// A history commit and how it relates to the upstream.
    Commit(&'a Commit, Mark),
    /// Reads more history when selected.
    More,
}

impl FileItem<'_> {
    /// Selection identity kept across rescans and history reloads.
    pub fn key(&self) -> String {
        match self {
            FileItem::File(f) => format!("f:{}", f.path),
            FileItem::WorkTree => "wt".to_string(),
            FileItem::Commit(c, _) => format!("c:{}", c.hash),
            FileItem::More => "more".to_string(),
        }
    }

    fn source(&self) -> Option<DiffSource> {
        match self {
            FileItem::File(f) => Some(DiffSource::File((*f).clone())),
            FileItem::WorkTree => Some(DiffSource::WorkTree),
            FileItem::Commit(c, _) => Some(DiffSource::Commit((*c).clone())),
            FileItem::More => None,
        }
    }
}

impl App {
    /// Selectable items of the file pane, in display order.
    pub fn file_items(&self) -> Vec<FileItem<'_>> {
        let Some(status) = self.current_tree().and_then(|t| t.status.as_ref()) else {
            return Vec::new();
        };
        let mut items: Vec<FileItem> = status.files.iter().map(FileItem::File).collect();
        if !status.is_clean() {
            items.push(FileItem::WorkTree);
        }
        if let Some(h) = self.current_history() {
            let unpushed = |hash: &str| status.unpushed.iter().any(|c| c.hash == hash);
            items.extend(h.rows.iter().map(|r| {
                let mark = if r.incoming {
                    Mark::Incoming
                } else if unpushed(&r.commit.hash) {
                    Mark::Unpushed
                } else {
                    Mark::Shared
                };
                FileItem::Commit(&r.commit, mark)
            }));
            if h.more {
                items.push(FileItem::More);
            }
        }
        items
    }

    /// The changed file under the file cursor (not a commit or `… more`).
    /// Never a file of the commit view: those are past versions, and
    /// discarding or opening one would act on the working tree's file.
    pub(crate) fn selected_file(&self) -> Option<FileEntry> {
        if self.commit_view.is_some() {
            return None;
        }
        match self.file_items().get(self.file_sel)? {
            FileItem::File(f) => Some((*f).clone()),
            _ => None,
        }
    }

    /// History of the selected working tree, once loaded for it.
    pub(crate) fn current_history(&self) -> Option<&crate::git::history::History> {
        let target = self.current_target()?;
        if self.history.target.as_deref() != Some(target.as_path()) {
            return None;
        }
        self.history.history.as_ref()
    }

    /// Re-reads the items after a scan or a target change: keeps the cursor on
    /// the same item when it is still listed, then reloads the diff.
    pub(crate) fn refresh_files(&mut self) {
        self.ensure_history();
        // A commit's files do not change; another working tree closes them.
        if let Some(view) = &self.commit_view {
            if self.current_target().as_ref() == Some(&view.target) {
                return;
            }
            self.commit_view = None;
        }
        let items = self.file_items();
        let pos = self
            .file_key
            .as_ref()
            .and_then(|key| items.iter().position(|i| &i.key() == key));
        let len = items.len();
        drop(items);
        self.file_sel = pos.unwrap_or(self.file_sel).min(len.saturating_sub(1));
        self.select_file(self.file_sel);
    }

    /// After the history changed: keeps the cursor on the same item without
    /// reloading its diff; picks an item only when the old one is gone.
    pub(crate) fn keep_file_selection(&mut self) {
        let items = self.file_items();
        let more_loaded = self.file_key.as_deref() == Some("more")
            && items
                .get(self.file_sel)
                .is_some_and(|i| !matches!(i, FileItem::More));
        let pos = self
            .file_key
            .as_ref()
            .and_then(|key| items.iter().position(|i| &i.key() == key));
        let len = items.len();
        drop(items);
        // The commit view keeps its diff; `←` finds the commit again.
        if self.commit_view.is_some() {
            if let Some(i) = pos {
                self.file_sel = i;
            }
            return;
        }
        // After `… more` loaded, the cursor stays put: on the first new commit.
        if more_loaded {
            self.select_file(self.file_sel);
            return;
        }
        match pos {
            Some(i) => self.file_sel = i,
            None => {
                self.file_sel = self.file_sel.min(len.saturating_sub(1));
                self.select_file(self.file_sel);
            }
        }
    }

    /// Moves the file cursor and loads that item's diff (`… more` reads more
    /// history instead).
    pub(crate) fn select_file(&mut self, index: usize) {
        let Some(target) = self.current_target() else {
            self.diff.clear();
            return;
        };
        let picked = self.file_items().get(index).map(|i| (i.key(), i.source()));
        match picked {
            Some((key, source)) => {
                self.file_sel = index;
                self.file_key = Some(key.clone());
                match source {
                    Some(source) => {
                        let diff_key = format!("{}\0{key}", target.display());
                        self.load_diff(target, source, diff_key);
                    }
                    None => {
                        self.diff.clear();
                        self.load_more_history();
                    }
                }
            }
            None => {
                self.file_sel = 0;
                self.file_key = None;
                self.diff.clear();
            }
        }
    }
}
