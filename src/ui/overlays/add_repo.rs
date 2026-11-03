//! Add Repository dialog (`+`): a folder and the group to file it under.
//!
//! The folder may be a repository (or any folder inside one, or a linked
//! worktree — each resolves to its main repository) or a parent folder, in
//! which case every repository found below it is added.

use crate::git::discover;
use crate::ui::components::input::TextInput;
use crate::ui::components::list_nav;
use crate::ui::components::modal::{
    button_row, centered_fixed_rect, dialog_width, draw_combo, dropdown_frame, form_input,
    modal_block, render_modal,
};
use crate::ui::components::text::truncate_w;
use crate::ui::overlays::message::MessageKind;
use crate::ui::tree::RowKey;
use crate::ui::{App, Level, UiMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Padding, Paragraph},
    Frame,
};
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddFocus {
    Path,
    Group,
    Ok,
    Cancel,
}

pub struct AddRepoDialog {
    pub path: TextInput,
    /// `None` = Ungrouped.
    pub groups: Vec<Option<String>>,
    pub group: usize,
    /// Open group dropdown and its cursor.
    pub popup: Option<usize>,
    pub focus: AddFocus,
    pub error: Option<String>,
}

const POPUP_MAX: usize = 8;

impl App {
    pub(crate) fn open_add_repo(&mut self) {
        let mut groups = vec![None];
        groups.extend(self.store.groups.iter().map(|g| Some(g.name.clone())));
        let current = self.current_group();
        let group = groups.iter().position(|g| *g == current).unwrap_or(0);
        self.add_repo = Some(AddRepoDialog {
            path: TextInput::selected(self.add_repo_prefill()),
            groups,
            group,
            popup: None,
            focus: AddFocus::Path,
            error: None,
        });
        self.mode = UiMode::AddRepo;
    }

    /// The launch folder when it is an unlisted repository, otherwise the
    /// parent of the selected repository (siblings are the likely next add).
    fn add_repo_prefill(&self) -> String {
        if let Some(dir) = &self.launch_dir {
            if let Ok(root) = discover::main_repo(dir) {
                if !self.store.contains(&root) {
                    return crate::config::collapse_home(&root);
                }
            }
        }
        self.current_repo_index()
            .and_then(|i| self.store.repos[i].path.parent().map(PathBuf::from))
            .map(|p| format!("{}/", crate::config::collapse_home(&p)))
            .unwrap_or_default()
    }

    pub(crate) fn on_key_add_repo(&mut self, key: KeyEvent) {
        let Some(d) = self.add_repo.as_mut() else {
            self.mode = UiMode::Normal;
            return;
        };
        if let Some(cursor) = d.popup {
            if let Some(mv) = list_nav::list_move(&key, true) {
                d.popup = Some(list_nav::apply(cursor, d.groups.len(), mv, POPUP_MAX));
                return;
            }
            match key.code {
                KeyCode::Esc => d.popup = None,
                KeyCode::Enter | KeyCode::Char(' ') => {
                    d.group = cursor;
                    d.popup = None;
                }
                _ => {}
            }
            return;
        }
        use AddFocus::*;
        match (key.code, d.focus) {
            (KeyCode::Esc, _) => self.close_add_repo(),
            (KeyCode::Tab, f) => d.focus = [Group, Ok, Cancel, Path][f as usize],
            (KeyCode::BackTab, f) => d.focus = [Cancel, Path, Group, Ok][f as usize],
            (KeyCode::Down | KeyCode::Enter, Path) => d.focus = Group,
            (KeyCode::Up, Group) => d.focus = Path,
            (KeyCode::Down, Group) => d.focus = Ok,
            (KeyCode::Up, Ok | Cancel) => d.focus = Group,
            (KeyCode::Enter | KeyCode::Char(' '), Group) => d.popup = Some(d.group),
            (KeyCode::Left, Ok | Cancel) => d.focus = Ok,
            (KeyCode::Right, Ok | Cancel) => d.focus = Cancel,
            (KeyCode::Enter, Cancel) => self.close_add_repo(),
            (KeyCode::Enter, Ok) => self.submit_add_repo(),
            (KeyCode::Left, Path) => d.path.move_left(),
            (KeyCode::Right, Path) => d.path.move_right(),
            (KeyCode::Home, Path) => d.path.home(),
            (KeyCode::End, Path) => d.path.end(),
            (KeyCode::Backspace, Path) => d.path.backspace(),
            (KeyCode::Delete, Path) => d.path.delete(),
            (KeyCode::Char(c), Path)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                d.path.insert_char(c);
                d.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn paste_add_repo(&mut self, text: &str) {
        if let Some(d) = self.add_repo.as_mut() {
            if d.focus == AddFocus::Path && d.popup.is_none() {
                d.path.insert_paste(text);
            }
        }
    }

    fn close_add_repo(&mut self) {
        self.add_repo = None;
        self.mode = UiMode::Normal;
    }

    fn submit_add_repo(&mut self) {
        let Some(d) = self.add_repo.as_mut() else {
            return;
        };
        let raw = d.path.value.trim();
        if raw.is_empty() {
            d.error = Some("Enter a folder".into());
            d.focus = AddFocus::Path;
            return;
        }
        let path = crate::config::expand_home(raw);
        if !path.is_dir() {
            d.error = Some(format!("Folder not found: {raw}"));
            d.focus = AddFocus::Path;
            return;
        }
        let group = d.groups[d.group].clone();
        self.close_add_repo();
        let shown = crate::config::collapse_home(&path);
        self.notify(Level::Info, format!("Looking for repositories in {shown}…"));
        self.store_sync.discoveries += 1;
        self.bg.spawn(move || {
            let result = discover::repos_for(&path).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| {
                app.store_sync.discoveries -= 1;
                app.apply_added(result, group);
            })
        });
    }

    fn apply_added(&mut self, result: Result<Vec<PathBuf>, String>, group: Option<String>) {
        let roots = match result {
            Ok(roots) => roots,
            Err(err) => {
                self.show_message(" Add Repository ", vec![err], MessageKind::Error);
                return;
            }
        };
        self.change_store(
            move |store| {
                store.require_group(group.as_deref())?;
                let added: Vec<_> = roots
                    .iter()
                    .filter(|root| store.add_repo((*root).clone(), group.clone()))
                    .cloned()
                    .collect();
                Ok((roots, added))
            },
            |app, (roots, added)| {
                let skipped = roots.len() - added.len();
                if added.is_empty() {
                    let text = match roots.as_slice() {
                        [one] => format!("{} is already listed", crate::config::collapse_home(one)),
                        _ => format!("All {} repositories are already listed", roots.len()),
                    };
                    app.notify(Level::Warn, text);
                    return;
                }
                app.select_row_key(Some(&RowKey::Repo(added[0].clone())));
                let mut text = match added.as_slice() {
                    [one] => format!("Added {}", crate::config::collapse_home(one)),
                    _ => format!("Added {} repositories", added.len()),
                };
                if skipped > 0 {
                    text.push_str(&format!(" ({skipped} already listed)"));
                }
                app.notify(Level::Ok, text);
            },
        );
    }
}

fn group_label(group: &Option<String>) -> &str {
    group.as_deref().unwrap_or("Ungrouped")
}

pub(crate) fn draw_add_repo(f: &mut Frame, app: &App) {
    let Some(d) = app.add_repo.as_ref() else {
        return;
    };
    let th = &app.theme;
    // Borders (2) + top padding (1) + path (3) + group (3) + notice (1) + blank (1) + buttons (1).
    let area = centered_fixed_rect(dialog_width(76, f.area()), 12, f.area());
    let block = modal_block(" Add Repository ", th.accent).padding(Padding::new(1, 1, 1, 0));
    let inner = render_modal(f, area, block, th);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let popup_open = d.popup.is_some();
    form_input(
        f,
        rows[0],
        " Folder ",
        &d.path,
        d.focus == AddFocus::Path && !popup_open,
        "Repository, a folder inside one, or a parent folder",
        th,
    );
    let value = draw_combo(
        f,
        rows[1],
        "Group",
        d.focus == AddFocus::Group || popup_open,
        th,
    );
    f.render_widget(
        Paragraph::new(truncate_w(
            group_label(&d.groups[d.group]),
            value.width as usize,
        )),
        value,
    );
    let notice = match &d.error {
        Some(err) => Span::styled(err.clone(), Style::default().fg(th.error)),
        None => Span::styled(
            "A parent folder adds every repository below it",
            th.soft_dim(),
        ),
    };
    f.render_widget(Paragraph::new(notice), rows[2]);
    let focus = match d.focus {
        AddFocus::Ok => Some(true),
        AddFocus::Cancel => Some(false),
        _ => None,
    };
    f.render_widget(
        button_row("Add", "Cancel", focus.filter(|_| !popup_open), th),
        rows[4],
    );

    if let Some(cursor) = d.popup {
        let view = d.groups.len().min(POPUP_MAX);
        let combo = rows[1];
        let popup = Rect::new(
            combo.x,
            combo.bottom() - 1,
            combo.width,
            (view as u16 + 2).min(f.area().height.saturating_sub(combo.bottom() - 1)),
        );
        let border = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);
        let list = dropdown_frame(f, popup, area, border, th);
        let offset = cursor.saturating_sub(view.saturating_sub(1));
        let w = list.width as usize;
        let items: Vec<ListItem> = d
            .groups
            .iter()
            .enumerate()
            .skip(offset)
            .take(view)
            .map(|(i, g)| {
                let mark = if i == d.group { "● " } else { "  " };
                let text = truncate_w(&format!(" {mark}{}", group_label(g)), w);
                let pad = w.saturating_sub(text.width());
                let style = if i == cursor {
                    Style::default()
                        .fg(th.selection_fg)
                        .bg(th.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{text}{}", " ".repeat(pad)),
                    style,
                )))
            })
            .collect();
        f.render_widget(List::new(items), list);
    }
}
