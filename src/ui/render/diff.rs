//! Diff pane: colored, line-numbered diff of the selected file or commit.

use super::{draw_divider, empty_line, pane_block, pane_tone, Divider, Tone};
use crate::git::diff::{DiffLine, LineKind};
use crate::theme::{color_rgb, Theme};
use crate::ui::components::modal::fade_toward;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_left_w, truncate_w, wrap_w};
use crate::ui::files::FileItem;
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

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
        LineKind::Byline => th.soft_dim(),
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
    let stats = app
        .diff
        .doc
        .as_ref()
        .map(|d| format!(" +{} -{}", d.added, d.removed))
        .unwrap_or_default();
    match item {
        FileItem::File(f) => format!(" {}{stats} ", truncate_left_w(&f.path, 48)),
        FileItem::Commit(c, _) => format!(" {}{stats} ", c.hash),
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
    // commit message always wraps; other lines wrap only with `.`. When
    // nothing wraps, only the visible window is materialized.
    let wraps = |line: &DiffLine| {
        !matches!(
            line.kind,
            LineKind::Section | LineKind::File | LineKind::Rule | LineKind::Gap
        ) && (app.diff.wrap || is_message(line.kind))
    };
    let any_wrap = app.diff.wrap || doc.lines.iter().any(|l| is_message(l.kind));
    let total;
    let mut rows: Vec<(usize, String, bool)> = Vec::new();
    if any_wrap {
        let mut all = Vec::new();
        for (i, line) in doc.lines.iter().enumerate() {
            let segs = if wraps(line) {
                wrap_w(&line.text, width_of(line))
            } else if matches!(
                line.kind,
                LineKind::Section | LineKind::File | LineKind::Rule
            ) {
                vec![line.text.clone()]
            } else {
                vec![truncate_w(&line.text, width_of(line))]
            };
            if segs.is_empty() {
                all.push((i, String::new(), true));
            }
            for (k, seg) in segs.into_iter().enumerate() {
                all.push((i, seg, k == 0));
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
            rows.push((i, truncate_w(&line.text, width_of(line)), true));
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
        .map(|(row, (i, seg, first))| {
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
