//! File pane: changed files (status code + path), then unpushed commits.

use super::{body_style, empty_line, pane_block, pane_tone, selection_style, Tone};
use crate::git::EntryKind;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_left_w, truncate_w};
use crate::ui::files::FileItem;
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub(super) fn draw_files(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let tone = pane_tone(app, Pane::Files);
    let tree = app.current_tree();
    let status = tree.and_then(|t| t.status.as_ref());
    let title = match status {
        Some(s) if !s.unpushed.is_empty() => format!(
            " Files[{}] Commits[{}] ",
            s.counts().changed,
            s.unpushed.len()
        ),
        Some(s) => format!(" Files[{}] ", s.counts().changed),
        None => " Files ".to_string(),
    };
    let block = pane_block(title, tone, Pane::Files, th);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = inner.height as usize;
    app.file_page.set(view);
    let dim_style = Style::default().fg(th.dim);

    if app.current_target().is_none() {
        let text = if app.rows.is_empty() {
            ""
        } else {
            "Select a repository"
        };
        empty_line(f, inner, text, dim_style);
        return;
    }
    let Some(status) = status else {
        let (text, style) = match tree.and_then(|t| t.error.as_ref()) {
            Some(err) => (err.clone(), Style::default().fg(th.error)),
            None => ("Scanning…".to_string(), dim_style),
        };
        empty_line(f, inner, &truncate_w(&text, inner.width as usize), style);
        return;
    };
    let items = app.file_items();
    if items.is_empty() {
        let mut text = "Working tree clean".to_string();
        if status.branch.behind > 0 {
            text.push_str(&format!(" · {} behind upstream", status.branch.behind));
        }
        empty_line(f, inner, &text, dim_style);
        return;
    }

    let list_area = Rect {
        x: inner.x.saturating_sub(1),
        width: inner.width + 2,
        ..inner
    };
    let w = list_area.width.saturating_sub(1) as usize;
    let dim = |s: Style| body_style(tone, s, th);
    let lines: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut spans: Vec<Span> = match item {
                FileItem::File(file) => {
                    let code: Vec<char> = file.code().chars().collect();
                    let (xs, ys) = match file.kind {
                        EntryKind::Untracked => {
                            let s = Style::default().fg(th.untracked);
                            (s, s)
                        }
                        EntryKind::Conflict => {
                            let s = Style::default().fg(th.error).add_modifier(Modifier::BOLD);
                            (s, s)
                        }
                        EntryKind::Tracked => (
                            Style::default().fg(th.success).add_modifier(Modifier::BOLD),
                            Style::default().fg(th.warning).add_modifier(Modifier::BOLD),
                        ),
                    };
                    let path = match &file.orig_path {
                        Some(orig) => format!("{orig} → {}", file.path),
                        None => file.path.clone(),
                    };
                    let room = w.saturating_sub(4);
                    let path = truncate_left_w(&path, room);
                    let (dir, name) = match path.rfind('/') {
                        Some(i) => (path[..=i].to_string(), path[i + 1..].to_string()),
                        None => (String::new(), path),
                    };
                    vec![
                        Span::raw(" "),
                        Span::styled(code[0].to_string(), dim(xs)),
                        Span::styled(code[1].to_string(), dim(ys)),
                        Span::raw(" "),
                        Span::styled(dir, th.soft_dim()),
                        Span::styled(name, dim(Style::default())),
                    ]
                }
                // `↑` sits where a file's status code does, then the
                // message's first line.
                FileItem::Commit(c) => {
                    let head = " ↑  ";
                    let room = w.saturating_sub(head.width());
                    vec![
                        Span::styled(head, dim(Style::default().fg(th.branch))),
                        Span::styled(truncate_w(&c.subject, room), dim(Style::default())),
                    ]
                }
            };
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            // Pad through the right padding cell so the selection bar reaches
            // the border.
            spans.push(Span::raw(" ".repeat((w + 1).saturating_sub(used))));
            if i == app.file_sel {
                let sel = selection_style(tone, th);
                for s in &mut spans {
                    s.style = sel;
                }
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let total = lines.len();
    let mut state = app.file_list.borrow_mut();
    state.select(Some(app.file_sel));
    f.render_stateful_widget(List::new(lines), list_area, &mut state);
    let offset = state.offset();
    drop(state);
    draw_vscrollbar(f, area, tone == Tone::Focused, offset, total, view, th);
}
