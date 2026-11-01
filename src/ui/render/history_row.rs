//! Pieces of a history row in the file pane: graph cells colored by lane,
//! and branch / tag badges.

use crate::git::graph::Cell;
use crate::git::history::{RefKind, RefLabel};
use crate::theme::{color_rgb, Theme};
use crate::ui::components::modal::fade_toward;
use crate::ui::components::text::truncate_w;
use crate::ui::files::Mark;
use crate::ui::history_view::RefLabels;
use ratatui::{
    style::{Color, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

/// Longest ref name shown in a label; longer names are cut with `…`.
const REF_NAME_MAX: usize = 24;

/// Branch and tag labels drawn as solid badges (the theme background as text
/// on the label color, no icon), joined into one strip; a `│` in the
/// background color splits two badges of the same color (different colors
/// need no line), within `room` cells; a label that does not
/// fit is left out with the rest. The kind is told by
/// color: local branches the branch color, remote branches
/// the hunk color, tags the warning color, each moved a quarter toward the
/// text color, then faded back a third toward the background so the badge
/// stays readable without outweighing the subject. Labels look the same
/// whether or not the pane has focus.
pub(super) fn ref_spans(
    refs: &[RefLabel],
    labels: RefLabels,
    room: usize,
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
        let badge = mix(mix(color_rgb(color), ink, 25), th.bg_rgb(), 35);
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
pub(super) fn mix((r, g, b): (u8, u8, u8), (tr, tg, tb): (u8, u8, u8), pct: u32) -> (u8, u8, u8) {
    let m = |c: u8, t: u8| ((u32::from(c) * (100 - pct) + u32::from(t) * pct) / 100) as u8;
    (m(r, tr), m(g, tg), m(b, tb))
}

/// Graph cells colored by lane (a lane keeps its color) and faded so history
/// stays in the background. A commit to push is `↑` and one to pull `↓`, in
/// the branch color.
pub(super) fn graph_spans(
    graph: &[Cell],
    width: usize,
    mark: Mark,
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
        // Marked nodes in full color, lanes faded once, whatever the focus.
        let style = if marked {
            Style::default().fg(th.branch)
        } else {
            Style::default().fg(fade_toward(color_rgb(color), th.bg_rgb()))
        };
        spans.push(Span::styled(glyph.to_string(), style));
        used += 1;
    }
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used) + 1)));
    spans
}
