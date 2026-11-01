//! Quick Command keys and command execution. Every palette command calls the
//! same method its shortcut calls.

use super::registry::{build_items, CommandId};
use super::state::{build_term_items, save_history, save_terminal_history, QuickMode, QuickState};
use super::VIEWPORT;
use crate::ui::components::input::TextInput;
use crate::ui::tree::Row;
use crate::ui::{App, Level, Pane, TerminalRequest, UiMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const MAX_HISTORY: usize = 20;

impl App {
    pub(crate) fn open_quick_command(&mut self) {
        self.quick = Some(QuickState::new(QuickMode::Palette, None));
        self.quick_recompute();
        self.mode = UiMode::QuickCommand;
    }

    pub(crate) fn open_quick_terminal(&mut self) {
        let Some(folder) = self.terminal_target() else {
            self.notify(Level::Info, "Select a repository to run a command in");
            return;
        };
        self.quick = Some(QuickState::new(QuickMode::Terminal, Some(folder)));
        self.quick_term_recompute();
        self.mode = UiMode::QuickCommand;
    }

    fn terminal_target(&self) -> Option<std::path::PathBuf> {
        self.current_target().filter(|p| p.is_dir())
    }

    fn quick_recompute(&mut self) {
        let Some(query) = self.quick.as_ref().map(|s| s.input.value.clone()) else {
            return;
        };
        let items = build_items(&query, &self.quick_history, |id| self.quick_enabled(id));
        if let Some(state) = self.quick.as_mut() {
            state.items = items;
            state.cursor = 0;
            state.scroll = 0;
        }
    }

    fn quick_term_recompute(&mut self) {
        let Some(query) = self.quick.as_ref().map(|s| s.term_typed.clone()) else {
            return;
        };
        let items = build_term_items(&query, &self.terminal_history);
        if let Some(state) = self.quick.as_mut() {
            state.term_items = items;
            state.term_selected = None;
            state.scroll = 0;
        }
    }

    fn quick_enabled(&self, id: CommandId) -> bool {
        use CommandId::*;
        let row = self.current_row();
        let named_group = matches!(row, Some(Row::Group { name: Some(_), .. }));
        let branch = self
            .current_tree()
            .and_then(|t| t.status.as_ref())
            .map(|s| &s.branch)
            .filter(|b| b.head.is_some());
        let dirty = self
            .current_tree()
            .and_then(|t| t.status.as_ref())
            .is_some_and(|s| !s.is_clean() && s.counts().conflicts == 0);
        match id {
            FocusRepos => self.focus != Pane::Repos,
            FocusFiles => self.focus != Pane::Files,
            FocusDiff => self.focus != Pane::Diff,
            RemoveRepository => matches!(row, Some(Row::Repo { .. })),
            RemoveWorktree => matches!(row, Some(Row::Worktree { .. })),
            RenameGroup | DeleteGroup => named_group,
            MoveGroupUp | MoveGroupDown => named_group && self.store.groups.len() > 1,
            MoveToGroup => matches!(row, Some(Row::Repo { .. } | Row::Worktree { .. })),
            FoldAllGroups | UnfoldAllGroups => !self.store.groups.is_empty(),
            Commit => dirty,
            DiscardFile => self
                .selected_file()
                .is_some_and(|f| f.kind != crate::git::EntryKind::Conflict),
            Push => branch.is_some_and(|b| b.upstream.is_none() || b.ahead > 0),
            Pull => branch.is_some_and(|b| b.upstream.is_some()),
            OpenWith => self.current_target().is_some(),
            ClearFilter => self.list_filtered(),
            TogglePendingOnly => true,
            ToggleWrap | ToggleSplitDiff | CycleRefLabels | CycleCommitMeta => true,
            TerminalCommand => self.terminal_target().is_some(),
            AddRepository | NewGroup | FilterRepos | RefreshAll | ChangeTheme | EditConfig
            | OpenHelp | Quit => true,
        }
    }

    pub(crate) fn on_key_quick(&mut self, key: KeyEvent) {
        let Some((mode, empty)) = self
            .quick
            .as_ref()
            .map(|s| (s.mode, s.input.value.is_empty()))
        else {
            self.mode = UiMode::Normal;
            return;
        };
        if let KeyCode::Char(c @ (':' | '!')) = key.code {
            if empty
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                match c {
                    ':' if mode == QuickMode::Terminal => {
                        if let Some(s) = self.quick.as_mut() {
                            s.mode = QuickMode::Palette;
                        }
                        self.quick_recompute();
                    }
                    '!' if mode == QuickMode::Palette => {
                        if let Some(folder) = self.terminal_target() {
                            if let Some(s) = self.quick.as_mut() {
                                s.mode = QuickMode::Terminal;
                                s.term_folder = Some(folder);
                                s.term_typed.clear();
                            }
                            self.quick_term_recompute();
                        }
                    }
                    _ => {}
                }
                return;
            }
        }
        match mode {
            QuickMode::Palette => self.on_key_palette(key),
            QuickMode::Terminal => self.on_key_terminal(key),
        }
    }

    /// Shared text editing of the input; returns whether the text changed.
    fn quick_edit(input: &mut TextInput, key: &KeyEvent) -> Option<bool> {
        match key.code {
            KeyCode::Left => input.move_left(),
            KeyCode::Right => input.move_right(),
            KeyCode::Home => input.home(),
            KeyCode::End => input.end(),
            KeyCode::Backspace => {
                input.backspace();
                return Some(true);
            }
            KeyCode::Delete => {
                input.delete();
                return Some(true);
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                input.insert_char(c);
                return Some(true);
            }
            _ => return None,
        }
        Some(false)
    }

    fn on_key_palette(&mut self, key: KeyEvent) {
        let Some(state) = self.quick.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.close_quick(),
            KeyCode::Enter => {
                let Some(item) = state.items.get(state.cursor).cloned() else {
                    return;
                };
                if !item.enabled {
                    return;
                }
                self.close_quick();
                self.record_quick_history(item.spec().key);
                self.quick_execute(item.spec().id);
            }
            KeyCode::Up => move_cursor(state, -1),
            KeyCode::Down => move_cursor(state, 1),
            KeyCode::PageUp => move_cursor(state, -(VIEWPORT as isize)),
            KeyCode::PageDown => move_cursor(state, VIEWPORT as isize),
            _ => {
                if Self::quick_edit(&mut state.input, &key) == Some(true) {
                    self.quick_recompute();
                }
            }
        }
    }

    fn on_key_terminal(&mut self, key: KeyEvent) {
        let Some(state) = self.quick.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.close_quick(),
            KeyCode::Enter => {
                let command = state.input.value.trim().to_string();
                let Some(cwd) = state.term_folder.clone() else {
                    return;
                };
                if command.is_empty() {
                    return;
                }
                self.close_quick();
                self.record_terminal_history(&command);
                self.terminal_request = Some(TerminalRequest {
                    cwd,
                    command,
                    pause: true,
                    name: None,
                });
            }
            KeyCode::Up => term_move(state, -1),
            KeyCode::Down => term_move(state, 1),
            KeyCode::PageUp => term_move(state, -(VIEWPORT as isize)),
            KeyCode::PageDown => term_move(state, VIEWPORT as isize),
            _ => {
                if Self::quick_edit(&mut state.input, &key) == Some(true) {
                    state.term_typed = state.input.value.clone();
                    self.quick_term_recompute();
                }
            }
        }
    }

    /// A pasted newline must not run a terminal command, so terminal mode keeps
    /// only the first line.
    pub(crate) fn paste_quick(&mut self, text: &str) {
        let Some(state) = self.quick.as_mut() else {
            return;
        };
        match state.mode {
            QuickMode::Palette => {
                state.input.insert_paste(text);
                self.quick_recompute();
            }
            QuickMode::Terminal => {
                state.input.insert_paste_first_line(text);
                state.term_typed = state.input.value.clone();
                self.quick_term_recompute();
            }
        }
    }

    fn close_quick(&mut self) {
        self.quick = None;
        self.mode = UiMode::Normal;
    }

    fn record_quick_history(&mut self, key: &str) {
        self.quick_history.retain(|k| k != key);
        self.quick_history.insert(0, key.to_string());
        self.quick_history.truncate(MAX_HISTORY);
        save_history(&self.quick_history);
    }

    fn record_terminal_history(&mut self, command: &str) {
        self.terminal_history.retain(|c| c != command);
        self.terminal_history.insert(0, command.to_string());
        self.terminal_history.truncate(MAX_HISTORY);
        save_terminal_history(&self.terminal_history);
    }

    pub(crate) fn quick_execute(&mut self, id: CommandId) {
        use CommandId::*;
        match id {
            FocusRepos => self.focus_pane(Pane::Repos),
            FocusFiles => self.focus_pane(Pane::Files),
            FocusDiff => self.focus_pane(Pane::Diff),
            AddRepository => self.open_add_repo(),
            RemoveRepository | RemoveWorktree | DeleteGroup => self.open_remove_row(),
            NewGroup => self.open_new_group(),
            RenameGroup | MoveToGroup => self.open_edit_row(),
            MoveGroupUp => self.move_current_group(-1),
            MoveGroupDown => self.move_current_group(1),
            FoldAllGroups => self.fold_all_groups(true),
            UnfoldAllGroups => self.fold_all_groups(false),
            Commit => self.open_commit(),
            DiscardFile => self.open_discard_file(),
            Push => self.open_push(),
            Pull => self.start_pull(),
            OpenWith => {
                if self.focus == Pane::Repos || !self.open_with_file() {
                    self.open_with_folder();
                }
            }
            FilterRepos => self.open_filter(),
            ClearFilter => self.clear_filter(),
            TogglePendingOnly => self.toggle_pending_only(),
            RefreshAll => self.refresh_all(false),
            ToggleWrap => self.toggle_wrap(),
            CycleRefLabels => self.cycle_ref_labels(),
            CycleCommitMeta => self.cycle_commit_meta(),
            ToggleSplitDiff => self.toggle_split(),
            TerminalCommand => self.open_quick_terminal(),
            ChangeTheme => self.open_theme_select(),
            EditConfig => self.request_edit_config(),
            OpenHelp => self.open_help(),
            Quit => self.should_quit = true,
        }
    }

    fn move_current_group(&mut self, delta: isize) {
        if let Some(Row::Group {
            name: Some(name), ..
        }) = self.current_row().cloned()
        {
            if self.store.move_group(&name, delta) {
                self.save_store();
                self.rebuild_rows();
            }
        }
    }

    fn fold_all_groups(&mut self, fold: bool) {
        for g in &mut self.store.groups {
            g.collapsed = fold;
        }
        self.save_store();
        self.rebuild_rows();
    }

    /// Opens config.toml in the editor through the terminal handover, seeding
    /// the commented template first.
    fn request_edit_config(&mut self) {
        crate::config::ensure_config_template();
        let path = crate::config::config_file_path();
        self.terminal_request = Some(TerminalRequest {
            cwd: crate::config::config_base_dir(),
            command: format!(
                "{} {}",
                self.cfg.editor_command(),
                shell_quote(&path.to_string_lossy())
            ),
            pause: false,
            name: None,
        });
    }
}

/// Single-quotes `s` for `sh -c`.
pub(crate) fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn move_cursor(state: &mut QuickState, delta: isize) {
    if state.items.is_empty() {
        return;
    }
    let last = state.items.len() - 1;
    state.cursor = (state.cursor as isize + delta).clamp(0, last as isize) as usize;
    if state.cursor < state.scroll {
        state.scroll = state.cursor;
    } else if state.cursor >= state.scroll + VIEWPORT {
        state.scroll = state.cursor + 1 - VIEWPORT;
    }
}

/// Moves the history selection and recalls it into the input; above the first
/// row the typed text comes back.
fn term_move(state: &mut QuickState, delta: isize) {
    if state.term_items.is_empty() {
        return;
    }
    let last = state.term_items.len() as isize - 1;
    let cur = state.term_selected.map(|i| i as isize).unwrap_or(-1);
    let next = (cur + delta).clamp(-1, last);
    state.term_selected = (next >= 0).then_some(next as usize);
    match state.term_selected {
        Some(i) => {
            state.input = TextInput::new(state.term_items[i].clone());
            if i < state.scroll {
                state.scroll = i;
            } else if i >= state.scroll + VIEWPORT {
                state.scroll = i + 1 - VIEWPORT;
            }
        }
        None => {
            state.input = TextInput::new(state.term_typed.clone());
            state.scroll = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(super::shell_quote("a'b"), r"'a'\''b'");
    }
}
