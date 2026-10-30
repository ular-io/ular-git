//! Open With: `enter` on a repository or worktree row, or on a changed file,
//! lists the apps from config.toml (`[[apps]]` / `[[file_apps]]`) and runs the
//! chosen one in the working tree. Terminal apps take the terminal over until
//! they exit; GUI apps start in the background and the screen stays.

use crate::config::Launcher;
use crate::ui::components::list_nav;
use crate::ui::components::modal::{centered_fixed_rect, dialog_width, modal_block, render_modal};
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::truncate_w;
use crate::ui::quick::input::shell_quote;
use crate::ui::{App, Level, TerminalRequest, UiMode};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Padding},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

/// What the dialog opens; picks the app list and the remembered app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    Folder,
    File,
}

pub struct OpenWithDialog {
    pub kind: OpenKind,
    /// Working tree the app runs in.
    pub cwd: PathBuf,
    /// Folder or file put in for `{path}`.
    pub path: PathBuf,
    pub cursor: usize,
}

/// Last app used per kind, so the next Open With starts on it.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastApps {
    pub folder: Option<String>,
    pub file: Option<String>,
}

fn last_apps_path() -> PathBuf {
    crate::config::config_base_dir().join("open_with.json")
}

/// Unit tests never read the user's file.
pub(crate) fn load_last_apps() -> LastApps {
    if cfg!(test) {
        return LastApps::default();
    }
    std::fs::read_to_string(last_apps_path())
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_last_apps(last: &LastApps) {
    if cfg!(test) {
        return;
    }
    if let Ok(data) = serde_json::to_string_pretty(last) {
        let _ = crate::config::write_atomic(&last_apps_path(), data.as_bytes());
    }
}

impl App {
    fn launchers(&self, kind: OpenKind) -> &[Launcher] {
        match kind {
            OpenKind::Folder => &self.cfg.launchers,
            OpenKind::File => &self.cfg.file_launchers,
        }
    }

    /// `enter` on a repository or worktree row.
    pub(crate) fn open_with_folder(&mut self) {
        let Some(target) = self.current_target() else {
            return;
        };
        self.open_with(OpenKind::Folder, target.clone(), target);
    }

    /// `enter` on a changed file. Returns false when the selection is not a
    /// changed file (a commit or `… more`), so `enter` keeps its other use.
    pub(crate) fn open_with_file(&mut self) -> bool {
        let (Some(target), Some(file)) = (self.current_target(), self.selected_file()) else {
            return false;
        };
        let path = target.join(&file.path);
        if !path.exists() {
            self.notify(Level::Info, format!("{} is deleted", file.path));
            return true;
        }
        self.open_with(OpenKind::File, target, path);
        true
    }

    fn open_with(&mut self, kind: OpenKind, cwd: PathBuf, path: PathBuf) {
        let last = match kind {
            OpenKind::Folder => self.last_apps.folder.as_deref(),
            OpenKind::File => self.last_apps.file.as_deref(),
        };
        let cursor = last
            .and_then(|name| self.launchers(kind).iter().position(|l| l.name == name))
            .unwrap_or(0);
        self.open_with = Some(OpenWithDialog {
            kind,
            cwd,
            path,
            cursor,
        });
        self.mode = UiMode::OpenWith;
    }

    pub(crate) fn on_key_open_with(&mut self, key: KeyEvent) {
        let Some(d) = self.open_with.as_mut() else {
            self.mode = UiMode::Normal;
            return;
        };
        let len = match d.kind {
            OpenKind::Folder => self.cfg.launchers.len(),
            OpenKind::File => self.cfg.file_launchers.len(),
        };
        if let Some(mv) = list_nav::list_move(&key, true) {
            d.cursor = list_nav::apply(d.cursor, len, mv, VIEW_MAX);
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.open_with = None;
                self.mode = UiMode::Normal;
            }
            KeyCode::Enter => {
                if let Some(d) = self.open_with.take() {
                    self.mode = UiMode::Normal;
                    self.launch(d);
                }
            }
            _ => {}
        }
    }

    fn launch(&mut self, d: OpenWithDialog) {
        let Some(app) = self.launchers(d.kind).get(d.cursor).cloned() else {
            return;
        };
        let slot = match d.kind {
            OpenKind::Folder => &mut self.last_apps.folder,
            OpenKind::File => &mut self.last_apps.file,
        };
        if slot.as_deref() != Some(app.name.as_str()) {
            *slot = Some(app.name.clone());
            save_last_apps(&self.last_apps);
        }
        let command = app
            .command
            .replace("{path}", &shell_quote(&d.path.to_string_lossy()));
        if !app.gui {
            self.terminal_request = Some(TerminalRequest {
                cwd: d.cwd,
                command,
                pause: false,
                name: Some(app.name),
            });
            return;
        }
        self.notify(Level::Info, format!("Opening {}…", app.name));
        let (cwd, name) = (d.cwd, app.name);
        self.bg.spawn(move || {
            let result = crate::launch::spawn_detached(&cwd, &command);
            Box::new(move |app: &mut App| match result {
                Ok(()) => app.notify(Level::Ok, format!("Opened {name}")),
                Err(err) => app.notify(Level::Error, format!("{name}: {err}")),
            })
        });
    }
}

const VIEW_MAX: usize = 14;

/// The app list: name left, `terminal` / `gui` right and faint. No buttons;
/// `enter` runs the selected app.
pub(crate) fn draw_open_with(f: &mut Frame, app: &App) {
    let Some(d) = app.open_with.as_ref() else {
        return;
    };
    let th = &app.theme;
    let apps = app.launchers(d.kind);
    let total = apps.len();
    let view = total.clamp(1, VIEW_MAX);
    // Borders (2) + top and bottom padding (1 each) + rows.
    let h = (view as u16).saturating_add(4).min(f.area().height);
    let area = centered_fixed_rect(dialog_width(46, f.area()), h, f.area());
    let name = d
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = format!(
        " Open With · {} ",
        truncate_w(&name, (area.width as usize).saturating_sub(18))
    );
    let block = modal_block(&title, th.accent).padding(Padding::new(1, 1, 1, 1));
    let inner = render_modal(f, area, block, th);
    if inner.height < 1 || inner.width < 8 {
        return;
    }
    let list_area = Rect::new(inner.x, inner.y, inner.width, view as u16);
    let offset = d.cursor.saturating_sub(view - 1);
    let w = list_area.width as usize;
    let items: Vec<ListItem> = apps
        .iter()
        .enumerate()
        .skip(offset)
        .take(view)
        .map(|(i, l)| {
            let (name_style, tag_style) = if i == d.cursor {
                let s = Style::default().fg(th.selection_fg).bg(th.selection_bg);
                (s.add_modifier(Modifier::BOLD), s)
            } else {
                (Style::default(), th.soft_dim())
            };
            let tag = if l.gui { "gui " } else { "terminal " };
            let label = format!(" {}", truncate_w(&l.name, w.saturating_sub(12)));
            let pad = w.saturating_sub(label.width() + tag.width());
            ListItem::new(Line::from(vec![
                Span::styled(label, name_style),
                Span::styled(" ".repeat(pad), name_style),
                Span::styled(tag, tag_style),
            ]))
        })
        .collect();
    f.render_widget(List::new(items), list_area);
    if total > view {
        let sb = Rect::new(
            area.x,
            list_area.y.saturating_sub(1),
            area.width.saturating_sub(1),
            view as u16 + 2,
        );
        draw_vscrollbar(f, sb, true, offset, total, view, th);
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Launcher;
    use crate::git::test_repo::TestRepo;
    use crate::ui::test_support::{key, scanned_app, store_of};
    use crate::ui::{Pane, UiMode};
    use crossterm::event::KeyCode;

    fn app_of(name: &str, command: &str, gui: bool) -> Launcher {
        Launcher {
            name: name.into(),
            command: command.into(),
            gui,
        }
    }

    #[test]
    fn enter_on_a_repository_runs_the_picked_terminal_app_in_it() {
        let a = TestRepo::new();
        let mut app = scanned_app(store_of(&[&a.root]));
        app.cfg.launchers = vec![
            app_of("Open Folder", "open .", true),
            app_of("tool", "tool --dir {path}", false),
        ];
        app.press(&[key(KeyCode::Enter)]);
        assert_eq!(app.mode, UiMode::OpenWith);
        assert_eq!(app.focus, Pane::Repos, "enter no longer moves to Files");
        app.press(&[key(KeyCode::Down), key(KeyCode::Down), key(KeyCode::Enter)]);
        assert_eq!(app.mode, UiMode::Normal);
        let req = app.terminal_request.take().expect("terminal handover");
        assert_eq!(req.cwd, a.root);
        assert_eq!(req.command, format!("tool --dir '{}'", a.root.display()));
        assert!(!req.pause);
        assert_eq!(
            req.name.as_deref(),
            Some("tool"),
            "the notice names the app"
        );
        // The next Open With starts on the app used last.
        app.press(&[key(KeyCode::Enter)]);
        assert_eq!(app.open_with.as_ref().unwrap().cursor, 1);
        app.press(&[key(KeyCode::Esc)]);
        assert_eq!(app.mode, UiMode::Normal);
        assert!(app.terminal_request.is_none());
    }

    #[test]
    fn gui_apps_start_in_the_background_and_report_failures() {
        let a = TestRepo::new();
        let mut app = scanned_app(store_of(&[&a.root]));
        app.cfg.launchers = vec![app_of("ok", "true", true), app_of("bad", "exit 3", true)];
        app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
        assert!(app.terminal_request.is_none(), "GUI apps keep the screen");
        app.wait_background(10);
        assert_eq!(app.notice.as_ref().unwrap().text, "Opened ok");
        app.press(&[key(KeyCode::Enter), key(KeyCode::Down), key(KeyCode::Enter)]);
        app.wait_background(10);
        assert_eq!(app.notice.as_ref().unwrap().text, "bad: exited with code 3");
    }

    #[test]
    fn enter_on_a_changed_file_opens_it_and_on_a_commit_moves_to_the_diff() {
        let a = TestRepo::new();
        a.write("a.txt", "changed\n");
        let mut app = scanned_app(store_of(&[&a.root]));
        app.wait_background(10);
        app.focus = Pane::Files;
        app.press(&[key(KeyCode::Enter)]);
        assert_eq!(app.mode, UiMode::OpenWith);
        app.press(&[key(KeyCode::Enter)]);
        let req = app.terminal_request.take().expect("vim by default");
        assert_eq!(req.cwd, a.root);
        assert_eq!(
            req.command,
            format!("vim '{}'", a.root.join("a.txt").display())
        );
        // The row below the changed file is a history commit.
        app.press(&[key(KeyCode::Down), key(KeyCode::Enter)]);
        assert_eq!(app.mode, UiMode::Normal);
        assert_eq!(app.focus, Pane::Diff);
    }
}
