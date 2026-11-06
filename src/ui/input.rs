//! Key routing. The front-most layer (dialog, palette, help, filter line)
//! takes every key; only `ctrl+c` passes through to arm quitting. On the
//! screen the focused pane consumes its own keys (each shown in its header
//! columns), then `c` / `P` / `p` (in every pane header), then the common
//! keys (`:` `!` `ctrl+u` `?` `q`, plus the pane count `1..3` / `\`).

use super::components::list_nav::{self, ListMove};
use super::{App, Level, Pane, UiMode};
use crate::ui::tree::Row;
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
        if !consumed && !self.on_key_tree_ops(&key) {
            self.on_key_global(&key);
        }
    }

    /// `c` / `P` / `p` / `ctrl+f` on the selected working tree; every pane
    /// header lists them.
    fn on_key_tree_ops(&mut self, key: &KeyEvent) -> bool {
        if ctrl(key, 'f') {
            self.start_fetch();
            return true;
        }
        match plain(key) {
            Some('c') => self.open_commit(),
            Some('P') => self.open_push(),
            Some('p') => self.start_pull(),
            _ => return false,
        }
        true
    }

    /// Keys that work in every pane: the header's common column, plus the
    /// pane count (help only).
    fn on_key_global(&mut self, key: &KeyEvent) {
        if ctrl(key, 'u') {
            self.refresh_and_fetch();
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
            '1'..='3' => self.set_pane_count(c as usize - '0' as usize),
            '\\' => self.cycle_pane_count(),
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
        if ctrl(key, 'e') {
            self.open_edit_row();
            return true;
        }
        if ctrl(key, 'd') || key.code == KeyCode::Delete {
            self.open_remove_row();
            return true;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Up => self.move_row(-1),
                KeyCode::Down => self.move_row(1),
                _ => return false,
            }
            return true;
        }
        let is_group = matches!(self.current_row(), Some(Row::Group { .. }));
        match key.code {
            KeyCode::Enter if is_group => self.toggle_group(),
            // A group header has no files to open: `→` unfolds it, `←` folds.
            KeyCode::Right | KeyCode::Char('l') if is_group => self.fold_group(false),
            KeyCode::Left | KeyCode::Char('h') if is_group => self.fold_group(true),
            KeyCode::Enter => self.open_with_folder(),
            KeyCode::Right | KeyCode::Char('l') => self.move_focus(1),
            KeyCode::Char('.') => self.toggle_pending_only(),
            KeyCode::Char('/') => self.open_filter(),
            KeyCode::Char('0') => self.clear_filter(),
            KeyCode::Char('+') => self.open_add_repo(),
            _ => return false,
        }
        true
    }

    fn on_key_files(&mut self, key: &KeyEvent) -> bool {
        if self.commit_view.is_some() {
            return self.on_key_commit_files(key);
        }
        match key.code {
            KeyCode::PageUp => self.diff.page_by(-1),
            KeyCode::PageDown => self.diff.page_by(1),
            KeyCode::Left | KeyCode::Char('h') => self.move_focus(-1),
            // A changed file opens in an app; a commit row shows its diff.
            KeyCode::Enter if self.open_with_file() => {}
            // A commit lists its files; `enter` still opens its whole diff.
            KeyCode::Right | KeyCode::Char('l') if self.open_commit_files() => {}
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.move_focus(1),
            KeyCode::Char('.') => self.cycle_commit_meta(),
            KeyCode::Char('>') => self.cycle_ref_labels(),
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
                ListMove::Home => self.diff.scroll_to_top(),
                ListMove::End => self.diff.scroll_to_end(),
            }
            return true;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => self.move_focus(-1),
            KeyCode::Char('.') => self.toggle_wrap(),
            KeyCode::Char('s') => self.toggle_split(),
            _ => return false,
        }
        true
    }

    /// `1` / `2` / `3`: how many panes to show side by side. Only counts the
    /// width can show apply, and none when it shows one pane.
    pub(crate) fn set_pane_count(&mut self, count: usize) {
        let fits = super::render::visible_panes(self.panes_w.get());
        if fits < 2 || count > fits || count == self.pane_count {
            return;
        }
        self.pane_count = count;
        self.save_view();
    }

    /// `\`: the next pane count the width can show, from the one shown now
    /// (3 → 2 → 1 → 3; 2 ↔ 1 below 160 columns).
    pub(crate) fn cycle_pane_count(&mut self) {
        let fits = super::render::visible_panes(self.panes_w.get());
        let next = match self.pane_count.min(fits) {
            0 | 1 => fits,
            shown => shown - 1,
        };
        self.set_pane_count(next);
    }

    /// `s`: side-by-side diffs on wide panes, or always unified.
    pub(crate) fn toggle_split(&mut self) {
        self.diff.split = !self.diff.split;
        self.save_view();
        let text = if self.diff.split {
            "Side-by-side diff when the diff pane is 120+ columns wide"
        } else {
            "Unified diff"
        };
        self.notify(Level::Info, text);
    }

    pub(crate) fn toggle_wrap(&mut self) {
        self.diff.wrap = !self.diff.wrap;
        let state = if self.diff.wrap { "on" } else { "off" };
        self.notify(Level::Info, format!("Line wrap {state}"));
    }

    pub(crate) fn select_repo_row(&mut self, index: usize) {
        if index == self.repo_sel {
            return;
        }
        let before = self.current_target();
        let was_in_folded_group = self
            .current_group()
            .is_some_and(|name| self.store.is_collapsed(Some(&name)))
            && before.is_some();
        self.repo_sel = index.min(self.rows.len().saturating_sub(1));
        if self.current_target() != before {
            self.on_target_changed();
        }
        if was_in_folded_group {
            self.rebuild_rows();
        }
    }

    pub(crate) fn toggle_group(&mut self) {
        if let Some(Row::Group { name, .. }) = self.current_row().cloned() {
            self.change_group_fold(name, None);
        }
    }

    /// Explicit fold directions replay even when a preceding request has not
    /// reached the screen yet; toggles instead resolve against latest state.
    pub(crate) fn fold_group(&mut self, fold: bool) {
        if let Some(Row::Group { name, .. }) = self.current_row().cloned() {
            self.change_group_fold(name, Some(fold));
        }
    }

    /// `alt+↑` / `alt+↓`: moves the group under the cursor among the groups,
    /// or the repository among the repositories of its group. Worktrees
    /// follow git's order. Not while a filter hides rows, where one place up
    /// may be a row that is not shown.
    pub(crate) fn move_row(&mut self, delta: isize) {
        let row = self.current_row().cloned();
        if !matches!(
            row,
            Some(Row::Group { name: Some(_), .. } | Row::Repo { .. })
        ) {
            return;
        }
        if self.list_filtered() {
            self.notify(
                Level::Info,
                "The order stays fixed while a filter is applied",
            );
            return;
        }
        self.change_store(
            move |store| {
                match row {
                    Some(Row::Group {
                        name: Some(name), ..
                    }) => {
                        store.require_group(Some(&name))?;
                        store.move_group(&name, delta);
                    }
                    Some(Row::Repo { path, .. }) => {
                        store.require_repo(&path)?;
                        store.move_repo(&path, delta);
                    }
                    _ => {}
                }
                Ok(())
            },
            |_, ()| {},
        );
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
        if !self.filter_clearable() {
            return;
        }
        self.filter = super::TextInput::new(String::new());
        self.pending_only = false;
        self.rebuild_rows_for_filter();
        self.notify(Level::Info, "Filter cleared");
    }

    /// `.` on the repository pane: only working trees with something to
    /// commit, pull, or push, or a stash; again for every repository.
    pub(crate) fn toggle_pending_only(&mut self) {
        self.pending_only = !self.pending_only;
        self.rebuild_rows_for_filter();
        let text = if self.pending_only {
            "Showing repositories with changes, commits to pull or push, or stashes"
        } else {
            "Showing all repositories"
        };
        self.notify(Level::Info, text);
    }

    /// Whether `0` has a text filter or the changed-only view to clear.
    pub(crate) fn filter_clearable(&self) -> bool {
        !self.filter.value.is_empty() || self.pending_only
    }

    /// A text filter or the changed-only view narrows the list.
    pub(crate) fn list_filtered(&self) -> bool {
        !self.filter.value.trim().is_empty() || self.pending_only
    }

    /// What narrows the list, for the pane title and the header:
    /// `api`, `changed`, `api · pending`.
    pub(crate) fn filter_label(&self) -> Option<String> {
        let text = self.filter.value.trim();
        let view = self.pending_only.then_some("pending");
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
