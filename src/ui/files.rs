//! File pane items — changed files, then the history commits of the graph,
//! then `… more` — and their selection, which drives the diff pane.

use super::App;
use crate::git::diff::DiffSource;
use crate::git::{Commit, FileEntry};

#[derive(Debug, Clone, Copy)]
pub enum FileItem<'a> {
    File(&'a FileEntry),
    /// A history commit; `unpushed` = not on the upstream yet.
    Commit(&'a Commit, bool),
    /// Reads more history when selected.
    More,
}

impl FileItem<'_> {
    /// Selection identity kept across rescans and history reloads.
    pub fn key(&self) -> String {
        match self {
            FileItem::File(f) => format!("f:{}", f.path),
            FileItem::Commit(c, _) => format!("c:{}", c.hash),
            FileItem::More => "more".to_string(),
        }
    }

    fn source(&self) -> Option<DiffSource> {
        match self {
            FileItem::File(f) => Some(DiffSource::File((*f).clone())),
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
        if let Some(h) = self.current_history() {
            let unpushed = |hash: &str| status.unpushed.iter().any(|c| c.hash == hash);
            items.extend(
                h.rows
                    .iter()
                    .map(|r| FileItem::Commit(&r.commit, unpushed(&r.commit.hash))),
            );
            if h.more {
                items.push(FileItem::More);
            }
        }
        items
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
