//! Diff pane state: the loaded document, its scroll position, and the
//! latest-wins loader (one load in flight; a newer request replaces any
//! queued one, and a result for an older request is dropped).

use super::App;
use crate::git::diff::{self, DiffDoc, DiffSource};
use std::cell::Cell;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct DiffView {
    /// Identity (`target \0 item key`) of what `doc` shows or is loading.
    pub key: Option<String>,
    pub doc: Option<DiffDoc>,
    pub error: Option<String>,
    pub loading: bool,
    /// Top visual line.
    pub scroll: usize,
    /// Largest `scroll`, recorded by the renderer (it knows the height).
    pub max_scroll: Cell<usize>,
    /// Visible line count, recorded by the renderer (page scroll size).
    pub page: Cell<usize>,
    /// Long lines wrap instead of being cut.
    pub wrap: bool,
    generation: u64,
    in_flight: bool,
    queued: Option<(PathBuf, DiffSource, String)>,
}

impl DiffView {
    pub fn clear(&mut self) {
        self.key = None;
        self.doc = None;
        self.error = None;
        self.loading = false;
        self.scroll = 0;
        self.queued = None;
        self.generation += 1;
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let max = self.max_scroll.get();
        self.scroll = (self.scroll as isize + delta).clamp(0, max as isize) as usize;
    }

    pub fn page_by(&mut self, pages: isize) {
        let page = self.page.get().max(1) as isize;
        self.scroll_by(pages * (page - 1).max(1));
    }

    pub fn scroll_to_end(&mut self) {
        self.scroll = self.max_scroll.get();
    }
}

impl App {
    /// Loads the diff for `source`. The same key reloads in place (after a
    /// rescan) and keeps the scroll position; a new key starts at the top.
    pub(crate) fn load_diff(&mut self, target: PathBuf, source: DiffSource, key: String) {
        let view = &mut self.diff;
        if view.key.as_deref() != Some(key.as_str()) {
            view.scroll = 0;
            view.doc = None;
            view.error = None;
        }
        view.key = Some(key.clone());
        view.loading = true;
        view.generation += 1;
        if view.in_flight {
            view.queued = Some((target, source, key));
            return;
        }
        self.spawn_diff(target, source);
    }

    fn spawn_diff(&mut self, target: PathBuf, source: DiffSource) {
        self.diff.in_flight = true;
        let generation = self.diff.generation;
        let max = self.cfg.max_diff_lines;
        self.bg.spawn(move || {
            let result = diff::load(&target, &source, max).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| app.apply_diff(generation, result))
        });
    }

    fn apply_diff(&mut self, generation: u64, result: Result<DiffDoc, String>) {
        self.diff.in_flight = false;
        if let Some((target, source, _)) = self.diff.queued.take() {
            self.spawn_diff(target, source);
            return;
        }
        if generation != self.diff.generation {
            return;
        }
        self.diff.loading = false;
        match result {
            Ok(doc) => {
                self.diff.doc = Some(doc);
                self.diff.error = None;
            }
            Err(err) => {
                self.diff.doc = None;
                self.diff.error = Some(err);
            }
        }
    }
}
