//! File pane: changed files (status code + path), then the commit history
//! with its branch graph (one row per commit), faded, under a `History`
//! divider.

use super::{
    body_style, draw_divider, empty_line, pane_block, pane_tone, selection_style, Divider, Tone,
};
use crate::git::graph::Cell;
use crate::git::EntryKind;
use crate::git::FileEntry;
use crate::theme::{color_rgb, Theme};
use crate::ui::components::modal::fade_toward;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_left_w, truncate_w};
use crate::ui::files::{FileItem, Mark};
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
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
    // The selected repository's (or worktree's) folder name; `Files` when a
    // group header is selected.
    let title = match app.current_target() {
        Some(target) => format!(
            " {} ",
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        None => " Files ".to_string(),
    };
    let block = pane_block(app, title, tone, Pane::Files);
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
    let history = app.current_history();

    // Visual rows; `Item` rows are selectable (index into `items`).
    let mut visual: Vec<Visual> = Vec::new();
    let mut next = 0;
    for _ in &status.files {
        visual.push(Visual::Item(next));
        next += 1;
    }
    if status.files.is_empty() {
        let mut text = "Working tree clean".to_string();
        if status.branch.behind > 0 {
            text.push_str(&format!(" · {} behind upstream", status.branch.behind));
        }
        visual.push(Visual::Note(text));
    }
    let mut graph_w = 0;
    match (history, &app.history.error) {
        (Some(h), _) if !h.rows.is_empty() => {
            visual.push(Visual::Divider);
            graph_w = h
                .rows
                .iter()
                .map(|r| r.cells.len())
                .max()
                .unwrap_or(0)
                .min(GRAPH_MAX);
            for row in &h.rows {
                visual.push(Visual::Graph(next, &row.cells));
                next += 1;
            }
            if h.more {
                visual.push(Visual::Item(next));
            }
        }
        (Some(_), _) => {}
        (None, Some(err)) => {
            visual.push(Visual::Divider);
            visual.push(Visual::Note(err.clone()));
        }
        (None, None) => {
            visual.push(Visual::Divider);
            visual.push(Visual::Note("Loading history…".into()));
        }
    }

    let list_area = Rect {
        x: inner.x.saturating_sub(1),
        width: inner.width + 2,
        ..inner
    };
    let w = list_area.width.saturating_sub(1) as usize;
    let dim = |s: Style| body_style(tone, s, th);
    let mut selected_row = 0;
    let lines: Vec<ListItem> = visual
        .iter()
        .enumerate()
        .map(|(row, v)| {
            let (item_index, mut spans): (Option<usize>, Vec<Span>) = match v {
                Visual::Divider => return ListItem::new(Line::default()),
                Visual::Note(text) => {
                    return ListItem::new(Line::from(Span::styled(
                        format!(" {}", truncate_w(text, w.saturating_sub(1))),
                        dim_style,
                    )))
                }
                Visual::Graph(i, _) | Visual::Item(i) if items.get(*i).is_some() => {
                    let item = items[*i];
                    let spans = match item {
                        FileItem::File(file) => file_spans(file, w, tone, th),
                        FileItem::Commit(c, mark) => {
                            let mut spans = graph_spans(graph_of(v), graph_w, mark, tone, th);
                            let used: usize = spans.iter().map(|s| s.content.width()).sum();
                            // Commits to push or pull stand out of the faded history.
                            let style = if mark == Mark::Shared {
                                th.soft_dim()
                            } else {
                                dim(Style::default())
                            };
                            spans.push(Span::styled(
                                truncate_w(&c.subject, w.saturating_sub(used + 1)),
                                style,
                            ));
                            spans
                        }
                        FileItem::More => vec![Span::styled(" … more", th.soft_dim())],
                    };
                    (Some(*i), spans)
                }
                _ => (None, Vec::new()),
            };
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            // Pad through the right padding cell so the selection bar reaches
            // the border.
            spans.push(Span::raw(" ".repeat((w + 1).saturating_sub(used))));
            if item_index == Some(app.file_sel) {
                selected_row = row;
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
    state.select(Some(selected_row));
    f.render_stateful_widget(List::new(lines), list_area, &mut state);
    let offset = state.offset();
    drop(state);
    // The History divider joins the pane border like the diff titles.
    if let Some(at) = visual.iter().position(|v| matches!(v, Visual::Divider)) {
        if at >= offset && at < offset + view {
            let d = Divider {
                y: inner.y + (at - offset) as u16,
                label: "History".into(),
                style: th.soft_dim(),
            };
            draw_divider(f, area, &d, tone, th);
        }
    }
    draw_vscrollbar(f, area, tone == Tone::Focused, offset, total, view, th);
}

/// Graph columns shown at most; wider graphs are cut on the right.
const GRAPH_MAX: usize = 12;

/// One row of the file pane.
enum Visual<'a> {
    /// A selectable file or `… more` row (index into the items).
    Item(usize),
    /// A history commit (item index) and its graph cells.
    Graph(usize, &'a [Cell]),
    Note(String),
    Divider,
}

fn graph_of<'a>(v: &Visual<'a>) -> &'a [Cell] {
    match v {
        Visual::Graph(_, g) => g,
        _ => &[],
    }
}

/// Graph cells colored by lane (a lane keeps its color) and faded so history
/// stays in the background. A commit to push is `↑` and one to pull `↓`, in
/// the branch color.
fn graph_spans(
    graph: &[Cell],
    width: usize,
    mark: Mark,
    tone: Tone,
    th: &Theme,
) -> Vec<Span<'static>> {
    let lanes = [
        th.branch,
        th.diff_add,
        th.warning,
        th.diff_hunk,
        th.untracked,
        th.accent,
    ];
    let mut spans = vec![Span::raw(" ")];
    let mut used = 0;
    for cell in graph.iter().take(width) {
        let node = cell.ch == '●';
        let glyph = match (node, mark) {
            (true, Mark::Unpushed) => '↑',
            (true, Mark::Incoming) => '↓',
            _ => cell.ch,
        };
        let marked = node && mark != Mark::Shared;
        let color = lanes[cell.color.unwrap_or(0) % lanes.len()];
        // Each step toward the background is one fade: a focused pane shows
        // marked nodes at full color and lanes faded once; another pane's
        // focus fades both one step further, so the graph never gets
        // stronger when the pane loses focus.
        let fade = |c: Color| fade_toward(color_rgb(c), th.bg_rgb());
        let style = match (tone == Tone::Dimmed, marked) {
            (false, true) => Style::default().fg(th.branch).add_modifier(Modifier::BOLD),
            (false, false) => Style::default().fg(fade(color)),
            (true, true) => Style::default().fg(fade(th.branch)),
            (true, false) => Style::default().fg(fade(fade(color))),
        };
        spans.push(Span::styled(glyph.to_string(), style));
        used += 1;
    }
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used) + 1)));
    spans
}

fn file_spans(file: &FileEntry, w: usize, tone: Tone, th: &Theme) -> Vec<Span<'static>> {
    let dim = |s: Style| body_style(tone, s, th);
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
    let path = truncate_left_w(&path, w.saturating_sub(4));
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
