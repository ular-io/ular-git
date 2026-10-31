//! The file pane's commit view: `→` on a history commit lists the files it
//! changed, read in the background. Row 0 is the commit itself (its whole
//! `git show`); the cursor starts on the first file, whose diff shows alone.
//! `←` returns to the changes and history with the cursor on the commit.
//! Rescans and history reloads leave the view alone; selecting another
//! working tree closes it.

use super::components::list_nav;
use super::files::FileItem;
use super::App;
use crate::git::commit_files::{self, CommitFile};
use crate::git::diff::DiffSource;
use crate::git::Commit;
use crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;

#[derive(Debug)]
pub struct CommitView {
    /// Working tree the commit was opened from.
    pub target: PathBuf,
    pub commit: Commit,
    /// `None` while loading.
    pub files: Option<Vec<CommitFile>>,
    pub error: Option<String>,
    /// Row under the cursor: 0 the commit, then its files.
    pub sel: usize,
}

impl CommitView {
    /// Selectable rows: the commit and its files.
    pub fn rows(&self) -> usize {
        1 + self.files.as_ref().map_or(0, Vec::len)
    }

    /// The file under the cursor (not the commit row).
    pub fn selected(&self) -> Option<&CommitFile> {
        self.files.as_ref()?.get(self.sel.checked_sub(1)?)
    }
}

impl App {
    /// `→` on a history commit: lists its files. Returns whether the cursor
    /// was on a commit.
    pub(crate) fn open_commit_files(&mut self) -> bool {
        let commit = match self.file_items().get(self.file_sel) {
            Some(FileItem::Commit(c, _)) => (*c).clone(),
            _ => return false,
        };
        let Some(target) = self.current_target() else {
            return false;
        };
        self.commit_view = Some(CommitView {
            target: target.clone(),
            commit: commit.clone(),
            files: None,
            error: None,
            sel: 1,
        });
        *self.commit_list.borrow_mut().offset_mut() = 0;
        self.bg.spawn(move || {
            let result = commit_files::load(&target, &commit.hash).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| app.apply_commit_files(&target, &commit.hash, result))
        });
        true
    }

    fn apply_commit_files(
        &mut self,
        target: &std::path::Path,
        hash: &str,
        result: Result<Vec<CommitFile>, String>,
    ) {
        // A result for a view since closed or replaced is dropped.
        let Some(view) = self
            .commit_view
            .as_mut()
            .filter(|v| v.target == target && v.commit.hash == hash)
        else {
            return;
        };
        match result {
            Ok(files) => view.files = Some(files),
            Err(err) => view.error = Some(err),
        }
        let sel = view.sel.min(view.rows() - 1);
        self.select_commit_row(sel);
    }

    /// `←`: back to the changes and history, the cursor on the commit (or
    /// where it was when the history has changed since).
    pub(crate) fn close_commit_files(&mut self) {
        self.commit_view = None;
        let items = self.file_items();
        let pos = self
            .file_key
            .as_ref()
            .and_then(|key| items.iter().position(|i| &i.key() == key));
        let len = items.len();
        drop(items);
        self.select_file(pos.unwrap_or(self.file_sel).min(len.saturating_sub(1)));
    }

    /// Moves the commit view's cursor and loads that row's diff: the whole
    /// commit on row 0, one file below it.
    pub(crate) fn select_commit_row(&mut self, index: usize) {
        let Some(view) = self.commit_view.as_mut() else {
            return;
        };
        view.sel = index.min(view.rows() - 1);
        let c = view.commit.clone();
        let (key, source) = match view.selected() {
            Some(f) => (
                format!("cf:{}:{}", c.hash, f.path),
                DiffSource::CommitFile(c, f.clone()),
            ),
            None => (format!("c:{}", c.hash), DiffSource::Commit(c)),
        };
        let target = view.target.clone();
        let diff_key = format!("{}\0{key}", target.display());
        self.load_diff(target, source, diff_key);
    }

    /// File pane keys while the commit view is open.
    pub(crate) fn on_key_commit_files(&mut self, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::PageUp => self.diff.page_by(-1),
            KeyCode::PageDown => self.diff.page_by(1),
            KeyCode::Left | KeyCode::Char('h') => self.close_commit_files(),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                self.focus = super::Pane::Diff;
            }
            KeyCode::Char('.') => self.toggle_wrap(),
            _ => {
                let Some(mv) = list_nav::list_move(key, true) else {
                    return false;
                };
                let Some(view) = self.commit_view.as_ref() else {
                    return false;
                };
                let page = self.file_page.get().max(1);
                let next = list_nav::apply(view.sel, view.rows(), mv, page);
                if next != view.sel {
                    self.select_commit_row(next);
                }
            }
        }
        true
    }
}
