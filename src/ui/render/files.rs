//! File pane: changed files (status code + path), then the commit history
//! with its branch graph (one row per commit), faded, under a `History`
//! divider.

use super::{
    body_style, draw_divider, empty_box, empty_line, pane_block, pane_tone, selection_style,
    Divider, Tone,
};
use crate::git::graph::Cell;
use crate::git::history::{RefKind, RefLabel};
use crate::git::EntryKind;
use crate::git::FileEntry;
use crate::theme::{color_rgb, Theme};
use crate::ui::components::modal::fade_toward;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_left_w, truncate_w};
use crate::ui::files::{FileItem, Mark};
use crate::ui::history_view::RefLabels;
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
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
        // A group header: the message sits on the group row color.
        if !app.rows.is_empty() {
            empty_box(f, inner, "Select a repository", dim_style, th.group_bg);
        }
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
    // A tree with changes: the History divider, then the working-tree row.
    // A clean tree starts with the history.
    if !status.is_clean() {
        visual.push(Visual::Divider);
        let no_cells: &[Cell] = &[];
        visual.push(Visual::WorkTree(
            next,
            history.map_or(no_cells, |h| &h.worktree),
        ));
        next += 1;
    }
    let mut graph_w = 1;
    match (history, &app.history.error) {
        (Some(h), _) if !h.rows.is_empty() => {
            graph_w = h
                .rows
                .iter()
                .map(|r| r.cells.len())
                .chain([h.worktree.len()])
                .max()
                .unwrap_or(1)
                .min(GRAPH_MAX);
            for row in &h.rows {
                visual.push(Visual::Graph(next, &row.cells, &row.refs));
                next += 1;
            }
            if h.more {
                visual.push(Visual::Item(next));
            }
        }
        (Some(_), _) => {}
        (None, Some(err)) => visual.push(Visual::Note(err.clone())),
        (None, None) => visual.push(Visual::Note("Loading history…".into())),
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
                Visual::Graph(i, ..) | Visual::WorkTree(i, _) | Visual::Item(i)
                    if items.get(*i).is_some() =>
                {
                    let item = items[*i];
                    let spans = match item {
                        FileItem::File(file) => file_spans(file, w, tone, th),
                        FileItem::Commit(c, mark) => {
                            let mut spans = graph_spans(graph_of(v), graph_w, mark, tone, th);
                            let used: usize = spans.iter().map(|s| s.content.width()).sum();
                            // Labels run through the right padding cell to the
                            // border; the subject keeps at least SUBJECT_MIN
                            // cells before them.
                            let row_w = w + 1;
                            let labels = ref_spans(
                                refs_of(v),
                                app.history.labels,
                                row_w.saturating_sub(used + SUBJECT_MIN),
                                tone,
                                th,
                            );
                            let labels_w: usize = labels.iter().map(|s| s.content.width()).sum();
                            // Commits to push or pull stand out of the faded history.
                            let style = if mark == Mark::Shared {
                                th.soft_dim()
                            } else {
                                dim(Style::default())
                            };
                            let room = if labels.is_empty() { w } else { row_w };
                            let subject = truncate_w(
                                &c.subject,
                                room.saturating_sub(used + labels_w).saturating_sub(1),
                            );
                            let gap = room.saturating_sub(used + subject.width() + labels_w);
                            spans.push(Span::styled(subject, style));
                            if !labels.is_empty() {
                                spans.push(Span::raw(" ".repeat(gap)));
                                spans.extend(labels);
                            }
                            spans
                        }
                        FileItem::WorkTree => {
                            let cells = graph_of(v);
                            let node = [Cell {
                                ch: '○',
                                color: Some(0),
                            }];
                            let cells = if cells.is_empty() { &node[..] } else { cells };
                            let mut spans = graph_spans(cells, graph_w, Mark::Shared, tone, th);
                            let used: usize = spans.iter().map(|s| s.content.width()).sum();
                            spans.push(Span::styled(
                                truncate_w(&worktree_summary(status), w.saturating_sub(used)),
                                dim(Style::default()),
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
                // Ref labels (the only spans with a background) keep their
                // colors on the selection bar.
                for s in spans.iter_mut().filter(|s| s.style.bg.is_none()) {
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
    /// A history commit (item index), its graph cells, and its refs.
    Graph(usize, &'a [Cell], &'a [RefLabel]),
    /// The working-tree row (item index) and its graph cells.
    WorkTree(usize, &'a [Cell]),
    Note(String),
    Divider,
}

fn graph_of<'a>(v: &Visual<'a>) -> &'a [Cell] {
    match v {
        Visual::Graph(_, g, _) | Visual::WorkTree(_, g) => g,
        _ => &[],
    }
}

/// `3 changes · 2 modified · 1 untracked`, then `· N behind` with commits
/// to pull (the row exists only while the tree has changes).
fn worktree_summary(status: &crate::git::RepoStatus) -> String {
    let n = status.counts().changed;
    let mut parts = vec![format!("{n} change{}", if n == 1 { "" } else { "s" })];
    let kind = |k: EntryKind| status.files.iter().filter(|f| f.kind == k).count();
    for (count, what) in [
        (kind(EntryKind::Tracked), "modified"),
        (kind(EntryKind::Untracked), "untracked"),
        (kind(EntryKind::Conflict), "conflicted"),
    ] {
        if count > 0 {
            parts.push(format!("{count} {what}"));
        }
    }
    if status.branch.behind > 0 {
        parts.push(format!("{} behind", status.branch.behind));
    }
    parts.join(" · ")
}

fn refs_of<'a>(v: &Visual<'a>) -> &'a [RefLabel] {
    match v {
        Visual::Graph(_, _, r) => r,
        _ => &[],
    }
}

/// Longest ref name shown in a label; longer names are cut with `…`.
const REF_NAME_MAX: usize = 24;

/// Subject cells kept in front of the labels.
const SUBJECT_MIN: usize = 12;

/// Branch and tag labels drawn as solid badges (the theme background as text
/// on the label color, no icon), joined into one strip; a `│` in the
/// background color splits two badges of the same color (different colors
/// need no line), within `room` cells; a label that does not
/// fit is left out with the rest. The kind is told by
/// color: local branches the branch color, remote branches
/// the hunk color, tags the warning color, each moved a quarter toward the
/// text color so the badge stands out of the background (a dimmed pane fades
/// it back a third, not the usual half).
fn ref_spans(
    refs: &[RefLabel],
    labels: RefLabels,
    room: usize,
    tone: Tone,
    th: &Theme,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for r in refs {
        let color = match r.kind {
            RefKind::Branch if labels.branches() => th.branch,
            RefKind::Remote if labels.branches() => th.diff_hunk,
            RefKind::Tag if labels.tags() => th.warning,
            _ => continue,
        };
        let text = format!(" {} ", truncate_w(&r.name, REF_NAME_MAX));
        let ink = match th.fg {
            Color::Reset if th.dark => (255, 255, 255),
            Color::Reset => (0, 0, 0),
            c => color_rgb(c),
        };
        let mut badge = mix(color_rgb(color), ink, 25);
        if tone == Tone::Dimmed {
            badge = mix(badge, th.bg_rgb(), 35);
        }
        let rgb = |(r, g, b): (u8, u8, u8)| Color::Rgb(r, g, b);
        let style = Style::default().bg(rgb(badge)).fg(rgb(th.bg_rgb()));
        let sep = usize::from(spans.last().is_some_and(|s| s.style.bg == style.bg));
        let w = text.width() + sep;
        if used + w > room {
            break;
        }
        used += w;
        if sep == 1 {
            // The line sits on the left badge's color.
            let left = spans.last().and_then(|s| s.style.bg);
            let mut line = Style::default().fg(rgb(th.bg_rgb()));
            line.bg = left;
            spans.push(Span::styled("│", line));
        }
        spans.push(Span::styled(text, style));
    }
    spans
}

/// `a` moved `pct` percent toward `b`.
fn mix((r, g, b): (u8, u8, u8), (tr, tg, tb): (u8, u8, u8), pct: u32) -> (u8, u8, u8) {
    let m = |c: u8, t: u8| ((u32::from(c) * (100 - pct) + u32::from(t) * pct) / 100) as u8;
    (m(r, tr), m(g, tg), m(b, tb))
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
            (false, true) => Style::default().fg(th.branch),
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
            let s = Style::default().fg(th.error);
            (s, s)
        }
        EntryKind::Tracked => (
            Style::default().fg(th.success),
            Style::default().fg(th.warning),
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
