//! History pane section state: the commit graph of the selected working tree,
//! loaded in the background (one load in flight; a newer request replaces a
//! queued one; results for an older request are dropped).

use super::App;
use crate::git::history::{self, History, FIRST_PAGE};
use std::path::PathBuf;

/// Which refs the history labels (`>` cycles them in this order). The last
/// choice is kept in `view.json` across launches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefLabels {
    #[default]
    None,
    Branches,
    BranchesAndTags,
    Tags,
}

impl RefLabels {
    pub fn next(self) -> Self {
        match self {
            RefLabels::None => RefLabels::Branches,
            RefLabels::Branches => RefLabels::BranchesAndTags,
            RefLabels::BranchesAndTags => RefLabels::Tags,
            RefLabels::Tags => RefLabels::None,
        }
    }

    pub fn branches(self) -> bool {
        matches!(self, RefLabels::Branches | RefLabels::BranchesAndTags)
    }

    pub fn tags(self) -> bool {
        matches!(self, RefLabels::BranchesAndTags | RefLabels::Tags)
    }
}

/// What history rows show between the graph and the subject (`.` on the
/// Files pane cycles them in this order), whatever the pane width. Kept in
/// `view.json` across launches. `None` (stored as `none`) shows subjects
/// only and the selected commit's date and author on a second line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitMeta {
    #[default]
    Date,
    DateAndAuthor,
    Author,
    None,
}

impl CommitMeta {
    pub fn next(self) -> Self {
        match self {
            CommitMeta::Date => CommitMeta::DateAndAuthor,
            CommitMeta::DateAndAuthor => CommitMeta::Author,
            CommitMeta::Author => CommitMeta::None,
            CommitMeta::None => CommitMeta::Date,
        }
    }

    pub fn date(self) -> bool {
        matches!(self, CommitMeta::Date | CommitMeta::DateAndAuthor)
    }

    pub fn author(self) -> bool {
        matches!(self, CommitMeta::DateAndAuthor | CommitMeta::Author)
    }

    /// The selected commit gets a second line with its date and author.
    pub fn details_on_selection(self) -> bool {
        self == CommitMeta::None
    }
}

/// View choices remembered across launches (`~/.config/ular-git/view.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedView {
    #[serde(default)]
    pub history_labels: RefLabels,
    #[serde(default)]
    pub history_meta: CommitMeta,
    /// Side-by-side diffs on wide panes (`s`).
    #[serde(default = "on")]
    pub split_diff: bool,
    /// Panes shown side by side (`1` / `2` / `3`); the width may show fewer.
    #[serde(default = "all_panes")]
    pub pane_count: usize,
}

fn on() -> bool {
    true
}

fn all_panes() -> usize {
    3
}

impl Default for SavedView {
    fn default() -> Self {
        SavedView {
            history_labels: RefLabels::default(),
            history_meta: CommitMeta::default(),
            split_diff: true,
            pane_count: 3,
        }
    }
}

fn view_path() -> std::path::PathBuf {
    crate::config::config_base_dir().join("view.json")
}

/// The saved view choices; unit tests never read the user's file.
pub(crate) fn load_view() -> SavedView {
    if cfg!(test) {
        return SavedView::default();
    }
    std::fs::read_to_string(view_path())
        .ok()
        .and_then(|data| serde_json::from_str::<SavedView>(&data).ok())
        .unwrap_or_default()
}

impl App {
    /// Writes the current view choices to `view.json`.
    pub(crate) fn save_view(&self) {
        if cfg!(test) {
            return;
        }
        let view = SavedView {
            history_labels: self.history.labels,
            history_meta: self.history.meta,
            split_diff: self.diff.split,
            pane_count: self.pane_count,
        };
        if let Ok(data) = serde_json::to_string_pretty(&view) {
            let _ = crate::config::write_atomic(&view_path(), data.as_bytes());
        }
    }
}

#[derive(Debug, Default)]
pub struct HistoryView {
    /// Branch / tag labels on history rows.
    pub labels: RefLabels,
    /// Date and author on history rows.
    pub meta: CommitMeta,
    /// Working tree the history belongs to (loaded or loading).
    pub target: Option<PathBuf>,
    /// HEAD (short oid), ahead/behind counts, and whether the tree had
    /// changes when it was requested; a change (commit, pull, checkout, a
    /// fetch elsewhere, the first edit or the last commit) reloads.
    pub head: Option<String>,
    /// Commits requested.
    pub limit: usize,
    pub history: Option<History>,
    pub loading: bool,
    pub error: Option<String>,
    /// The history's commit count ([`history::count`]), counted apart from
    /// the list; `None` until it arrives.
    pub commits: Option<usize>,
    count_generation: u64,
    generation: u64,
    in_flight: bool,
    queued: Option<(PathBuf, usize, bool)>,
}

impl App {
    /// `>`: none → branches → branches and tags → tags → none.
    pub(crate) fn cycle_ref_labels(&mut self) {
        self.history.labels = self.history.labels.next();
        self.save_view();
        let text = match self.history.labels {
            RefLabels::None => "History labels hidden",
            RefLabels::Branches => "History labels: branches",
            RefLabels::BranchesAndTags => "History labels: branches and tags",
            RefLabels::Tags => "History labels: tags",
        };
        self.notify(super::Level::Info, text);
    }

    /// `.` on the Files pane: date → date and author → author → subject only
    /// (date and author on the selected commit's second line).
    pub(crate) fn cycle_commit_meta(&mut self) {
        self.history.meta = self.history.meta.next();
        self.save_view();
        let text = match self.history.meta {
            CommitMeta::Date => "History rows: date",
            CommitMeta::DateAndAuthor => "History rows: date and author",
            CommitMeta::Author => "History rows: author",
            CommitMeta::None => "History rows: subject, date and author when selected",
        };
        self.notify(super::Level::Info, text);
    }

    /// Loads the history of the selected working tree when it is not loaded
    /// for that tree and HEAD yet (target change, commit, pull, checkout).
    pub(crate) fn ensure_history(&mut self) {
        let Some(target) = self.current_target() else {
            self.history = HistoryView {
                labels: self.history.labels,
                count_generation: self.history.count_generation + 1,
                ..HistoryView::default()
            };
            return;
        };
        let head = self
            .current_tree()
            .and_then(|t| t.status.as_ref())
            .map(|s| {
                let b = &s.branch;
                format!(
                    "{}:{}:{}:{}",
                    b.oid.as_deref().unwrap_or(""),
                    b.ahead,
                    b.behind,
                    !s.is_clean()
                )
            });
        let same_tree = self.history.target.as_deref() == Some(target.as_path());
        if same_tree && self.history.head == head {
            return;
        }
        let limit = if same_tree {
            self.history.limit.max(FIRST_PAGE)
        } else {
            FIRST_PAGE
        };
        if !same_tree {
            self.history.history = None;
            self.history.error = None;
            self.history.commits = None;
        }
        self.history.head = head;
        self.request_history(target.clone(), limit);
        self.request_count(target);
    }

    /// Counts the history's commits on its own thread, so a long history
    /// never holds up the list; the latest request wins. The count shown
    /// stays until the new one arrives.
    fn request_count(&mut self, target: PathBuf) {
        self.history.count_generation += 1;
        let generation = self.history.count_generation;
        self.bg.spawn(move || {
            let commits = history::count(&target);
            Box::new(move |app: &mut App| {
                if app.history.count_generation == generation {
                    app.history.commits = commits;
                }
            })
        });
    }

    /// `… more`: reads twice as many commits (the graph needs one pass from
    /// HEAD, so the page is re-read rather than appended).
    pub(crate) fn load_more_history(&mut self) {
        let Some(target) = self.history.target.clone() else {
            return;
        };
        if self.history.loading {
            return;
        }
        let limit = self.history.limit.max(FIRST_PAGE) * 2;
        self.request_history(target, limit);
    }

    fn request_history(&mut self, target: PathBuf, limit: usize) {
        // A tree with changes gets the working-tree node above HEAD.
        let worktree = self
            .current_tree()
            .and_then(|t| t.status.as_ref())
            .is_some_and(|s| !s.is_clean());
        let view = &mut self.history;
        view.target = Some(target.clone());
        view.limit = limit;
        view.loading = true;
        view.generation += 1;
        if view.in_flight {
            view.queued = Some((target, limit, worktree));
            return;
        }
        self.spawn_history(target, limit, worktree);
    }

    fn spawn_history(&mut self, target: PathBuf, limit: usize, worktree: bool) {
        self.history.in_flight = true;
        let generation = self.history.generation;
        self.bg.spawn(move || {
            let result = history::load(&target, limit, worktree).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| app.apply_history(generation, result))
        });
    }

    fn apply_history(&mut self, generation: u64, result: Result<History, String>) {
        self.history.in_flight = false;
        if let Some((target, limit, worktree)) = self.history.queued.take() {
            self.spawn_history(target, limit, worktree);
            return;
        }
        if generation != self.history.generation {
            return;
        }
        self.history.loading = false;
        match result {
            Ok(h) => {
                self.history.history = Some(h);
                self.history.error = None;
            }
            Err(err) => self.history.error = Some(err),
        }
        self.keep_file_selection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cycle_back_to_none_and_save_by_name() {
        let mut l = RefLabels::None;
        for _ in 0..4 {
            l = l.next();
        }
        assert_eq!(l, RefLabels::None);
        let mut m = CommitMeta::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(m);
            m = m.next();
        }
        use CommitMeta::*;
        assert_eq!(seen, [Date, DateAndAuthor, Author, None]);
        assert_eq!(m, Date);
        let view = SavedView {
            history_labels: RefLabels::BranchesAndTags,
            history_meta: CommitMeta::Author,
            split_diff: false,
            pane_count: 2,
        };
        let json = serde_json::to_string(&view).unwrap();
        assert_eq!(
            json,
            r#"{"history_labels":"branches_and_tags","history_meta":"author","split_diff":false,"pane_count":2}"#
        );
        // A file from before split_diff existed keeps side-by-side on.
        let back: SavedView = serde_json::from_str(r#"{"history_labels":"tags"}"#).unwrap();
        assert_eq!(back.history_labels, RefLabels::Tags);
        assert_eq!(back.history_meta, CommitMeta::Date, "dates until chosen");
        assert!(back.split_diff);
        assert_eq!(back.pane_count, 3, "all panes until a count is chosen");
    }
}
