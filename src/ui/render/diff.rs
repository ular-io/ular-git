//! Diff pane: colored, line-numbered diff of the selected file or commit.

use super::{draw_divider, empty_line, pane_block, pane_tone, split, Divider, Tone};
use crate::git::diff::{DiffLine, LineKind};
use crate::theme::{color_rgb, Theme};
use crate::ui::components::modal::fade_toward;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_w, wrap_path, wrap_w};
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

/// `old new ` line-number gutter.
const GUTTER_W: usize = 10;

fn kind_style(kind: LineKind, tone: Tone, th: &Theme) -> Style {
    let colored = |c: Color| {
        if tone == Tone::Dimmed {
            Style::default().fg(fade_toward(color_rgb(c), th.bg_rgb()))
        } else {
            Style::default().fg(c)
        }
    };
    let plain = |s: Style| {
        if tone == Tone::Dimmed {
            th.soft_dim()
        } else {
            s
        }
    };
    match kind {
        LineKind::Section | LineKind::File | LineKind::Rule => {
            colored(th.accent).add_modifier(Modifier::BOLD)
        }
        LineKind::Byline | LineKind::Path => th.soft_dim(),
        // Drawn per sign by `counts_spans`.
        LineKind::Counts => plain(Style::default()),
        LineKind::Subject => plain(Style::default().add_modifier(Modifier::BOLD)),
        LineKind::Header | LineKind::Stat | LineKind::Context => plain(Style::default()),
        LineKind::Meta => th.soft_dim(),
        LineKind::Gap => Style::default().fg(th.dim),
        LineKind::Add => colored(th.diff_add),
        LineKind::Del => colored(th.diff_del),
    }
}

/// A shown commit's message lines, which always wrap.
fn is_message(kind: LineKind) -> bool {
    matches!(
        kind,
        LineKind::Byline | LineKind::Subject | LineKind::Header
    )
}

/// Lines that carry old/new numbers (and the gutter).
fn is_code(kind: LineKind) -> bool {
    matches!(kind, LineKind::Add | LineKind::Del | LineKind::Context)
}

/// `+20 -3` with each side in its diff color.
fn counts_spans(text: &str, tone: Tone, th: &Theme) -> Vec<Span<'static>> {
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
        spans.push(Span::styled(part.to_string(), kind_style(kind, tone, th)));
    }
    spans
}

/// One visual row: a segment of a line across the pane, or the two sides of
/// the side-by-side layout (line index, segment, first segment of its line).
enum Row {
    Line(usize, String, bool),
    Pair(Option<(usize, String, bool)>, Option<(usize, String, bool)>),
}

/// One side of a side-by-side row: its line number (old on the left, new
/// on the right; only on a line's first segment) and the segment, padded to
/// `width`.
fn side_spans(
    doc: &crate::git::diff::DiffDoc,
    side: Option<(usize, String, bool)>,
    width: usize,
    old: bool,
    tone: Tone,
    th: &Theme,
) -> Vec<Span<'static>> {
    let Some((i, seg, first)) = side else {
        return vec![Span::raw(" ".repeat(width))];
    };
    let line = &doc.lines[i];
    let n = if old { line.old } else { line.new };
    let num = match (first, n) {
        (true, Some(n)) => format!("{n:>4} "),
        _ => " ".repeat(split::SIDE_GUTTER_W),
    };
    let pad = width.saturating_sub(split::SIDE_GUTTER_W + seg.width());
    vec![
        Span::styled(num, Style::default().fg(th.dim)),
        Span::styled(seg, kind_style(line.kind, tone, th)),
        Span::raw(" ".repeat(pad)),
    ]
}

fn gutter(line: &DiffLine, first: bool) -> String {
    if !first {
        return " ".repeat(GUTTER_W);
    }
    let n = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_default();
    format!("{:>4} {:>4} ", n(line.old), n(line.new))
}

fn title(app: &App) -> String {
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
                format!(" Working Tree +{} -{} ", d.added, d.removed)
            }
            _ => " Working Tree ".into(),
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

    let numbered =
        inner.width as usize >= 50 && doc.lines.iter().any(|l| l.new.is_some() || l.old.is_some());
    // Only code lines carry the gutter; headers, meta, and hunk lines start
    // at the left edge and get the full width.
    let full_w = (inner.width as usize).max(1);
    let has_gutter = |line: &DiffLine| numbered && is_code(line.kind);
    let width_of = |line: &DiffLine| {
        if has_gutter(line) {
            full_w.saturating_sub(GUTTER_W).max(1)
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
    let total;
    let mut rows: Vec<Row> = Vec::new();
    if split_on {
        let (lw, rw) = split::sides(full_w);
        let code_w = |side: usize| side.saturating_sub(split::SIDE_GUTTER_W).max(1);
        let mut all = Vec::new();
        for item in split::pair(&doc.lines) {
            match item {
                split::Item::Full(i) => {
                    let line = &doc.lines[i];
                    let segs = segments(line, width_of(line));
                    if segs.is_empty() {
                        all.push(Row::Line(i, String::new(), true));
                    }
                    for (k, seg) in segs.into_iter().enumerate() {
                        all.push(Row::Line(i, seg, k == 0));
                    }
                }
                split::Item::Pair(l, r) => {
                    let side = |i: Option<usize>, w: usize| -> Vec<String> {
                        i.map(|i| segments(&doc.lines[i], code_w(w)))
                            .unwrap_or_default()
                    };
                    let (ls, rs) = (side(l, lw), side(r, rw));
                    for k in 0..ls.len().max(rs.len()).max(1) {
                        let at = |i: Option<usize>, segs: &[String]| {
                            i.zip(segs.get(k).cloned()).map(|(i, s)| (i, s, k == 0))
                        };
                        all.push(Row::Pair(at(l, &ls), at(r, &rs)));
                    }
                }
            }
        }
        total = all.len() + usize::from(doc.truncated);
        let max = total.saturating_sub(view);
        app.diff.max_scroll.set(max);
        let scroll = app.diff.scroll.min(max);
        rows = all.into_iter().skip(scroll).take(view).collect();
    } else if any_wrap {
        let mut all = Vec::new();
        for (i, line) in doc.lines.iter().enumerate() {
            let segs = segments(line, width_of(line));
            if segs.is_empty() {
                all.push(Row::Line(i, String::new(), true));
            }
            for (k, seg) in segs.into_iter().enumerate() {
                all.push(Row::Line(i, seg, k == 0));
            }
        }
        total = all.len() + usize::from(doc.truncated);
        let max = total.saturating_sub(view);
        app.diff.max_scroll.set(max);
        let scroll = app.diff.scroll.min(max);
        rows = all.into_iter().skip(scroll).take(view).collect();
    } else {
        total = doc.lines.len() + usize::from(doc.truncated);
        let max = total.saturating_sub(view);
        app.diff.max_scroll.set(max);
        let scroll = app.diff.scroll.min(max);
        for (i, line) in doc.lines.iter().enumerate().skip(scroll).take(view) {
            rows.push(Row::Line(i, truncate_w(&line.text, width_of(line)), true));
        }
    }
    let scroll = app.diff.scroll.min(app.diff.max_scroll.get());

    let num_style = Style::default().fg(th.dim);
    // Section and file titles are drawn over the full pane width afterwards,
    // joined to the side borders; their paragraph rows stay empty.
    let mut dividers: Vec<Divider> = Vec::new();
    let mut lines: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .map(|(row, r)| {
            let (i, seg, first) = match r {
                Row::Line(i, seg, first) => (i, seg, first),
                Row::Pair(l, r) => {
                    let (lw, rw) = split::sides(full_w);
                    let mut spans = side_spans(doc, l, lw, true, tone, th);
                    spans.push(Span::styled("│", Style::default().fg(th.dim)));
                    spans.extend(side_spans(doc, r, rw, false, tone, th));
                    return Line::from(spans);
                }
            };
            let line = &doc.lines[i];
            let style = kind_style(line.kind, tone, th);
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
            // The gap marker sits in the old-number column of the gutter.
            if line.kind == LineKind::Gap {
                let at = if numbered { "   ⋯" } else { "⋯" };
                return Line::from(Span::styled(at, style));
            }
            if line.kind == LineKind::Counts {
                return Line::from(counts_spans(&seg, tone, th));
            }
            let mut spans = Vec::new();
            if has_gutter(line) {
                spans.push(Span::styled(gutter(line, first), num_style));
            }
            spans.push(Span::styled(seg, style));
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
