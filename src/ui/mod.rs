//! TUI application state.
//!
//! One screen with three panes — repositories, changed files, diff — plus the
//! dialogs and overlays layered over it. `UiMode` says who receives keys,
//! `Pane` says which pane has focus while the screen itself does.

pub mod background;
pub mod components;
pub mod diff_view;
mod files;
pub mod history_view;
pub mod input;
pub mod keys;
mod notice;
pub mod overlays;
pub mod quick;
pub mod render;
pub mod scan;
pub mod sync;
pub mod tree;

pub use components::input::TextInput;
pub use diff_view::DiffView;
pub use notice::{Level, Notice};

use crate::config::Config;
use crate::store::RepoStore;
use background::Background;
use scan::{ScanState, TreeState};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use tree::{Row, RowKey};

/// The three panes, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Repos,
    Files,
    Diff,
}

impl Pane {
    pub const ALL: [Pane; 3] = [Pane::Repos, Pane::Files, Pane::Diff];

    pub fn index(self) -> usize {
        match self {
            Pane::Repos => 0,
            Pane::Files => 1,
            Pane::Diff => 2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Pane::Repos => "Repos",
            Pane::Files => "Files",
            Pane::Diff => "Diff",
        }
    }
}

/// Who receives key input. Every dialog state lives in its own `Option` on
/// [`App`] and is `Some` exactly while its mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// The three-pane screen.
    Normal,
    /// `/` repository filter line under the header.
    Filter,
    AddRepo,
    GroupName,
    GroupPick,
    Confirm,
    Commit,
    /// App picker of Open With (`enter` on a repository row or a file).
    OpenWith,
    QuickCommand,
    ThemeSelect,
    Help,
    Message,
}

/// Shell command to run with the terminal handed over (`!`, Edit Config, and
/// terminal apps of Open With).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRequest {
    pub cwd: PathBuf,
    pub command: String,
    /// Wait for a key after the command exits so its output can be read.
    pub pause: bool,
    /// Named in the return notice instead of the command (an Open With app).
    pub name: Option<String>,
}

pub struct App {
    pub cfg: Config,
    pub store: RepoStore,
    /// `None` disables saving (unit tests, or a malformed file kept untouched).
    pub(crate) store_path: Option<PathBuf>,
    /// Scan results per working tree path (repositories and linked worktrees).
    pub statuses: HashMap<PathBuf, TreeState>,
    pub(crate) scan: ScanState,
    /// Push / pull running per working tree.
    pub(crate) remote_ops: HashMap<PathBuf, sync::RemoteOp>,
    pub(crate) bg: Background,

    pub mode: UiMode,
    pub focus: Pane,
    /// Visible repository pane rows (rebuilt by [`App::rebuild_rows`]).
    pub rows: Vec<Row>,
    pub repo_sel: usize,
    pub(crate) repo_list: RefCell<ratatui::widgets::ListState>,
    /// Visible repository rows, recorded by the renderer for page movement.
    pub(crate) repo_page: Cell<usize>,
    /// Selected row of the file pane and the identity it is kept on.
    pub file_sel: usize,
    pub(crate) file_key: Option<String>,
    pub(crate) file_list: RefCell<ratatui::widgets::ListState>,
    pub(crate) file_page: Cell<usize>,
    pub diff: DiffView,
    /// Commit graph of the selected working tree (file pane, below the files).
    pub history: history_view::HistoryView,

    /// Repository filter text (`/`); applies while non-empty.
    pub filter: TextInput,
    /// `.` on the repository pane: list only working trees with something to
    /// commit, pull, or push, or a stash. Session only.
    pub pending_only: bool,
    /// Filter value when the line was opened, restored by `esc`.
    pub(crate) filter_before: String,

    pub(crate) add_repo: Option<overlays::add_repo::AddRepoDialog>,
    pub(crate) group_name: Option<overlays::group::GroupNameDialog>,
    pub(crate) group_pick: Option<overlays::group::GroupPickDialog>,
    pub(crate) confirm: Option<overlays::confirm::ConfirmDialog>,
    pub(crate) commit: Option<overlays::commit::CommitDialog>,
    pub(crate) message: Option<overlays::message::MessageDialog>,
    pub(crate) open_with: Option<overlays::open_with::OpenWithDialog>,
    pub(crate) last_apps: overlays::open_with::LastApps,
    pub(crate) theme_select: Option<overlays::theme::ThemeSelectState>,
    pub(crate) help_scroll: Cell<usize>,
    pub(crate) help_max: Cell<usize>,
    pub(crate) quick: Option<quick::QuickState>,
    pub(crate) quick_history: Vec<String>,
    pub(crate) terminal_history: Vec<String>,

    pub theme: crate::theme::Theme,
    pub notice: Option<Notice>,
    pub should_quit: bool,
    pub(crate) quit_armed: bool,
    pub terminal_request: Option<TerminalRequest>,
    /// Folder the app was launched from (prefills Add Repository).
    pub(crate) launch_dir: Option<PathBuf>,
}

impl App {
    /// Builds the app from the persisted store and status cache (both skipped
    /// in unit tests) and starts the scan workers.
    pub fn new(cfg: Config) -> Self {
        let (store, store_path, load_error) = if cfg!(test) {
            (RepoStore::default(), None, None)
        } else {
            let path = crate::config::repos_path();
            match RepoStore::load(&path) {
                Ok(store) => (store, Some(path), None),
                Err(err) => (
                    RepoStore::default(),
                    None,
                    Some(format!(
                        "Repository list unreadable, changes are not saved: {err}"
                    )),
                ),
            }
        };
        let mut app = App::with_store(cfg, store, store_path);
        if !cfg!(test) {
            app.load_status_cache();
            app.quick_history = quick::load_history();
            app.terminal_history = quick::load_terminal_history();
            app.last_apps = overlays::open_with::load_last_apps();
            let view = history_view::load_view();
            app.history.labels = view.history_labels;
            app.diff.split = view.split_diff;
            app.launch_dir = std::env::current_dir().ok();
        }
        if let Some(err) = load_error {
            app.notify(Level::Error, err);
        }
        app.rebuild_rows();
        app.scan_all();
        app
    }

    pub(crate) fn with_store(cfg: Config, store: RepoStore, store_path: Option<PathBuf>) -> Self {
        let workers = cfg.scan_jobs;
        App {
            cfg,
            store,
            store_path,
            statuses: HashMap::new(),
            scan: ScanState::new(),
            remote_ops: HashMap::new(),
            bg: Background::new(workers),
            mode: UiMode::Normal,
            focus: Pane::Repos,
            rows: Vec::new(),
            repo_sel: 0,
            repo_list: RefCell::new(Default::default()),
            repo_page: Cell::new(1),
            file_sel: 0,
            file_key: None,
            file_list: RefCell::new(Default::default()),
            file_page: Cell::new(1),
            diff: DiffView::default(),
            history: history_view::HistoryView::default(),
            filter: TextInput::new(String::new()),
            pending_only: false,
            filter_before: String::new(),
            add_repo: None,
            group_name: None,
            group_pick: None,
            confirm: None,
            commit: None,
            message: None,
            theme_select: None,
            help_scroll: Cell::new(0),
            help_max: Cell::new(0),
            quick: None,
            quick_history: Vec::new(),
            terminal_history: Vec::new(),
            open_with: None,
            last_apps: Default::default(),
            theme: crate::theme::current(),
            notice: None,
            should_quit: false,
            quit_armed: false,
            terminal_request: None,
            launch_dir: None,
        }
    }

    /// Row under the repository cursor.
    pub fn current_row(&self) -> Option<&Row> {
        self.rows.get(self.repo_sel)
    }

    /// Working tree the file and diff panes show.
    pub fn current_target(&self) -> Option<PathBuf> {
        self.current_row()
            .and_then(|r| r.target())
            .map(PathBuf::from)
    }

    pub fn current_tree(&self) -> Option<&TreeState> {
        let target = self.current_target()?;
        self.statuses.get(&target)
    }

    /// Repository entry owning the selected row (a worktree's main repository).
    pub fn current_repo_index(&self) -> Option<usize> {
        match self.current_row()? {
            Row::Repo { idx, .. } => Some(*idx),
            Row::Worktree { repo, .. } => Some(*repo),
            Row::Group { .. } => None,
        }
    }

    /// Group the selected row belongs to (header or member).
    pub fn current_group(&self) -> Option<String> {
        match self.current_row()? {
            Row::Group { name, .. } => name.clone(),
            _ => self
                .current_repo_index()
                .and_then(|i| self.store.repos[i].group.clone()),
        }
    }

    /// Rebuilds the repository rows, keeping the cursor on the same item when
    /// it is still visible (otherwise on the nearest row).
    pub fn rebuild_rows(&mut self) {
        self.rebuild_rows_with(false);
    }

    /// Rebuild after the filter text changed: a row the filter hides gives
    /// the cursor to the first match.
    pub(crate) fn rebuild_rows_for_filter(&mut self) {
        self.rebuild_rows_with(true);
    }

    fn rebuild_rows_with(&mut self, first_on_miss: bool) {
        let key = self.current_row().map(|r| r.key());
        let before = self.current_target();
        // A rescan or list change keeps the tree being looked at; a filter
        // change applies to it like to any other.
        let keep = if first_on_miss { None } else { before.clone() };
        self.rows = tree::build_rows(
            &self.store,
            &self.statuses,
            &self.filter.value,
            self.pending_only,
            keep.as_deref(),
        );
        let fallback = first_on_miss.then(|| {
            self.rows
                .iter()
                .position(|r| !matches!(r, Row::Group { .. }))
                .unwrap_or(0)
        });
        self.place_cursor(key.as_ref(), fallback, before);
    }

    /// Puts the cursor on `key` (after rows were rebuilt elsewhere).
    pub(crate) fn select_row_key(&mut self, key: Option<&RowKey>) {
        let before = self.current_target();
        self.place_cursor(key, None, before);
    }

    /// Moves the cursor to `key`, else `fallback`, else the nearest row, and
    /// resets the file and diff panes when the working tree changed from
    /// `before` (the target selected before the rows changed).
    fn place_cursor(
        &mut self,
        key: Option<&RowKey>,
        fallback: Option<usize>,
        before: Option<PathBuf>,
    ) {
        let found = key.and_then(|key| self.rows.iter().position(|r| r.key() == *key));
        if let Some(i) = found.or(fallback) {
            self.repo_sel = i;
        }
        self.repo_sel = self.repo_sel.min(self.rows.len().saturating_sub(1));
        // A repository that left the list hands the cursor to the nearest
        // repository row (below first), never to a group header.
        let was_tree = key.is_some_and(|k| !matches!(k, RowKey::Group(_)));
        if found.is_none() && was_tree && matches!(self.current_row(), Some(Row::Group { .. })) {
            let is_tree = |i: &usize| !matches!(self.rows[*i], Row::Group { .. });
            let below = (self.repo_sel..self.rows.len()).find(is_tree);
            let above = (0..self.repo_sel).rev().find(is_tree);
            if let Some(i) = below.or(above) {
                self.repo_sel = i;
            }
        }
        if self.current_target() != before {
            self.on_target_changed();
        }
    }

    /// Persists the repository list; a failure is reported, never fatal.
    pub(crate) fn save_store(&mut self) {
        let Some(path) = self.store_path.clone() else {
            return;
        };
        if let Err(err) = self.store.save(&path) {
            self.notify(Level::Error, format!("Cannot save repository list: {err}"));
        }
    }

    /// Press twice to quit.
    pub(crate) fn arm_quit(&mut self) {
        if self.quit_armed {
            self.should_quit = true;
        } else {
            self.quit_armed = true;
            self.notify(Level::Info, "Press q or ctrl+c again to quit");
        }
    }

    /// Runs on exit: the cache lets the next launch show this state instantly.
    pub fn shutdown(&mut self) {
        self.save_status_cache();
    }
}

#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod sync_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
