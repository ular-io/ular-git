//! The file pane's commit view: `→` on a history commit lists the files it
//! changed, read in the background. Row 0 is the commit itself (its whole
//! `git show`), where the cursor starts; each file below shows its diff alone.
//! `←` returns to the changes and history with the cursor on the commit.
//! Rescans and history reloads leave a commit's view alone; selecting
//! another working tree closes it.
//!
//! `→` on the Uncommitted Changes row opens the same view over the working
//! tree: its files against HEAD, re-read on every rescan (the cursor starts
//! on row 0 like a commit's and stays on its file), closed once the tree is
//! clean.

use super::components::list_nav;
use super::files::FileItem;
use super::App;
use crate::git::commit_files::{self, CommitFile};
use crate::git::diff::DiffSource;
use crate::git::{Commit, FileEntry};
use crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;

/// What the view lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewOf {
    Commit(Commit),
    /// The uncommitted changes.
    WorkTree,
}

#[derive(Debug)]
pub struct CommitView {
    /// Working tree the view was opened from.
    pub target: PathBuf,
    pub of: ViewOf,
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

    pub fn commit(&self) -> Option<&Commit> {
        match &self.of {
            ViewOf::Commit(c) => Some(c),
            ViewOf::WorkTree => None,
        }
    }

    /// Diff identity of the row under the cursor and what it shows.
    fn row_source(&self) -> (String, DiffSource) {
        match (&self.of, self.selected()) {
            (ViewOf::Commit(c), Some(f)) => (
                format!("cf:{}:{}", c.hash, f.path),
                DiffSource::CommitFile(c.clone(), f.clone()),
            ),
            (ViewOf::Commit(c), None) => (format!("c:{}", c.hash), DiffSource::Commit(c.clone())),
            (ViewOf::WorkTree, Some(f)) => (
                format!("wf:{}", f.path),
                DiffSource::WorkTreeFile(f.clone()),
            ),
            (ViewOf::WorkTree, None) => ("wt".into(), DiffSource::WorkTree),
        }
    }
}

fn load_files(
    target: &std::path::Path,
    of: &ViewOf,
    big_file: u64,
) -> Result<Vec<CommitFile>, String> {
    match of {
        ViewOf::Commit(c) => commit_files::load(target, &c.hash, big_file),
        ViewOf::WorkTree => commit_files::load_worktree(target, big_file),
    }
    .map_err(|e| e.to_string())
}

impl App {
    /// `→` on a history commit or the Uncommitted Changes row: lists its
    /// files. Returns whether the cursor was on one.
    pub(crate) fn open_commit_files(&mut self) -> bool {
        let of = match self.file_items().get(self.file_sel) {
            Some(FileItem::Commit(c, _)) => ViewOf::Commit((*c).clone()),
            Some(FileItem::WorkTree) => ViewOf::WorkTree,
            _ => return false,
        };
        let Some(target) = self.current_target() else {
            return false;
        };
        // Row 0 first: the diff the history row already showed.
        self.commit_view = Some(CommitView {
            target: target.clone(),
            of,
            files: None,
            error: None,
            sel: 0,
        });
        *self.commit_list.borrow_mut().offset_mut() = 0;
        self.spawn_view_files();
        true
    }

    /// Reads the open view's files in the background.
    fn spawn_view_files(&mut self) {
        let Some(view) = self.commit_view.as_ref() else {
            return;
        };
        let (target, of) = (view.target.clone(), view.of.clone());
        let big_file = self.cfg.max_diff_file_bytes();
        self.bg.spawn(move || {
            let result = load_files(&target, &of, big_file);
            Box::new(move |app: &mut App| app.apply_commit_files(&target, &of, result))
        });
    }

    /// A rescan of the view's working tree: the uncommitted changes are read
    /// again; a commit's files never change.
    pub(crate) fn rescan_commit_files(&mut self) {
        if self
            .commit_view
            .as_ref()
            .is_some_and(|v| v.of == ViewOf::WorkTree)
        {
            self.spawn_view_files();
        }
    }

    fn apply_commit_files(
        &mut self,
        target: &std::path::Path,
        of: &ViewOf,
        result: Result<Vec<CommitFile>, String>,
    ) {
        // A result for a view since closed or replaced is dropped.
        let Some(view) = self
            .commit_view
            .as_mut()
            .filter(|v| v.target == target && &v.of == of)
        else {
            return;
        };
        let was = view.selected().map(|f| f.path.clone());
        match result {
            Ok(files) => {
                // A clean tree has nothing left to list.
                if files.is_empty() && *of == ViewOf::WorkTree {
                    self.close_commit_files();
                    return;
                }
                view.files = Some(files);
                view.error = None;
            }
            Err(err) => view.error = Some(err),
        }
        // A re-read keeps the cursor on its file while it is listed.
        let kept = was.and_then(|p| {
            view.files
                .as_ref()?
                .iter()
                .position(|f| f.path == p)
                .map(|i| i + 1)
        });
        let sel = kept.unwrap_or(view.sel).min(view.rows() - 1);
        self.select_commit_row(sel);
    }

    /// The working tree's entry for the file under the uncommitted changes'
    /// cursor (a commit's files are past versions: `None`).
    pub(crate) fn view_worktree_file(&self) -> Option<FileEntry> {
        let view = self.commit_view.as_ref()?;
        if view.of != ViewOf::WorkTree {
            return None;
        }
        let path = &view.selected()?.path;
        let status = self.current_tree()?.status.as_ref()?;
        status.files.iter().find(|f| &f.path == path).cloned()
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
        let (key, source) = view.row_source();
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
            // An uncommitted file opens in an app, like a changed file.
            KeyCode::Enter if self.open_with_file() => {}
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                self.focus = super::Pane::Diff;
            }
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
