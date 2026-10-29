//! File pane items (changed files, then unpushed commits) and their selection,
//! which drives the diff pane.

use super::App;
use crate::git::diff::DiffSource;
use crate::git::{Commit, FileEntry};

#[derive(Debug, Clone, Copy)]
pub enum FileItem<'a> {
    File(&'a FileEntry),
    Commit(&'a Commit),
}

impl FileItem<'_> {
    /// Selection identity kept across rescans.
    pub fn key(&self) -> String {
        match self {
            FileItem::File(f) => format!("f:{}", f.path),
            FileItem::Commit(c) => format!("c:{}", c.hash),
        }
    }

    fn source(&self) -> DiffSource {
        match self {
            FileItem::File(f) => DiffSource::File((*f).clone()),
            FileItem::Commit(c) => DiffSource::Commit((*c).clone()),
        }
    }
}

impl App {
    pub fn file_items(&self) -> Vec<FileItem<'_>> {
        let Some(status) = self.current_tree().and_then(|t| t.status.as_ref()) else {
            return Vec::new();
        };
        status
            .files
            .iter()
            .map(FileItem::File)
            .chain(status.unpushed.iter().map(FileItem::Commit))
            .collect()
    }

    /// Re-reads the items after a scan or a target change: keeps the cursor on
    /// the same file when it is still listed, then reloads the diff.
    pub(crate) fn refresh_files(&mut self) {
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

    /// Moves the file cursor and loads that item's diff.
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
                let diff_key = format!("{}\0{key}", target.display());
                self.load_diff(target, source, diff_key);
            }
            None => {
                self.file_sel = 0;
                self.file_key = None;
                self.diff.clear();
            }
        }
    }
}
