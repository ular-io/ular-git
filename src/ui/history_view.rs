//! History pane section state: the commit graph of the selected working tree,
//! loaded in the background (one load in flight; a newer request replaces a
//! queued one; results for an older request are dropped).

use super::App;
use crate::git::history::{self, History, FIRST_PAGE};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct HistoryView {
    /// Working tree the history belongs to (loaded or loading).
    pub target: Option<PathBuf>,
    /// HEAD (short oid) when it was requested; a different HEAD reloads.
    pub head: Option<String>,
    /// Commits requested.
    pub limit: usize,
    pub history: Option<History>,
    pub loading: bool,
    pub error: Option<String>,
    generation: u64,
    in_flight: bool,
    queued: Option<(PathBuf, usize)>,
}

impl App {
    /// Loads the history of the selected working tree when it is not loaded
    /// for that tree and HEAD yet (target change, commit, pull, checkout).
    pub(crate) fn ensure_history(&mut self) {
        let Some(target) = self.current_target() else {
            self.history = HistoryView::default();
            return;
        };
        let head = self
            .current_tree()
            .and_then(|t| t.status.as_ref())
            .and_then(|s| s.branch.oid.clone());
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
        }
        self.history.head = head;
        self.request_history(target, limit);
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
        let view = &mut self.history;
        view.target = Some(target.clone());
        view.limit = limit;
        view.loading = true;
        view.generation += 1;
        if view.in_flight {
            view.queued = Some((target, limit));
            return;
        }
        self.spawn_history(target, limit);
    }

    fn spawn_history(&mut self, target: PathBuf, limit: usize) {
        self.history.in_flight = true;
        let generation = self.history.generation;
        self.bg.spawn(move || {
            let result = history::load(&target, limit).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| app.apply_history(generation, result))
        });
    }

    fn apply_history(&mut self, generation: u64, result: Result<History, String>) {
        self.history.in_flight = false;
        if let Some((target, limit)) = self.history.queued.take() {
            self.spawn_history(target, limit);
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
