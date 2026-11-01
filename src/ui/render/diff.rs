//! Diff pane: colored, line-numbered diff of the selected file or commit.

use super::files::WORKTREE_TITLE;
use super::gutter::{is_code, Gutter};
use super::history_row::mix;
use super::{draw_divider, empty_line, pane_block, pane_tone, split, Divider, Tone};
use crate::git::diff::{DiffLine, LineKind};
use crate::theme::{color_rgb, Theme};
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_w, wrap_path, wrap_w};
use crate::ui::diff_view::Anchor;
use crate::ui::files::FileItem;
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

/// How far unchanged (context) lines move from `muted` toward the background.
const CONTEXT_FADE_PCT: u32 = 25;

/// A line's style. The same whatever the focus: a pane's content is never
/// dimmed, so a diff read beside another focused pane stays legible.
fn kind_style(kind: LineKind, th: &Theme) -> Style {
    let fg = |c: Color| Style::default().fg(c);
    match kind {
        LineKind::Section | LineKind::File | LineKind::Rule => {
            fg(th.accent).add_modifier(Modifier::BOLD)
        }
        LineKind::Byline | LineKind::Path | LineKind::Meta => th.soft_dim(),
        // Counts are drawn per sign by `counts_spans`.
        LineKind::Counts | LineKind::Header | LineKind::Stat => Style::default(),
        LineKind::Subject => Style::default().add_modifier(Modifier::BOLD),
        // Unchanged lines recede so the changes stand out: `muted` a quarter
        // toward the background.
        LineKind::Context => {
            let (r, g, b) = mix(color_rgb(th.muted), th.bg_rgb(), CONTEXT_FADE_PCT);
            fg(Color::Rgb(r, g, b))
        }
        LineKind::Gap => fg(th.dim),
        LineKind::Add => fg(th.diff_add),
        LineKind::Del => fg(th.diff_del),
    }
}

/// A shown commit's message lines, which always wrap.
fn is_message(kind: LineKind) -> bool {
    matches!(
        kind,
        LineKind::Byline | LineKind::Subject | LineKind::Header
    )
}

/// `+20 -3` with each side in its diff color.
fn counts_spans(text: &str, th: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, part) in text.split(' ').enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let kind = if part.starts_with('-') {
            LineKind::Del
        } else {
            LineKind::Add
        };
        spans.push(Span::styled(part.to_string(), kind_style(kind, th)));
    }
    spans
}

/// One visual row: a segment of a line across the pane, or the two sides of
/// the side-by-side layout.
enum Row {
    Line(Seg),
    Pair(Option<Seg>, Option<Seg>),
}

impl Row {
    /// The line a visual row shows (the earlier side of a pair).
    fn line(&self) -> Option<usize> {
        match self {
            Row::Line(s) => Some(s.line),
            Row::Pair(l, r) => match (l, r) {
                (Some(l), Some(r)) => Some(l.line.min(r.line)),
                (l, r) => l.as_ref().or(r.as_ref()).map(|s| s.line),
            },
        }
    }
}

/// The top row of this frame. When the layout changed since the last frame
/// (`.`, `s`, or a new pane width) and nothing scrolled since, the line that
/// was on top stays on top (`row_of` finds its first row); otherwise the
/// scroll position stands. Records this frame's top line (`line_at` the
/// row) for the next frame.
fn top_row(
    app: &App,
    layout: (bool, bool, usize),
    max: usize,
    row_of: impl Fn(usize) -> usize,
    line_at: impl Fn(usize) -> usize,
) -> usize {
    let d = &app.diff;
    if let Some(a) = d.anchor.get() {
        if a.layout != layout && a.row == d.scroll.get() {
            d.scroll.set(row_of(a.line).min(max));
        }
    }
    let row = d.scroll.get();
    let top = row.min(max);
    d.anchor.set(Some(Anchor {
        row,
        line: line_at(top),
        layout,
    }));
    top
}

/// A wrapped or cut piece of one line.
struct Seg {
    /// Index into the lines.
    line: usize,
    text: String,
    /// The line's first piece (it carries the line numbers).
    first: bool,
    /// Byte offset of `text` in the line, to place the line's marks.
    start: usize,
}

/// Pieces of line `line` with their offsets (wrapping keeps every byte, so
/// each piece starts where the previous one ended); at least one piece.
fn segs_of(line: usize, pieces: Vec<String>) -> Vec<Seg> {
    if pieces.is_empty() {
        return vec![Seg {
            line,
            text: String::new(),
            first: true,
            start: 0,
        }];
    }
    let mut start = 0;
    pieces
        .into_iter()
        .enumerate()
        .map(|(k, text)| {
            let seg = Seg {
                line,
                first: k == 0,
                start,
                text,
            };
            start += seg.text.len();
            seg
        })
        .collect()
}

/// `text` in `style`, the bytes inside `marks` (offsets in the whole line,
/// `text` starting at `start`) reversed.
fn marked_spans(
    text: String,
    start: usize,
    marks: &[(usize, usize)],
    style: Style,
) -> Vec<Span<'static>> {
    if marks.is_empty() {
        return vec![Span::styled(text, style)];
    }
    let hi = style.add_modifier(Modifier::REVERSED);
    let mut out = Vec::new();
    let mut run = String::new();
    let mut run_on = false;
    for (off, c) in text.char_indices() {
        let at = start + off;
        let on = marks.iter().any(|&(s, e)| at >= s && at < e);
        if on != run_on && !run.is_empty() {
            out.push(Span::styled(
                std::mem::take(&mut run),
                if run_on { hi } else { style },
            ));
        }
        run_on = on;
        run.push(c);
    }
    if !run.is_empty() {
        out.push(Span::styled(run, if run_on { hi } else { style }));
    }
    out
}

/// One side of a side-by-side row: its line number (old on the left, new
/// on the right; only on a line's first segment) and the segment, padded to
/// `width`.
fn side_spans(
    doc: &crate::git::diff::DiffDoc,
    side: Option<Seg>,
    width: usize,
    old: bool,
    gutter: &Gutter,
    th: &Theme,
) -> Vec<Span<'static>> {
    let Some(seg) = side else {
        return vec![Span::raw(" ".repeat(width))];
    };
    let line = &doc.lines[seg.line];
    let n = if old { line.old } else { line.new };
    let num = gutter.split_text(old, n, !seg.first);
    let pad = width.saturating_sub(gutter.split_side(old) + seg.text.width());
    let mut spans = vec![Span::styled(num, Style::default().fg(th.dim))];
    spans.extend(marked_spans(
        seg.text,
        seg.start,
        &line.marks,
        kind_style(line.kind, th),
    ));
    spans.push(Span::raw(" ".repeat(pad)));
    spans
}

fn title(app: &App) -> String {
    if let Some(view) = &app.commit_view {
        return super::commit_files::diff_title(app, view);
    }
    let items = app.file_items();
    let Some(item) = items.get(app.file_sel) else {
        return " Diff ".into();
    };
    match item {
        // The full path and the counts are the first lines of the diff.
        FileItem::File(f) => {
            let name = f.path.rsplit('/').next().unwrap_or(&f.path);
            format!(" {} ", truncate_w(name, 48))
        }
        FileItem::Commit(c, _) => {
            let stats = app
                .diff
                .doc
                .as_ref()
                .map(|d| format!(" +{} -{}", d.added, d.removed))
                .unwrap_or_default();
            format!(" {}{stats} ", c.hash)
        }
        FileItem::WorkTree => match app.diff.doc.as_ref() {
            Some(d) if d.added + d.removed > 0 => {
                format!(" {WORKTREE_TITLE} +{} -{} ", d.added, d.removed)
            }
            _ => format!(" {WORKTREE_TITLE} "),
        },
        FileItem::More => " Diff ".into(),
    }
}

pub(super) fn draw_diff(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let tone = pane_tone(app, Pane::Diff);
    let block = pane_block(app, title(app), tone, Pane::Diff);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = inner.height as usize;
    app.diff.page.set(view);
    let dim_style = Style::default().fg(th.dim);

    let Some(doc) = app.diff.doc.as_ref() else {
        app.diff.max_scroll.set(0);
        let (text, style) = match (&app.diff.error, app.diff.loading, &app.diff.key) {
            (Some(err), _, _) => (err.clone(), Style::default().fg(th.error)),
            (None, true, _) => ("Loading…".into(), dim_style),
            (None, false, Some(_)) => (String::new(), dim_style),
            (None, false, None) => ("Select a file to see its changes".into(), dim_style),
        };
        empty_line(f, inner, &truncate_w(&text, inner.width as usize), style);
        return;
    };

    let gutter = Gutter::of(&doc.lines);
    let numbered = inner.width as usize >= 50 && !gutter.is_empty();
    // Only code lines carry the gutter; headers, meta, and hunk lines start
    // at the left edge and get the full width.
    let full_w = (inner.width as usize).max(1);
    let has_gutter = |line: &DiffLine| numbered && is_code(line.kind);
    let width_of = |line: &DiffLine| {
        if has_gutter(line) {
            full_w.saturating_sub(gutter.width()).max(1)
        } else {
            full_w
        }
    };

    // Visual rows: (line index, segment, first segment of its line). A
    // commit message and a file path always wrap (a path at `/`); other
    // lines wrap only with `.`. When nothing wraps, only the visible window
    // is materialized.
    let wraps = |line: &DiffLine| {
        !matches!(
            line.kind,
            LineKind::Section | LineKind::File | LineKind::Rule | LineKind::Gap
        ) && (app.diff.wrap || is_message(line.kind))
    };
    let any_wrap = app.diff.wrap
        || doc
            .lines
            .iter()
            .any(|l| is_message(l.kind) || l.kind == LineKind::Path);
    let split_on = numbered && app.diff.split && full_w >= split::SPLIT_MIN_W;
    // One wrapped or cut line: its segments within `width`.
    let segments = |line: &DiffLine, width: usize| -> Vec<String> {
        if line.kind == LineKind::Path {
            wrap_path(&line.text, width)
        } else if wraps(line) {
            wrap_w(&line.text, width)
        } else if matches!(
            line.kind,
            LineKind::Section | LineKind::File | LineKind::Rule
        ) {
            vec![line.text.clone()]
        } else {
            vec![truncate_w(&line.text, width)]
        }
    };
    let layout = (app.diff.wrap, split_on, full_w);
    let total;
    let scroll;
    let mut rows: Vec<Row> = Vec::new();
    if split_on || any_wrap {
        let mut all = Vec::new();
        if split_on {
            let (lw, rw) = split::sides(full_w);
            let code_w =
                |side: usize, old: bool| side.saturating_sub(gutter.split_side(old)).max(1);
            for item in split::pair(&doc.lines) {
                match item {
                    split::Item::Full(i) => {
                        let line = &doc.lines[i];
                        all.extend(
                            segs_of(i, segments(line, width_of(line)))
                                .into_iter()
                                .map(Row::Line),
                        );
                    }
                    split::Item::Pair(l, r) => {
                        let side = |i: Option<usize>, w: usize, old: bool| -> Vec<Seg> {
                            i.map(|i| segs_of(i, segments(&doc.lines[i], code_w(w, old))))
                                .unwrap_or_default()
                        };
                        let (ls, rs) = (side(l, lw, true), side(r, rw, false));
                        let n = ls.len().max(rs.len()).max(1);
                        let (mut ls, mut rs) = (ls.into_iter(), rs.into_iter());
                        for _ in 0..n {
                            all.push(Row::Pair(ls.next(), rs.next()));
                        }
                    }
                }
            }
        } else {
            for (i, line) in doc.lines.iter().enumerate() {
                all.extend(
                    segs_of(i, segments(line, width_of(line)))
                        .into_iter()
                        .map(Row::Line),
                );
            }
        }
        total = all.len() + usize::from(doc.truncated);
        let max = total.saturating_sub(view);
        app.diff.max_scroll.set(max);
        scroll = top_row(
            app,
            layout,
            max,
            |line| {
                all.iter()
                    .position(|r| r.line().is_some_and(|l| l >= line))
                    .unwrap_or(all.len())
            },
            |row| all.get(row).and_then(Row::line).unwrap_or(0),
        );
        rows = all.into_iter().skip(scroll).take(view).collect();
    } else {
        total = doc.lines.len() + usize::from(doc.truncated);
        let max = total.saturating_sub(view);
        app.diff.max_scroll.set(max);
        // One row per line.
        scroll = top_row(app, layout, max, |line| line, |row| row);
        for (i, line) in doc.lines.iter().enumerate().skip(scroll).take(view) {
            rows.push(Row::Line(Seg {
                line: i,
                text: truncate_w(&line.text, width_of(line)),
                first: true,
                start: 0,
            }));
        }
    }

    let num_style = Style::default().fg(th.dim);
    // Section and file titles are drawn over the full pane width afterwards,
    // joined to the side borders; their paragraph rows stay empty.
    let mut dividers: Vec<Divider> = Vec::new();
    let mut lines: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .map(|(row, r)| {
            let Seg {
                line: i,
                text: seg,
                first,
                start,
            } = match r {
                Row::Line(seg) => seg,
                Row::Pair(l, r) => {
                    let (lw, rw) = split::sides(full_w);
                    let mut spans = side_spans(doc, l, lw, true, &gutter, th);
                    spans.push(Span::styled("│", Style::default().fg(th.dim)));
                    spans.extend(side_spans(doc, r, rw, false, &gutter, th));
                    return Line::from(spans);
                }
            };
            let line = &doc.lines[i];
            let style = kind_style(line.kind, th);
            if matches!(
                line.kind,
                LineKind::Section | LineKind::File | LineKind::Rule
            ) {
                dividers.push(Divider {
                    y: inner.y + row as u16,
                    // A rule carries the stat summary, or nothing (`---`).
                    label: if line.kind == LineKind::Rule && line.text == "---" {
                        String::new()
                    } else {
                        line.text.clone()
                    },
                    style,
                });
                return Line::default();
            }
            // The gap marker sits in the first number column of the gutter.
            if line.kind == LineKind::Gap {
                let at = if numbered { gutter.gap() } else { "⋯".into() };
                return Line::from(Span::styled(at, style));
            }
            if line.kind == LineKind::Counts {
                return Line::from(counts_spans(&seg, th));
            }
            let mut spans = Vec::new();
            if has_gutter(line) {
                spans.push(Span::styled(gutter.text(line, !first), num_style));
            }
            spans.extend(marked_spans(seg, start, &line.marks, style));
            Line::from(spans)
        })
        .collect();
    if doc.truncated && lines.len() < view && scroll + lines.len() + 1 >= total {
        lines.push(Line::from(Span::styled(
            format!(
                "… cut at {} lines (max_diff_lines in config.toml)",
                app.cfg.max_diff_lines
            ),
            Style::default().fg(th.warning),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
    for d in &dividers {
        draw_divider(f, area, d, tone, th);
    }
    draw_vscrollbar(f, area, tone == Tone::Focused, scroll, total, view, th);
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
