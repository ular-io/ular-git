//! Diff pane state: the loaded document, its scroll position, and the
//! latest-wins loader (one load in flight; a newer request cancels it, killing
//! its git process, and replaces any queued one; a result for an older
//! request is dropped).

use super::App;
use crate::git::cancel::Cancel;
use crate::git::diff::{self, DiffDoc, DiffSource, LoadOpts};
use std::cell::Cell;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct DiffView {
    /// Identity (`target \0 item key`) of what `doc` shows or is loading.
    pub key: Option<String>,
    pub doc: Option<DiffDoc>,
    pub error: Option<String>,
    pub loading: bool,
    /// Top visual line. The renderer moves it when the layout changes, so
    /// the line on top stays on top ([`Anchor`]).
    pub scroll: Cell<usize>,
    /// Top line of the last frame, recorded by the renderer.
    pub anchor: Cell<Option<Anchor>>,
    /// Largest `scroll`, recorded by the renderer (it knows the height).
    pub max_scroll: Cell<usize>,
    /// Visible line count, recorded by the renderer (page scroll size).
    pub page: Cell<usize>,
    /// Long lines wrap instead of being cut.
    pub wrap: bool,
    /// Side by side (old left, new right) when the pane is wide enough;
    /// `s` toggles it, and the choice is kept in `view.json`.
    pub split: bool,
    generation: u64,
    in_flight: bool,
    /// Cancels the load in flight.
    cancel: Cancel,
    queued: Option<(PathBuf, DiffSource, String)>,
}

/// The line at the top of the last frame and the layout it was drawn in.
/// When wrapping, side by side, or the pane width changes, the next frame
/// starts at that line instead of at the same visual row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    /// `scroll` when the frame was drawn; a scroll since then wins.
    pub row: usize,
    /// Index of the line drawn on the top row.
    pub line: usize,
    /// Wrapping, side by side, and the pane width.
    pub layout: (bool, bool, usize),
}

impl DiffView {
    pub fn clear(&mut self) {
        self.key = None;
        self.doc = None;
        self.error = None;
        self.loading = false;
        self.scroll_to_top();
        self.queued = None;
        self.generation += 1;
        self.cancel.cancel();
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let max = self.max_scroll.get();
        let next = (self.scroll.get() as isize + delta).clamp(0, max as isize) as usize;
        self.scroll.set(next);
    }

    pub fn page_by(&mut self, pages: isize) {
        let page = self.page.get().max(1) as isize;
        self.scroll_by(pages * (page - 1).max(1));
    }

    pub fn scroll_to_end(&mut self) {
        self.scroll.set(self.max_scroll.get());
    }

    /// Back to the first line, forgetting the line that was on top.
    pub fn scroll_to_top(&mut self) {
        self.scroll.set(0);
        self.anchor.set(None);
    }
}

impl App {
    /// Loads the diff for `source`. The same key reloads in place (after a
    /// rescan) and keeps the scroll position; a new key starts at the top.
    pub(crate) fn load_diff(&mut self, target: PathBuf, source: DiffSource, key: String) {
        let view = &mut self.diff;
        if view.key.as_deref() != Some(key.as_str()) {
            view.scroll_to_top();
            view.doc = None;
            view.error = None;
        }
        view.key = Some(key.clone());
        view.loading = true;
        view.generation += 1;
        if view.in_flight {
            view.cancel.cancel();
            view.queued = Some((target, source, key));
            return;
        }
        self.spawn_diff(target, source);
    }

    fn spawn_diff(&mut self, target: PathBuf, source: DiffSource) {
        self.diff.in_flight = true;
        self.diff.cancel = Cancel::default();
        let generation = self.diff.generation;
        let opts = LoadOpts {
            max_lines: self.cfg.max_diff_lines,
            big_file: self.cfg.max_diff_file_bytes(),
            cancel: self.diff.cancel.clone(),
        };
        self.bg.spawn(move || {
            let result = diff::load(&target, &source, &opts).map_err(|e| e.to_string());
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

#[cfg(test)]
mod tests {
    use crate::git::commit_files::CommitFile;
    use crate::git::diff::DiffSource;
    use crate::git::test_repo::TestRepo;
    use crate::ui::test_support::*;

    fn untracked(path: &str) -> DiffSource {
        DiffSource::WorkTreeFile(CommitFile {
            status: '?',
            path: path.into(),
            orig_path: None,
            counts: None,
        })
    }

    #[test]
    fn a_newer_load_cancels_the_one_in_flight() {
        let a = TestRepo::new();
        a.write("one.txt", "first\n");
        a.write("two.txt", "second\n");
        let mut app = scanned_app(store_of(&[&a.root]));
        app.wait_background(10);
        app.load_diff(a.root.clone(), untracked("one.txt"), "one".into());
        let first = app.diff.cancel.clone();
        app.load_diff(a.root.clone(), untracked("two.txt"), "two".into());
        assert!(first.is_cancelled());
        app.wait_background(10);
        let doc = app.diff.doc.as_ref().expect("the newer diff");
        assert!(doc.lines.iter().any(|l| l.text == "+second"), "{doc:#?}");
        assert!(!app.diff.loading && app.diff.error.is_none());
        assert!(
            !app.diff.cancel.is_cancelled(),
            "the newer load has its own"
        );
    }
}
