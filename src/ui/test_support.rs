//! Shared fixtures for UI tests: an `App` over a given store, key helpers,
//! and a text dump of a rendered frame.

use super::App;
use crate::config::Config;
use crate::store::RepoStore;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

/// App over `store` with every scan finished.
pub(crate) fn scanned_app(store: RepoStore) -> App {
    let mut app = App::with_store(Config::default(), store, None);
    app.rebuild_rows();
    app.scan_all();
    app.wait_background(20);
    app
}

pub(crate) fn store_of(paths: &[&PathBuf]) -> RepoStore {
    let mut store = RepoStore::default();
    for p in paths {
        store.add_repo((*p).clone(), None);
    }
    store
}

pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(crate) fn ch(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

pub(crate) fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

impl App {
    pub(crate) fn press(&mut self, keys: &[KeyEvent]) {
        for k in keys {
            self.on_key(*k);
        }
    }

    pub(crate) fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.on_key(ch(c));
        }
    }
}

/// Rendered frame as one string per row.
pub(crate) fn screen(app: &App, w: u16, h: u16) -> Vec<String> {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| super::render::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect()
}

pub(crate) fn shows(rows: &[String], text: &str) -> bool {
    rows.iter().any(|r| r.contains(text))
}
