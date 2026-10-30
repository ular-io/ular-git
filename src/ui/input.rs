//! Key routing. The front-most layer (dialog, palette, help, filter line)
//! takes every key; only `ctrl+c` passes through to arm quitting. On the
//! screen the focused pane consumes its own keys and the rest fall through to
//! the global keys.

use super::components::list_nav::{self, ListMove};
use super::{App, Level, Pane, UiMode};
use crate::ui::tree::{ChangeView, Row};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn ctrl(key: &KeyEvent, c: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(c)
}

/// Plain character with no ctrl/alt (shift allowed: `+`, `?`, `G`).
fn plain(key: &KeyEvent) -> Option<char> {
    match key.code {
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(c)
        }
        _ => None,
    }
}

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        if ctrl(&key, 'c') {
            self.arm_quit();
            return;
        }
        let quit_key = self.mode == UiMode::Normal && plain(&key) == Some('q');
        if !quit_key {
            self.quit_armed = false;
        }
        match self.mode {
            UiMode::Normal => self.on_key_normal(key),
            UiMode::Filter => self.on_key_filter(key),
            UiMode::AddRepo => self.on_key_add_repo(key),
            UiMode::GroupName => self.on_key_group_name(key),
            UiMode::GroupPick => self.on_key_group_pick(key),
            UiMode::Confirm => self.on_key_confirm(key),
            UiMode::Commit => self.on_key_commit(key),
            UiMode::OpenWith => self.on_key_open_with(key),
            UiMode::QuickCommand => self.on_key_quick(key),
            UiMode::ThemeSelect => self.on_key_theme_select(key),
            UiMode::Help => self.on_key_help(key),
            UiMode::Message => self.on_key_message(key),
        }
    }

    /// One bracketed paste goes, as text, to the input that has focus.
    pub fn on_paste(&mut self, text: &str) {
        match self.mode {
            UiMode::Filter => {
                self.filter.insert_paste(text);
                self.rebuild_rows_for_filter();
            }
            UiMode::AddRepo => self.paste_add_repo(text),
            UiMode::GroupName => self.paste_group_name(text),
            UiMode::Commit => self.paste_commit(text),
            UiMode::QuickCommand => self.paste_quick(text),
            _ => {}
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent) {
        let consumed = match self.focus {
            Pane::Repos => self.on_key_repos(&key),
            Pane::Files => self.on_key_files(&key),
            Pane::Diff => self.on_key_diff(&key),
        };
        if !consumed {
            self.on_key_global(&key);
        }
    }

    fn on_key_global(&mut self, key: &KeyEvent) {
        if ctrl(key, 'u') {
            self.refresh_all(false);
            return;
        }
        let Some(c) = plain(key) else {
            return;
        };
        match c {
            'q' => self.arm_quit(),
            ':' => self.open_quick_command(),
            '!' => self.open_quick_terminal(),
            '?' => self.open_help(),
            '/' => self.open_filter(),
            '0' => self.clear_filter(),
            '+' => self.open_add_repo(),
            'c' => self.open_commit(),
            'P' => self.open_push(),
            'p' => self.start_pull(),
            '1' => self.focus_pane(Pane::Repos),
            '2' => self.focus_pane(Pane::Files),
            '3' => self.focus_pane(Pane::Diff),
            _ => {}
        }
    }

    /// Whether `→` moves on from the focused pane: not from Diff, and not from
    /// a group header (it has no files).
    pub(crate) fn can_move_right(&self) -> bool {
        match self.focus {
            Pane::Repos => !matches!(self.current_row(), Some(Row::Group { .. })),
            Pane::Files => true,
            Pane::Diff => false,
        }
    }

    pub(crate) fn focus_pane(&mut self, pane: Pane) {
        self.focus = pane;
    }

    fn move_focus(&mut self, delta: isize) {
        let i = (self.focus.index() as isize + delta).clamp(0, 2) as usize;
        self.focus = Pane::ALL[i];
    }

    /// Repository pane keys. Returns whether the key was used.
    fn on_key_repos(&mut self, key: &KeyEvent) -> bool {
        if let Some(mv) = list_nav::list_move(key, true) {
            let next = list_nav::apply(self.repo_sel, self.rows.len(), mv, self.repo_page());
            self.select_repo_row(next);
            return true;
        }
        if ctrl(key, 'n') {
            self.open_new_group();
            return true;
        }
        if ctrl(key, '.') {
            self.toggle_change_view(ChangeView::Pending);
            return true;
        }
        if ctrl(key, 'e') {
            self.open_edit_row();
            return true;
        }
        if ctrl(key, 'd') || key.code == KeyCode::Delete {
            self.open_remove_row();
            return true;
        }
        let is_group = matches!(self.current_row(), Some(Row::Group { .. }));
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') if is_group => self.toggle_group(),
            // A group header has no files to open.
            KeyCode::Right | KeyCode::Char('l') if is_group => {}
            KeyCode::Enter => self.open_with_folder(),
            KeyCode::Right | KeyCode::Char('l') => self.move_focus(1),
            KeyCode::Char('.') => self.toggle_change_view(ChangeView::Changed),
            _ => return false,
        }
        true
    }

    fn on_key_files(&mut self, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::PageUp => self.diff.page_by(-1),
            KeyCode::PageDown => self.diff.page_by(1),
            KeyCode::Left | KeyCode::Char('h') => self.move_focus(-1),
            // A changed file opens in an app; a commit row shows its diff.
            KeyCode::Enter if self.open_with_file() => {}
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.move_focus(1),
            KeyCode::Char('.') => self.toggle_wrap(),
            _ => {
                let Some(mv) = list_nav::list_move(key, true) else {
                    return false;
                };
                let len = self.file_items().len();
                let next = list_nav::apply(self.file_sel, len, mv, self.file_page());
                if next != self.file_sel || self.file_key.is_none() {
                    self.select_file(next);
                }
            }
        }
        true
    }

    fn on_key_diff(&mut self, key: &KeyEvent) -> bool {
        if let Some(mv) = list_nav::list_move(key, true) {
            match mv {
                ListMove::Up => self.diff.scroll_by(-1),
                ListMove::Down => self.diff.scroll_by(1),
                ListMove::PageUp => self.diff.page_by(-1),
                ListMove::PageDown => self.diff.page_by(1),
                ListMove::Home => self.diff.scroll = 0,
                ListMove::End => self.diff.scroll_to_end(),
            }
            return true;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => self.move_focus(-1),
            KeyCode::Char('.') => self.toggle_wrap(),
            _ => return false,
        }
        true
    }

    pub(crate) fn toggle_wrap(&mut self) {
        self.diff.wrap = !self.diff.wrap;
        self.diff.scroll = 0;
        let state = if self.diff.wrap { "on" } else { "off" };
        self.notify(Level::Info, format!("Line wrap {state}"));
    }

    pub(crate) fn select_repo_row(&mut self, index: usize) {
        if index == self.repo_sel {
            return;
        }
        let before = self.current_target();
        self.repo_sel = index.min(self.rows.len().saturating_sub(1));
        if self.current_target() != before {
            self.on_target_changed();
        }
    }

    pub(crate) fn toggle_group(&mut self) {
        let Some(Row::Group { name, .. }) = self.current_row().cloned() else {
            return;
        };
        if self.list_filtered() {
            self.notify(Level::Info, "Groups stay open while a filter is applied");
            return;
        }
        self.store.toggle_collapsed(name.as_deref());
        self.save_store();
        self.rebuild_rows();
    }

    /// Visible rows of the repository list (recorded at render time).
    fn repo_page(&self) -> usize {
        self.repo_page.get().max(1)
    }

    fn file_page(&self) -> usize {
        self.file_page.get().max(1)
    }

    // ---- `/` filter line ----

    pub(crate) fn open_filter(&mut self) {
        self.filter_before = self.filter.value.clone();
        self.filter.end();
        self.mode = UiMode::Filter;
        self.focus = Pane::Repos;
    }

    /// `0`: clears the text filter and the changed-only view.
    pub(crate) fn clear_filter(&mut self) {
        if self.filter.value.is_empty() && self.change_view == ChangeView::All {
            return;
        }
        self.filter = super::TextInput::new(String::new());
        self.change_view = ChangeView::All;
        self.rebuild_rows_for_filter();
        self.notify(Level::Info, "Filter cleared");
    }

    /// `.` (`Changed`) / `ctrl+.` (`Pending`) on the repository pane: turns
    /// that view on, or back to every repository when it is already on.
    pub(crate) fn toggle_change_view(&mut self, view: ChangeView) {
        self.change_view = if self.change_view == view {
            ChangeView::All
        } else {
            view
        };
        self.rebuild_rows_for_filter();
        let text = match self.change_view {
            ChangeView::All => "Showing all repositories",
            ChangeView::Changed => "Showing repositories with changes or commits to pull",
            ChangeView::Pending => {
                "Showing repositories with changes, commits to pull or push, or stashes"
            }
        };
        self.notify(Level::Info, text);
    }

    /// A text filter or the changed-only view narrows the list.
    pub(crate) fn list_filtered(&self) -> bool {
        !self.filter.value.trim().is_empty() || self.change_view != ChangeView::All
    }

    /// What narrows the list, for the pane title and the header:
    /// `api`, `changed`, `api · pending`.
    pub(crate) fn filter_label(&self) -> Option<String> {
        let text = self.filter.value.trim();
        let view = match self.change_view {
            ChangeView::All => None,
            ChangeView::Changed => Some("changed"),
            ChangeView::Pending => Some("pending"),
        };
        match (text.is_empty(), view) {
            (true, None) => None,
            (true, Some(v)) => Some(v.into()),
            (false, None) => Some(text.to_string()),
            (false, Some(v)) => Some(format!("{text} · {v}")),
        }
    }

    fn on_key_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter = super::TextInput::new(std::mem::take(&mut self.filter_before));
                self.mode = UiMode::Normal;
                self.rebuild_rows_for_filter();
            }
            KeyCode::Enter => self.mode = UiMode::Normal,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                if let Some(mv) = list_nav::list_move(&key, false) {
                    let next =
                        list_nav::apply(self.repo_sel, self.rows.len(), mv, self.repo_page());
                    self.select_repo_row(next);
                }
            }
            KeyCode::Left => self.filter.move_left(),
            KeyCode::Right => self.filter.move_right(),
            KeyCode::Home => self.filter.home(),
            KeyCode::End => self.filter.end(),
            KeyCode::Backspace => {
                self.filter.backspace();
                self.rebuild_rows_for_filter();
            }
            KeyCode::Delete => {
                self.filter.delete();
                self.rebuild_rows_for_filter();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.filter.insert_char(c);
                self.rebuild_rows_for_filter();
            }
            _ => {}
        }
    }
}
