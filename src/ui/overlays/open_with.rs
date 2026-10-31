//! Open With: `enter` on a repository or worktree row lists the folder apps
//! (`[[apps]]`). `enter` on a file — a changed file, or one in a commit's or
//! the uncommitted changes' file list — adds tabs on top (see
//! [`crate::ui::file_versions`]): Diff lists the diff apps (`[[diff_apps]]`),
//! the other tabs the file apps (`[[file_apps]]`) for the file before, after,
//! or in the working tree. Versions from git are written to read-only
//! temporary files first (in the background). Terminal apps take the
//! terminal over until they exit; GUI apps start in the background and the
//! screen stays.

use crate::config::Launcher;
use crate::ui::components::list_nav;
use crate::ui::components::modal::{centered_fixed_rect, dialog_width, modal_block, render_modal};
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::truncate_w;
use crate::ui::file_versions::{self, OpenTarget, Tab, TabKind, Version};
use crate::ui::files::FileItem;
use crate::ui::quick::input::shell_quote;
use crate::ui::{App, Level, TerminalRequest, UiMode};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Padding, Paragraph},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;

/// Which app list the dialog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppList {
    Folder,
    File,
    Diff,
}

pub struct OpenWithDialog {
    /// Working tree the app runs in.
    pub cwd: PathBuf,
    /// The folder, or the file's path relative to `cwd`.
    pub path: PathBuf,
    /// A file's tabs; empty for a folder.
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub cursor: usize,
}

impl OpenWithDialog {
    fn list(&self) -> AppList {
        match self.tabs.get(self.tab).map(|t| &t.target) {
            None => AppList::Folder,
            Some(OpenTarget::Diff(..)) => AppList::Diff,
            Some(OpenTarget::File(_)) => AppList::File,
        }
    }
}

/// Last app used per list and the last tab, so the next Open With starts on
/// them.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastApps {
    pub folder: Option<String>,
    pub file: Option<String>,
    pub diff: Option<String>,
    pub tab: Option<TabKind>,
}

impl LastApps {
    fn slot(&mut self, list: AppList) -> &mut Option<String> {
        match list {
            AppList::Folder => &mut self.folder,
            AppList::File => &mut self.file,
            AppList::Diff => &mut self.diff,
        }
    }
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
    fn launchers(&self, list: AppList) -> &[Launcher] {
        match list {
            AppList::Folder => &self.cfg.launchers,
            AppList::File => &self.cfg.file_launchers,
            AppList::Diff => &self.cfg.diff_launchers,
        }
    }

    /// Where `list` starts: the app used last from it, else the first.
    fn start_app(&mut self, list: AppList) -> usize {
        let last = self.last_apps.slot(list).clone();
        last.and_then(|name| self.launchers(list).iter().position(|l| l.name == name))
            .unwrap_or(0)
    }

    /// `enter` on a repository or worktree row.
    pub(crate) fn open_with_folder(&mut self) {
        let Some(target) = self.current_target() else {
            return;
        };
        let cursor = self.start_app(AppList::Folder);
        self.open_with = Some(OpenWithDialog {
            cwd: target.clone(),
            path: target,
            tabs: Vec::new(),
            tab: 0,
            cursor,
        });
        self.mode = UiMode::OpenWith;
    }

    /// Whether the file cursor is on a file Open With can open (a changed
    /// file, or a file of a commit's or the uncommitted changes' view).
    pub(crate) fn file_openable(&self) -> bool {
        match &self.commit_view {
            Some(view) => view.selected().is_some(),
            None => matches!(
                self.file_items().get(self.file_sel),
                Some(FileItem::File(_))
            ),
        }
    }

    /// The file under the file cursor and its tabs.
    fn file_tabs(&self, target: &Path) -> Option<(String, Vec<Tab>)> {
        let on_disk = |p: &str| target.join(p).exists();
        if let Some(view) = &self.commit_view {
            let f = view.selected()?;
            let tabs = match view.commit() {
                Some(c) => file_versions::commit_file_tabs(c, f, on_disk(&f.path)),
                None => file_versions::worktree_file_tabs(f, on_disk(&f.path)),
            };
            return Some((f.path.clone(), tabs));
        }
        match self.file_items().get(self.file_sel)? {
            FileItem::File(f) => Some((
                f.path.clone(),
                file_versions::changed_file_tabs(f, on_disk(&f.path)),
            )),
            _ => None,
        }
    }

    /// `enter` on a file. Returns false when the cursor is not on one (a
    /// commit, the summary row, `… more`), so `enter` keeps its other use.
    pub(crate) fn open_with_file(&mut self) -> bool {
        let Some(target) = self.current_target() else {
            return false;
        };
        let Some((path, tabs)) = self.file_tabs(&target) else {
            return false;
        };
        let tab = file_versions::start_tab(&tabs, self.last_apps.tab);
        let mut d = OpenWithDialog {
            cwd: target,
            path: PathBuf::from(path),
            tabs,
            tab,
            cursor: 0,
        };
        d.cursor = self.start_app(d.list());
        self.open_with = Some(d);
        self.mode = UiMode::OpenWith;
        true
    }

    pub(crate) fn on_key_open_with(&mut self, key: KeyEvent) {
        let Some(d) = self.open_with.as_ref() else {
            self.mode = UiMode::Normal;
            return;
        };
        let (tab, tabs) = (d.tab, d.tabs.len());
        let len = self.launchers(d.list()).len();
        if let Some(mv) = list_nav::list_move(&key, true) {
            if let Some(d) = self.open_with.as_mut() {
                d.cursor = list_nav::apply(d.cursor, len, mv, VIEW_MAX);
            }
            return;
        }
        // `←` / `→` stop at the ends; `tab` cycles.
        let next = match key.code {
            KeyCode::Left | KeyCode::Char('h') => Some(tab.saturating_sub(1)),
            KeyCode::Right | KeyCode::Char('l') => Some((tab + 1).min(tabs.saturating_sub(1))),
            KeyCode::Tab if tabs > 0 => Some((tab + 1) % tabs),
            KeyCode::BackTab if tabs > 0 => Some((tab + tabs - 1) % tabs),
            _ => None,
        };
        if let Some(next) = next {
            if next != tab {
                self.select_open_tab(next);
            }
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

    /// Moves to tab `i`; a different app list starts on its last app.
    fn select_open_tab(&mut self, i: usize) {
        let Some(list) = self.open_with.as_ref().map(|d| d.list()) else {
            return;
        };
        if let Some(d) = self.open_with.as_mut() {
            d.tab = i;
        }
        let Some(new_list) = self.open_with.as_ref().map(|d| d.list()) else {
            return;
        };
        if new_list != list {
            let cursor = self.start_app(new_list);
            if let Some(d) = self.open_with.as_mut() {
                d.cursor = cursor;
            }
        }
    }

    fn launch(&mut self, d: OpenWithDialog) {
        let list = d.list();
        let Some(app) = self.launchers(list).get(d.cursor).cloned() else {
            return;
        };
        let tab = d.tabs.get(d.tab).cloned();
        let mut changed = false;
        let slot = self.last_apps.slot(list);
        if slot.as_deref() != Some(app.name.as_str()) {
            *slot = Some(app.name.clone());
            changed = true;
        }
        if let Some(t) = &tab {
            if self.last_apps.tab != Some(t.kind) {
                self.last_apps.tab = Some(t.kind);
                changed = true;
            }
        }
        if changed {
            save_last_apps(&self.last_apps);
        }
        let target = match tab {
            None => {
                let command = app.command.replace("{path}", &quote(&d.path));
                self.run_app(app, d.cwd, command);
                return;
            }
            Some(t) => t.target,
        };
        let rel = d.path.to_string_lossy().into_owned();
        // The working tree's file needs nothing written.
        if target == OpenTarget::File(Version::WorkTree) {
            let command = app.command.replace("{path}", &quote(&d.cwd.join(&rel)));
            self.run_app(app, d.cwd, command);
            return;
        }
        let dir = self.versions_dir.clone();
        let cwd = d.cwd;
        self.bg.spawn(move || {
            let put = |v: &Version| materialize(&cwd, &rel, v, &dir);
            let command = match &target {
                OpenTarget::File(v) => put(v).map(|p| app.command.replace("{path}", &quote(&p))),
                OpenTarget::Diff(old, new) => put(old).and_then(|o| {
                    let n = put(new)?;
                    Ok(app
                        .command
                        .replace("{old}", &quote(&o))
                        .replace("{new}", &quote(&n)))
                }),
            };
            Box::new(move |a: &mut App| match command {
                Ok(command) => a.run_app(app, cwd, command),
                Err(err) => a.notify(Level::Error, format!("{}: {err}", app.name)),
            })
        });
    }

    /// Runs `command` in `cwd`: a terminal app takes the terminal over, a
    /// GUI app starts detached.
    fn run_app(&mut self, app: Launcher, cwd: PathBuf, command: String) {
        if !app.gui {
            self.terminal_request = Some(TerminalRequest {
                cwd,
                command,
                pause: false,
                name: Some(app.name),
            });
            return;
        }
        self.notify(Level::Info, format!("Opening {}…", app.name));
        let name = app.name;
        self.bg.spawn(move || {
            let result = crate::launch::spawn_detached(&cwd, &command);
            Box::new(move |app: &mut App| match result {
                Ok(()) => app.notify(Level::Ok, format!("Opened {name}")),
                Err(err) => app.notify(Level::Error, format!("{name}: {err}")),
            })
        });
    }
}

fn quote(path: &Path) -> String {
    shell_quote(&path.to_string_lossy())
}

/// The file an app opens for `v`: the working tree's, or a read-only copy
/// under `dir` (an empty one for a missing side).
fn materialize(cwd: &Path, rel: &str, v: &Version, dir: &Path) -> anyhow::Result<PathBuf> {
    Ok(match v {
        Version::WorkTree => cwd.join(rel),
        Version::Empty => crate::scratch::write_version(dir, "empty", rel, b"")?,
        Version::Rev { rev, path, label } => {
            let data = crate::git::blob::read(cwd, rev, path)?;
            crate::scratch::write_version(dir, label, path, &data)?
        }
    })
}

const VIEW_MAX: usize = 14;

/// A file's tabs (the selected one bracketed), a blank row, then
/// the app list: name left, `terminal` / `gui` right and faint. No buttons;
/// `enter` runs the selected app.
pub(crate) fn draw_open_with(f: &mut Frame, app: &App) {
    let Some(d) = app.open_with.as_ref() else {
        return;
    };
    let th = &app.theme;
    let apps = app.launchers(d.list());
    let total = apps.len();
    let view = total.clamp(1, VIEW_MAX);
    let tabs_h: u16 = if d.tabs.is_empty() { 0 } else { 2 };
    // Borders (2) + top and bottom padding (1 each) + tabs + rows.
    let h = (view as u16)
        .saturating_add(4 + tabs_h)
        .min(f.area().height);
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
    let accent = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);
    if tabs_h > 0 {
        let mut spans = Vec::new();
        for (i, t) in d.tabs.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            // The selected tab is bracketed in the accent color, so it does
            // not read as a second selection bar next to the app's.
            let text = if i == d.tab {
                Span::styled(format!("[{}]", t.label), accent)
            } else {
                Span::styled(format!(" {} ", t.label), th.soft_dim())
            };
            spans.push(text);
        }
        let rect = Rect::new(inner.x, inner.y, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
    let list_area = Rect::new(
        inner.x,
        inner.y + tabs_h,
        inner.width,
        (view as u16).min(inner.height.saturating_sub(tabs_h)),
    );
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
#[path = "open_with_tests.rs"]
mod tests;
