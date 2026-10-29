//! The one confirmation dialog. What it confirms ([`ConfirmAction`]) sets the
//! text and the action button; state, keys, and the default `Cancel` focus are
//! shared by every use.

use crate::git::ops;
use crate::ui::components::modal::{
    button_row, centered_fixed_rect, dialog_width, modal_block, render_modal,
};
use crate::ui::components::text::wrap_w;
use crate::ui::tree::Row;
use crate::ui::{App, Level, UiMode};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::Line,
    widgets::{Padding, Paragraph},
    Frame,
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    /// Drops a repository from the list (files untouched).
    RemoveRepo(PathBuf),
    /// Deletes a group; its repositories become ungrouped.
    DeleteGroup(String),
    /// `git worktree remove` (`force` discards uncommitted changes).
    RemoveWorktree {
        repo: PathBuf,
        path: PathBuf,
        force: bool,
    },
    /// `git push`, to the upstream or to `new` (which becomes the upstream).
    Push {
        target: PathBuf,
        new: Option<crate::git::remote::NewUpstream>,
    },
}

pub struct ConfirmDialog {
    pub action: ConfirmAction,
    pub title: String,
    pub lines: Vec<String>,
    pub ok_label: String,
    /// Destructive: drawn in the error color.
    pub danger: bool,
    pub ok_focused: bool,
}

impl App {
    /// `ctrl+d` / `delete` on the repository pane: the action depends on the row.
    pub(crate) fn open_remove_row(&mut self) {
        let Some(row) = self.current_row().cloned() else {
            return;
        };
        let dialog = match row {
            Row::Group { name: None, .. } => {
                self.notify(
                    Level::Info,
                    "Ungrouped is not a group and cannot be deleted",
                );
                return;
            }
            Row::Group {
                name: Some(name),
                total,
                ..
            } => ConfirmDialog {
                title: " Delete Group ".into(),
                lines: vec![
                    format!("Delete the group \"{name}\"?"),
                    format!("Its {total} repositories move to Ungrouped. No files are touched."),
                ],
                ok_label: "Delete".into(),
                danger: false,
                ok_focused: false,
                action: ConfirmAction::DeleteGroup(name),
            },
            Row::Repo { idx, .. } => {
                let path = self.store.repos[idx].path.clone();
                ConfirmDialog {
                    title: " Remove Repository ".into(),
                    lines: vec![
                        format!(
                            "Remove {} from the list?",
                            crate::config::collapse_home(&path)
                        ),
                        "The folder and its git data stay on disk.".into(),
                    ],
                    ok_label: "Remove".into(),
                    danger: false,
                    ok_focused: false,
                    action: ConfirmAction::RemoveRepo(path),
                }
            }
            Row::Worktree { repo, path, .. } => {
                let changed = self
                    .statuses
                    .get(&path)
                    .and_then(|t| t.status.as_ref())
                    .map(|s| s.counts().changed)
                    .unwrap_or(0);
                let mut lines = vec![format!(
                    "Remove the worktree {}?",
                    crate::config::collapse_home(&path)
                )];
                if changed > 0 {
                    lines.push(format!(
                        "It has {changed} uncommitted changes. They are deleted with the folder."
                    ));
                } else {
                    lines.push("The folder is deleted. Its branch is kept.".into());
                }
                ConfirmDialog {
                    title: " Remove Worktree ".into(),
                    lines,
                    ok_label: if changed > 0 {
                        "Force Remove"
                    } else {
                        "Remove"
                    }
                    .into(),
                    danger: true,
                    ok_focused: false,
                    action: ConfirmAction::RemoveWorktree {
                        repo: self.store.repos[repo].path.clone(),
                        path,
                        force: changed > 0,
                    },
                }
            }
        };
        self.confirm = Some(dialog);
        self.mode = UiMode::Confirm;
    }

    pub(crate) fn on_key_confirm(&mut self, key: KeyEvent) {
        let Some(dialog) = self.confirm.as_mut() else {
            self.mode = UiMode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc => self.close_confirm(),
            KeyCode::Left => dialog.ok_focused = true,
            KeyCode::Right => dialog.ok_focused = false,
            KeyCode::Tab | KeyCode::BackTab => dialog.ok_focused = !dialog.ok_focused,
            KeyCode::Enter => {
                if dialog.ok_focused {
                    let action = dialog.action.clone();
                    self.close_confirm();
                    self.run_confirmed(action);
                } else {
                    self.close_confirm();
                }
            }
            _ => {}
        }
    }

    fn close_confirm(&mut self) {
        self.confirm = None;
        self.mode = UiMode::Normal;
    }

    fn run_confirmed(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::Push { target, new } => {
                self.run_remote(target, crate::ui::sync::RemoteOp::Push, new);
            }
            ConfirmAction::RemoveRepo(path) => {
                if self.store.remove_repo(&path) {
                    self.statuses.remove(&path);
                    self.save_store();
                    self.rebuild_rows();
                    let name = crate::config::collapse_home(&path);
                    self.notify(Level::Ok, format!("Removed {name} from the list"));
                }
            }
            ConfirmAction::DeleteGroup(name) => {
                if self.store.group_exists(&name) {
                    let moved = self.store.delete_group(&name);
                    self.save_store();
                    self.rebuild_rows();
                    self.notify(
                        Level::Ok,
                        format!("Deleted group {name} ({moved} repositories ungrouped)"),
                    );
                }
            }
            ConfirmAction::RemoveWorktree { repo, path, force } => {
                self.notify(Level::Info, "Removing worktree…");
                self.bg.spawn(move || {
                    let result = ops::remove_worktree(&repo, &path, force)
                        .or_else(|err| {
                            // A folder deleted by hand leaves only git's record.
                            if path.exists() {
                                Err(err)
                            } else {
                                ops::prune_worktrees(&repo)
                            }
                        })
                        .map_err(|e| e.to_string());
                    Box::new(move |app: &mut App| {
                        match result {
                            Ok(()) => {
                                let name = crate::config::collapse_home(&path);
                                app.notify(Level::Ok, format!("Removed worktree {name}"));
                            }
                            Err(err) => app.show_message(
                                " Remove Worktree Failed ",
                                vec![err],
                                crate::ui::overlays::message::MessageKind::Error,
                            ),
                        }
                        app.request_scan(repo, true);
                    })
                });
            }
        }
    }
}

pub(crate) fn draw_confirm(f: &mut Frame, app: &App) {
    let Some(d) = app.confirm.as_ref() else {
        return;
    };
    let th = &app.theme;
    let color = if d.danger { th.error } else { th.warning };
    let full = f.area();
    let w = dialog_width(68, full);
    // Margins (2) + borders (2) + padding (2).
    let inner_w = (w as usize).saturating_sub(6);
    let body: Vec<Line> = d
        .lines
        .iter()
        .flat_map(|l| {
            // An empty line is a deliberate gap; wrapping would drop it.
            if l.is_empty() {
                vec![String::new()]
            } else {
                wrap_w(l, inner_w)
            }
        })
        .map(Line::from)
        .collect();
    // Borders (2) + top padding (1) + body + blank (1) + buttons (1).
    let h = (body.len() as u16 + 5).min(full.height);
    let area = centered_fixed_rect(w, h, full);
    let block = modal_block(&d.title, color).padding(Padding::new(1, 1, 1, 0));
    let inner = render_modal(f, area, block, th);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(body), rows[0]);
    f.render_widget(
        button_row(&d.ok_label, "Cancel", Some(d.ok_focused), th),
        rows[2],
    );
}
