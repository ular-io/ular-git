//! File pane: changed files (status code + path), then the commit history
//! with its branch graph (one row per commit) under a `History` divider.

use super::history_row::{detail_line, graph_spans, ref_spans};
use super::repos::CHANGED;
use super::{
    draw_divider, empty_box, empty_line, pane_block, pane_footer, pane_tone, selection_style,
    Divider, Tone,
};
use crate::git::graph::Cell;
use crate::git::history::{GraphRow, RefLabel};
use crate::git::EntryKind;
use crate::git::FileEntry;
use crate::theme::Theme;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{pad_w, truncate_left_w, truncate_w};
use crate::ui::files::{FileItem, Mark};
use crate::ui::history_view::CommitMeta;
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{List, ListItem},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub(super) fn draw_files(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let tone = pane_tone(app, Pane::Files);
    let tree = app.current_tree();
    let status = tree.and_then(|t| t.status.as_ref());
    let mut block = pane_block(app, pane_title(app), tone, Pane::Files);
    if let Some(label) = commits_label(app, area.width) {
        block = pane_footer(block, label, tone, th);
    }
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
    let mut author_w = 0;
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
            author_w = (h.rows.iter().map(|r| r.author.width()).max())
                .unwrap_or(0)
                .min(AUTHOR_MAX);
            for row in &h.rows {
                visual.push(Visual::Graph(next, row));
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
                        FileItem::File(file) => file_spans(file, w, th),
                        FileItem::Commit(c, mark) => {
                            let mut spans = graph_spans(graph_of(v), graph_w, mark, th);
                            spans.extend(meta_spans(v, app.history.meta, author_w, th));
                            let used: usize = spans.iter().map(|s| s.content.width()).sum();
                            // Labels run through the right padding cell to the
                            // border; the subject keeps at least SUBJECT_MIN
                            // cells before them.
                            let row_w = w + 1;
                            let labels = ref_spans(
                                refs_of(v),
                                app.history.labels,
                                row_w.saturating_sub(used + SUBJECT_MIN),
                                th,
                            );
                            let labels_w: usize = labels.iter().map(|s| s.content.width()).sum();
                            let style = Style::default();
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
                            let mut spans = graph_spans(cells, graph_w, Mark::Shared, th);
                            let used: usize = spans.iter().map(|s| s.content.width()).sum();
                            let badges = worktree_badges(status, th);
                            let badges_w: usize = badges.iter().map(|s| s.content.width()).sum();
                            let title =
                                truncate_w(WORKTREE_TITLE, w.saturating_sub(used + badges_w + 1));
                            let gap = w.saturating_sub(used + title.width() + badges_w);
                            spans.push(Span::styled(title, Style::default()));
                            spans.push(Span::raw(" ".repeat(gap)));
                            spans.extend(badges);
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
            let mut detail = None;
            if item_index == Some(app.file_sel) {
                selected_row = row;
                let sel = selection_style(tone, th);
                // Ref labels (the only spans with a background) keep their
                // colors on the selection bar.
                for s in spans.iter_mut().filter(|s| s.style.bg.is_none()) {
                    s.style = sel;
                }
                // Subject-only rows show the selected commit's date and
                // author on a second line.
                if let Visual::Graph(_, g) = v {
                    if app.history.meta.details_on_selection() {
                        detail = Some(detail_line(g, graph_w, w + 1, sel, th));
                    }
                }
            }
            let mut text = Text::from(Line::from(spans));
            text.lines.extend(detail);
            ListItem::new(text)
        })
        .collect();
    // The selected commit's second line is the only one past one per row.
    let total: usize = lines.iter().map(ListItem::height).sum();
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

/// The bottom border's `1 / 123 commits`: the selected commit's place in the
/// history and the history's commit count, or the count alone off a commit;
/// `1 / 123` when narrow, nothing when even that leaves no border to its left.
pub(super) fn commits_label(app: &App, width: u16) -> Option<String> {
    let total = app.history.commits?;
    let items = app.file_items();
    let is_commit = |i: &FileItem| matches!(i, FileItem::Commit(..));
    let unit = if total == 1 { "commit" } else { "commits" };
    let labels = match items.get(app.file_sel) {
        Some(item) if is_commit(item) => {
            let pos = items[..=app.file_sel]
                .iter()
                .filter(|i| is_commit(i))
                .count();
            vec![
                format!(" {pos} / {total} {unit} "),
                format!(" {pos} / {total} "),
            ]
        }
        _ => vec![format!(" {total} {unit} ")],
    };
    // Two corners, the border cell after the label, four before it.
    let room = (width as usize).saturating_sub(2 + 1 + 4);
    labels.into_iter().find(|l| l.width() <= room)
}

/// Graph columns shown at most; wider graphs are cut on the right.
const GRAPH_MAX: usize = 12;

/// One row of the file pane.
enum Visual<'a> {
    /// A selectable file or `… more` row (index into the items).
    Item(usize),
    /// A history commit (item index) and its row: graph cells, refs, date,
    /// and author.
    Graph(usize, &'a GraphRow),
    /// The working-tree row (item index) and its graph cells.
    WorkTree(usize, &'a [Cell]),
    Note(String),
    Divider,
}

fn graph_of<'a>(v: &Visual<'a>) -> &'a [Cell] {
    match v {
        Visual::Graph(_, row) => &row.cells,
        Visual::WorkTree(_, g) => g,
        _ => &[],
    }
}

/// The file pane's title in every view (changes and history, a commit's
/// files, the uncommitted changes' files): the selected repository's (or
/// worktree's) folder name; `Files` when a group header is selected.
pub(super) fn pane_title(app: &App) -> String {
    match app.current_target() {
        Some(target) => format!(
            " {} ",
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        None => " Files ".to_string(),
    }
}

/// The working-tree row's label; the diff pane title uses the same words.
pub(crate) const WORKTREE_TITLE: &str = "Uncommitted Changes";

/// Right-aligned badges of the working-tree row, as in the repository pane:
/// `!1` conflicts, `Ⓜ 3` other changes (untracked included), `↓2` commits to
/// pull (the row exists only while the tree has changes).
fn worktree_badges(status: &crate::git::RepoStatus, th: &Theme) -> Vec<Span<'static>> {
    let c = status.counts();
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut push = |text: String, style: Style| {
        if !out.is_empty() {
            out.push(Span::raw(" "));
        }
        out.push(Span::styled(text, style));
    };
    if c.conflicts > 0 {
        push(format!("!{}", c.conflicts), Style::default().fg(th.error));
    }
    let other = c.changed - c.conflicts;
    if other > 0 {
        push(format!("{CHANGED}{other}"), Style::default().fg(th.warning));
    }
    if status.branch.behind > 0 {
        push(
            format!("↓{}", status.branch.behind),
            Style::default().fg(th.branch),
        );
    }
    out
}

fn refs_of<'a>(v: &Visual<'a>) -> &'a [RefLabel] {
    match v {
        Visual::Graph(_, row) => &row.refs,
        _ => &[],
    }
}

/// The date and author a history row shows before its subject (as
/// `app.history.meta` chooses, whatever the width), each followed by a space;
/// authors pad to `author_w` so the subjects line up.
fn meta_spans(v: &Visual, meta: CommitMeta, author_w: usize, th: &Theme) -> Vec<Span<'static>> {
    let Visual::Graph(_, row) = v else {
        return Vec::new();
    };
    let style = Style::default().fg(th.muted);
    let mut out = Vec::new();
    if meta.date() {
        out.push(Span::styled(format!("{} ", row.date), style));
    }
    if meta.author() && author_w > 0 {
        let name = pad_w(&truncate_w(&row.author, author_w), author_w);
        out.push(Span::styled(format!("{name} "), style));
    }
    out
}

/// Widest author name on history rows; longer names are cut with `…`.
const AUTHOR_MAX: usize = 16;

/// Subject cells kept in front of the labels.
const SUBJECT_MIN: usize = 12;

fn file_spans(file: &FileEntry, w: usize, th: &Theme) -> Vec<Span<'static>> {
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
        Span::styled(code[0].to_string(), xs),
        Span::styled(code[1].to_string(), ys),
        Span::raw(" "),
        Span::styled(dir, th.soft_dim()),
        Span::styled(name, Style::default()),
    ]
}
