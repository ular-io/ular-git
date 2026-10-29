//! Group dialogs: the name form (New Group / Rename Group) and the Move to
//! Group picker.

use crate::ui::components::input::TextInput;
use crate::ui::components::list_nav;
use crate::ui::components::modal::{
    button_row, centered_fixed_rect, dialog_width, form_input, modal_block, render_modal,
};
use crate::ui::components::text::truncate_w;
use crate::ui::tree::{Row, RowKey};
use crate::ui::{App, Level, UiMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Padding, Paragraph},
    Frame,
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamePurpose {
    New,
    Rename(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFocus {
    Input,
    Ok,
    Cancel,
}

pub struct GroupNameDialog {
    pub purpose: NamePurpose,
    pub input: TextInput,
    pub focus: NameFocus,
    pub error: Option<String>,
    /// Repository moved into the new group once it is created (opened from
    /// the picker's `[NEW GROUP]` row).
    pub assign: Option<PathBuf>,
}

pub struct GroupPickDialog {
    pub repo: PathBuf,
    /// `None` = no group. The `[NEW GROUP]` row follows at `options.len()`.
    pub options: Vec<Option<String>>,
    pub current: Option<String>,
    pub cursor: usize,
}

impl App {
    pub(crate) fn open_new_group(&mut self) {
        self.open_group_name(NamePurpose::New, String::new(), None);
    }

    fn open_group_name(&mut self, purpose: NamePurpose, value: String, assign: Option<PathBuf>) {
        let input = if value.is_empty() {
            TextInput::new(value)
        } else {
            TextInput::selected(value)
        };
        self.group_name = Some(GroupNameDialog {
            purpose,
            input,
            focus: NameFocus::Input,
            error: None,
            assign,
        });
        self.mode = UiMode::GroupName;
    }

    /// `ctrl+e`: rename a group, or move a repository to another group.
    pub(crate) fn open_edit_row(&mut self) {
        match self.current_row().cloned() {
            Some(Row::Group {
                name: Some(name), ..
            }) => self.open_group_name(NamePurpose::Rename(name.clone()), name, None),
            Some(Row::Group { name: None, .. }) => {
                self.notify(Level::Info, "Ungrouped cannot be renamed");
            }
            Some(Row::Repo { idx, .. }) | Some(Row::Worktree { repo: idx, .. }) => {
                let repo = &self.store.repos[idx];
                let mut options = vec![None];
                options.extend(self.store.groups.iter().map(|g| Some(g.name.clone())));
                let cursor = options.iter().position(|o| *o == repo.group).unwrap_or(0);
                self.group_pick = Some(GroupPickDialog {
                    repo: repo.path.clone(),
                    current: repo.group.clone(),
                    options,
                    cursor,
                });
                self.mode = UiMode::GroupPick;
            }
            None => {}
        }
    }

    pub(crate) fn on_key_group_name(&mut self, key: KeyEvent) {
        let Some(d) = self.group_name.as_mut() else {
            self.mode = UiMode::Normal;
            return;
        };
        use NameFocus::*;
        match (key.code, d.focus) {
            (KeyCode::Esc, _) => self.close_group_name(),
            (KeyCode::Tab, f) => d.focus = [Ok, Cancel, Input][f as usize],
            (KeyCode::BackTab, f) => d.focus = [Cancel, Input, Ok][f as usize],
            (KeyCode::Down, Input) | (KeyCode::Enter, Input) => d.focus = Ok,
            (KeyCode::Up, Ok | Cancel) => d.focus = Input,
            (KeyCode::Left, Ok | Cancel) => d.focus = Ok,
            (KeyCode::Right, Ok | Cancel) => d.focus = Cancel,
            (KeyCode::Enter, Cancel) => self.close_group_name(),
            (KeyCode::Enter, Ok) => self.submit_group_name(),
            (KeyCode::Left, Input) => d.input.move_left(),
            (KeyCode::Right, Input) => d.input.move_right(),
            (KeyCode::Home, Input) => d.input.home(),
            (KeyCode::End, Input) => d.input.end(),
            (KeyCode::Backspace, Input) => d.input.backspace(),
            (KeyCode::Delete, Input) => d.input.delete(),
            (KeyCode::Char(c), Input)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                d.input.insert_char(c);
                d.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn paste_group_name(&mut self, text: &str) {
        if let Some(d) = self.group_name.as_mut() {
            if d.focus == NameFocus::Input {
                d.input.insert_paste(text);
            }
        }
    }

    fn close_group_name(&mut self) {
        self.group_name = None;
        self.mode = UiMode::Normal;
    }

    fn submit_group_name(&mut self) {
        let Some(d) = self.group_name.as_mut() else {
            return;
        };
        let name = d.input.value.trim().to_string();
        let result = match &d.purpose {
            NamePurpose::New => self.store.add_group(&name),
            NamePurpose::Rename(old) => self.store.rename_group(old, &name),
        };
        if let Err(err) = result {
            d.error = Some(err);
            d.focus = NameFocus::Input;
            return;
        }
        let (purpose, assign) = (d.purpose.clone(), d.assign.take());
        self.close_group_name();
        if let Some(repo) = assign.as_ref() {
            self.store.set_group(repo, Some(name.clone()));
        }
        self.save_store();
        self.rebuild_rows();
        let key = match assign {
            Some(repo) => RowKey::Repo(repo),
            None => RowKey::Group(Some(name.clone())),
        };
        self.select_row_key(Some(&key));
        match purpose {
            NamePurpose::New => self.notify(Level::Ok, format!("Created group {name}")),
            NamePurpose::Rename(old) => {
                self.notify(Level::Ok, format!("Renamed group {old} to {name}"))
            }
        }
    }

    pub(crate) fn on_key_group_pick(&mut self, key: KeyEvent) {
        let Some(d) = self.group_pick.as_mut() else {
            self.mode = UiMode::Normal;
            return;
        };
        let len = d.options.len() + 1;
        if let Some(mv) = list_nav::list_move(&key, true) {
            d.cursor = list_nav::apply(d.cursor, len, mv, len);
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.group_pick = None;
                self.mode = UiMode::Normal;
            }
            KeyCode::Enter => {
                let Some(d) = self.group_pick.take() else {
                    return;
                };
                self.mode = UiMode::Normal;
                match d.options.get(d.cursor).cloned() {
                    None => self.open_group_name(NamePurpose::New, String::new(), Some(d.repo)),
                    Some(group) => {
                        self.store.set_group(&d.repo, group.clone());
                        self.save_store();
                        self.rebuild_rows();
                        self.select_row_key(Some(&RowKey::Repo(d.repo.clone())));
                        let to = group.unwrap_or_else(|| "Ungrouped".into());
                        self.notify(Level::Ok, format!("Moved to {to}"));
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn draw_group_name(f: &mut Frame, app: &App) {
    let Some(d) = app.group_name.as_ref() else {
        return;
    };
    let th = &app.theme;
    let title = match d.purpose {
        NamePurpose::New => " New Group ",
        NamePurpose::Rename(_) => " Rename Group ",
    };
    // Borders (2) + top padding (1) + input (3) + notice (1) + blank (1) + buttons (1).
    let area = centered_fixed_rect(dialog_width(56, f.area()), 9, f.area());
    let block = modal_block(title, th.accent).padding(Padding::new(1, 1, 1, 0));
    let inner = render_modal(f, area, block, th);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    form_input(
        f,
        rows[0],
        " Name ",
        &d.input,
        d.focus == NameFocus::Input,
        "Group name",
        th,
    );
    if let Some(err) = &d.error {
        f.render_widget(
            Paragraph::new(Span::styled(
                truncate_w(err, rows[1].width as usize),
                Style::default().fg(th.error),
            )),
            rows[1],
        );
    }
    let focus = match d.focus {
        NameFocus::Input => None,
        NameFocus::Ok => Some(true),
        NameFocus::Cancel => Some(false),
    };
    f.render_widget(button_row("Save", "Cancel", focus, th), rows[3]);
}

pub(crate) fn draw_group_pick(f: &mut Frame, app: &App) {
    let Some(d) = app.group_pick.as_ref() else {
        return;
    };
    let th = &app.theme;
    let total = d.options.len() + 1;
    let view = total.min(14);
    let area = centered_fixed_rect(dialog_width(46, f.area()), view as u16 + 4, f.area());
    let block = modal_block(" Move to Group ", th.accent).padding(Padding::new(1, 1, 1, 1));
    let inner = render_modal(f, area, block, th);
    let offset = d.cursor.saturating_sub(view.saturating_sub(1));
    let w = inner.width as usize;
    let items: Vec<ListItem> = (0..total)
        .skip(offset)
        .take(view)
        .map(|i| {
            let (label, fixed) = match d.options.get(i) {
                Some(Some(name)) => (name.clone(), false),
                Some(None) => ("Ungrouped".to_string(), false),
                None => ("[NEW GROUP]".to_string(), true),
            };
            let mark = if d.options.get(i).is_some_and(|o| *o == d.current) {
                "● "
            } else {
                "  "
            };
            let text = truncate_w(&format!("{mark}{label}"), w);
            let pad = w.saturating_sub(unicode_width::UnicodeWidthStr::width(text.as_str()));
            let style = if i == d.cursor {
                Style::default()
                    .fg(th.selection_fg)
                    .bg(th.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else if fixed {
                Style::default().fg(th.accent)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{text}{}", " ".repeat(pad)),
                style,
            )))
        })
        .collect();
    f.render_widget(List::new(items), inner);
}
