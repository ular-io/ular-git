//! Commit dialog (`c`): commits the uncommitted changes of the selected
//! working tree — what is staged, or everything including untracked files.

use crate::git::ops::{self, CommitScope};
use crate::git::Counts;
use crate::ui::components::input::TextInput;
use crate::ui::components::modal::{
    button_row, centered_fixed_rect, dialog_width, form_input, modal_block, render_modal,
};
use crate::ui::components::text::truncate_w;
use crate::ui::overlays::message::MessageKind;
use crate::ui::{App, Level, UiMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
    Frame,
};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitFocus {
    Scope,
    Message,
    Ok,
    Cancel,
}

pub struct CommitDialog {
    pub target: PathBuf,
    pub name: String,
    pub branch: String,
    pub counts: Counts,
    pub scope: CommitScope,
    pub message: TextInput,
    pub focus: CommitFocus,
    pub error: Option<String>,
}

impl CommitDialog {
    /// "Staged only" needs something staged.
    fn staged_available(&self) -> bool {
        self.counts.staged > 0
    }
}

impl App {
    pub(crate) fn open_commit(&mut self) {
        let Some(target) = self.current_target() else {
            self.notify(Level::Info, "Select a repository to commit");
            return;
        };
        let Some(status) = self.current_tree().and_then(|t| t.status.as_ref()) else {
            self.notify(Level::Info, "Status is still loading");
            return;
        };
        let counts = status.counts();
        if counts.conflicts > 0 {
            self.notify(Level::Warn, "Resolve merge conflicts before committing");
            return;
        }
        if status.is_clean() {
            self.notify(Level::Info, "Nothing to commit");
            return;
        }
        let branch = status
            .branch
            .head
            .clone()
            .unwrap_or_else(|| "(detached HEAD)".into());
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.commit = Some(CommitDialog {
            target,
            name,
            branch,
            counts,
            scope: if counts.staged > 0 {
                CommitScope::Staged
            } else {
                CommitScope::All
            },
            message: TextInput::new(String::new()),
            focus: CommitFocus::Message,
            error: None,
        });
        self.mode = UiMode::Commit;
    }

    pub(crate) fn on_key_commit(&mut self, key: KeyEvent) {
        let Some(d) = self.commit.as_mut() else {
            self.mode = UiMode::Normal;
            return;
        };
        use CommitFocus::*;
        match (key.code, d.focus) {
            (KeyCode::Esc, _) => self.close_commit(),
            (KeyCode::Tab, f) => d.focus = [Message, Ok, Cancel, Scope][f as usize],
            (KeyCode::BackTab, f) => d.focus = [Cancel, Scope, Message, Ok][f as usize],
            (KeyCode::Down, Scope) => d.focus = Message,
            (KeyCode::Up, Message) => d.focus = Scope,
            (KeyCode::Down | KeyCode::Enter, Message) => d.focus = Ok,
            (KeyCode::Up, Ok | Cancel) => d.focus = Message,
            (KeyCode::Left, Scope) if d.staged_available() => d.scope = CommitScope::Staged,
            (KeyCode::Right, Scope) => d.scope = CommitScope::All,
            (KeyCode::Char(' ') | KeyCode::Enter, Scope) => {
                d.scope = match d.scope {
                    CommitScope::All if d.staged_available() => CommitScope::Staged,
                    _ => CommitScope::All,
                }
            }
            (KeyCode::Left, Ok | Cancel) => d.focus = Ok,
            (KeyCode::Right, Ok | Cancel) => d.focus = Cancel,
            (KeyCode::Enter, Cancel) => self.close_commit(),
            (KeyCode::Enter, Ok) => self.submit_commit(),
            (KeyCode::Left, Message) => d.message.move_left(),
            (KeyCode::Right, Message) => d.message.move_right(),
            (KeyCode::Home, Message) => d.message.home(),
            (KeyCode::End, Message) => d.message.end(),
            (KeyCode::Backspace, Message) => d.message.backspace(),
            (KeyCode::Delete, Message) => d.message.delete(),
            (KeyCode::Char(c), Message)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                d.message.insert_char(c);
                d.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn paste_commit(&mut self, text: &str) {
        if let Some(d) = self.commit.as_mut() {
            if d.focus == CommitFocus::Message {
                d.message.insert_paste(text);
            }
        }
    }

    fn close_commit(&mut self) {
        self.commit = None;
        self.mode = UiMode::Normal;
    }

    fn submit_commit(&mut self) {
        let Some(d) = self.commit.as_mut() else {
            return;
        };
        let message = d.message.value.trim().to_string();
        if message.is_empty() {
            d.error = Some("Enter a commit message".into());
            d.focus = CommitFocus::Message;
            return;
        }
        let (target, scope, name) = (d.target.clone(), d.scope, d.name.clone());
        self.close_commit();
        self.notify(Level::Info, format!("Committing {name}…"));
        self.bg.spawn(move || {
            let result = ops::commit(&target, &message, scope).map_err(|e| e.to_string());
            Box::new(move |app: &mut App| {
                match result {
                    Ok(summary) => app.notify(Level::Ok, format!("{name}: committed {summary}")),
                    Err(err) => app.show_message(" Commit Failed ", vec![err], MessageKind::Error),
                }
                app.request_scan(target, true);
            })
        });
    }
}

fn radio(
    label: &str,
    on: bool,
    enabled: bool,
    focused: bool,
    th: &crate::theme::Theme,
) -> Vec<Span<'static>> {
    let mark = if on { "(•) " } else { "( ) " };
    let style = if !enabled {
        Style::default().fg(th.dim)
    } else if on && focused {
        Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
    } else if on {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        th.soft_dim()
    };
    vec![Span::styled(format!("{mark}{label}"), style)]
}

pub(crate) fn draw_commit(f: &mut Frame, app: &App) {
    let Some(d) = app.commit.as_ref() else {
        return;
    };
    let th = &app.theme;
    // Borders (2) + top padding (1) + branch (1) + changes (1) + blank (1)
    // + scope (1) + blank (1) + message (3) + notice (1) + blank (1) + buttons (1).
    let area = centered_fixed_rect(dialog_width(76, f.area()), 14, f.area());
    let title = format!(" Commit · {} ", d.name);
    let block = modal_block(&title, th.accent).padding(Padding::new(1, 1, 1, 0));
    let inner = render_modal(f, area, block, th);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let w = inner.width as usize;
    let label = |s: &'static str| Span::styled(format!("{s:<9}"), th.soft_dim());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            label("Branch"),
            Span::styled(
                truncate_w(&d.branch, w.saturating_sub(9)),
                Style::default().fg(th.branch).add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );
    let c = d.counts;
    let changes = format!(
        "{} staged · {} unstaged · {} untracked",
        c.staged, c.unstaged, c.untracked
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![label("Changes"), Span::raw(changes)])),
        rows[1],
    );
    let scope_focused = d.focus == CommitFocus::Scope;
    let mut scope = vec![Span::styled(
        format!("{:<9}", "Include"),
        if scope_focused {
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
        } else {
            th.soft_dim()
        },
    )];
    scope.extend(radio(
        "Staged only",
        d.scope == CommitScope::Staged,
        d.staged_available(),
        scope_focused,
        th,
    ));
    scope.push(Span::raw("   "));
    scope.extend(radio(
        "All changes (adds untracked files)",
        d.scope == CommitScope::All,
        true,
        scope_focused,
        th,
    ));
    f.render_widget(Paragraph::new(Line::from(scope)), rows[3]);
    form_input(
        f,
        rows[5],
        " Message ",
        &d.message,
        d.focus == CommitFocus::Message,
        "Commit message",
        th,
    );
    if let Some(err) = &d.error {
        f.render_widget(
            Paragraph::new(Span::styled(err.clone(), Style::default().fg(th.error))),
            rows[6],
        );
    }
    let focus = match d.focus {
        CommitFocus::Ok => Some(true),
        CommitFocus::Cancel => Some(false),
        _ => None,
    };
    f.render_widget(button_row("Commit", "Cancel", focus, th), rows[8]);
}
