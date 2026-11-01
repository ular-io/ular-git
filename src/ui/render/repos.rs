//! Repository pane: group headers, repositories with status badges, and
//! their linked worktrees indented beneath them.

use super::{empty_line, pane_block, pane_tone, selection_style, Tone};
use crate::store::RepoStore;
use crate::theme::Theme;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::truncate_w;
use crate::ui::scan::TreeState;
use crate::ui::tree::Row;
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};
use std::collections::HashMap;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

/// Right-aligned badges: `Ⓜ 3` changed, `!1` conflicts, `↑2` ahead, `↓1`
/// behind, `Ⓢ 1` stashes, `…` not scanned yet, `✗` scan failed. A clean
/// tree shows no badge, so the rows that need attention stand out.
///
/// Circled letters are East Asian Ambiguous: counted as one cell, but many
/// fonts draw them two cells wide. The space after them ([`CHANGED`],
/// [`STASH`]) gives the glyph room so it never covers the count.
pub(crate) const CHANGED: &str = "Ⓜ ";
pub(crate) const STASH: &str = "Ⓢ ";

pub(crate) fn badges(tree: Option<&TreeState>, scanning: bool, th: &Theme) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let Some(tree) = tree else {
        let mark = if scanning { "…" } else { "?" };
        return vec![Span::styled(mark, th.soft_dim())];
    };
    let Some(s) = tree.status.as_ref() else {
        return match &tree.error {
            Some(_) => vec![Span::styled("✗", Style::default().fg(th.error))],
            None => vec![Span::styled("…", th.soft_dim())],
        };
    };
    let c = s.counts();
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
    if s.branch.ahead > 0 {
        push(
            format!("↑{}", s.branch.ahead),
            Style::default().fg(th.branch),
        );
    }
    if s.branch.behind > 0 {
        push(
            format!("↓{}", s.branch.behind),
            Style::default().fg(th.branch),
        );
    }
    if s.stashes > 0 {
        push(format!("{STASH}{}", s.stashes), th.soft_dim());
    }
    if tree.error.is_some() {
        push("✗".into(), Style::default().fg(th.error));
    }
    out
}

/// Cells kept for the badges when the pane fits its names (`Ⓜ 12 ↑3`).
/// Longer badges cut the name rather than move the panes.
const BADGE_ROOM: usize = 7;

/// Pane width that fits the longest name: every registered repository, group
/// header, and known worktree folder, whether filtered out, folded, or shown.
/// Measuring them all keeps the panes still while filtering, folding, and
/// rescanning. The layout caps it; branches and wider badges are cut.
pub(crate) fn fit_width(store: &RepoStore, statuses: &HashMap<PathBuf, TreeState>) -> u16 {
    let mut left = 0;
    for repo in &store.repos {
        let indent = if repo.group.is_some() { 3 } else { 1 };
        left = left.max(indent + repo.name().width());
        let worktrees = statuses
            .get(&repo.path)
            .and_then(|t| t.status.as_ref())
            .map(|s| s.worktrees.as_slice())
            .unwrap_or_default();
        for w in worktrees {
            let name = w.path.file_name().unwrap_or_default().to_string_lossy();
            // The `├ ` / `└ ` elbow.
            left = left.max(indent + 2 + name.width());
        }
    }
    for g in &store.groups {
        // ` ▾ ` before the name.
        left = left.max(3 + g.name.width());
    }
    // Two borders, the left padding, the gap before the badges, and the
    // right padding cell of the selection bar.
    u16::try_from(left + BADGE_ROOM + 4).unwrap_or(u16::MAX)
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.width()).sum()
}

/// One full-width row: left text, right badges, padded so the selection bar
/// spans the whole width. Selected rows paint everything in `sel`.
fn row_line(
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
    sel: Option<Style>,
    fill: Style,
) -> Line<'static> {
    let right_w = spans_width(&right);
    let room = width.saturating_sub(right_w + usize::from(right_w > 0));
    // Cut the left side to its room, keeping per-span styles.
    let mut used = 0;
    let mut spans: Vec<Span> = Vec::new();
    for span in left {
        let w = span.content.width();
        if used + w <= room {
            used += w;
            spans.push(span);
        } else {
            let cut = truncate_w(&span.content, room - used);
            used += cut.width();
            spans.push(Span::styled(cut, span.style));
            break;
        }
    }
    let pad = width.saturating_sub(used + right_w);
    spans.push(Span::styled(" ".repeat(pad), fill));
    spans.extend(right);
    // The bar runs through the right padding cell up to the border.
    spans.push(Span::styled(" ", fill));
    if let Some(sel) = sel {
        for s in &mut spans {
            s.style = sel;
        }
    }
    Line::from(spans)
}

pub(super) fn draw_repos(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let tone = pane_tone(app, Pane::Repos);
    let total = app.store.repos.len();
    let shown = app
        .rows
        .iter()
        .filter(|r| matches!(r, Row::Repo { .. }))
        .count();
    let title = match app.filter_label() {
        None => format!(" Repos[{total}] "),
        Some(label) => format!(" Repos[{label}: {shown}/{total}] "),
    };
    let block = pane_block(app, title, tone, Pane::Repos);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = inner.height as usize;
    app.repo_page.set(view);
    if app.rows.is_empty() {
        let text = if total == 0 {
            "No repositories. Press + to add one."
        } else if app.filter.value.trim().is_empty() && app.list_filtered() {
            "No repository has changes, commits to pull or push, or stashes"
        } else {
            "No repository matches the filter"
        };
        empty_line(f, inner, text, Style::default().fg(th.dim));
        return;
    }
    // Rows extend one cell into the left padding so the selection bar has a
    // one-cell margin on both sides.
    let list_area = Rect {
        x: inner.x.saturating_sub(1),
        width: inner.width + 2,
        ..inner
    };
    // Text width; the selection bar adds the right padding cell after it.
    let w = list_area.width.saturating_sub(1) as usize;
    let items: Vec<ListItem> = app
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let sel = (i == app.repo_sel).then(|| selection_style(tone, th));
            let line = match row {
                Row::Group {
                    name,
                    changed,
                    collapsed,
                    ..
                } => {
                    let fill = Style::default().bg(th.group_bg);
                    let arrow = if *collapsed { "▸ " } else { "▾ " };
                    let label = name.clone().unwrap_or_else(|| "Ungrouped".into());
                    // Headers are muted: they label the rows below and
                    // should not outweigh the repositories themselves.
                    let left = vec![
                        Span::styled(format!(" {arrow}"), fill.fg(th.muted)),
                        Span::styled(label, fill.fg(th.muted)),
                    ];
                    // An open group's rows carry their own badges; a folded
                    // group shows only how many changed files it hides.
                    let right = if *collapsed && *changed > 0 {
                        vec![Span::styled(
                            format!("{CHANGED}{changed}"),
                            fill.fg(th.warning),
                        )]
                    } else {
                        Vec::new()
                    };
                    row_line(left, right, w, sel, fill)
                }
                Row::Repo { idx, nested, .. } => {
                    let repo = &app.store.repos[*idx];
                    let tree = app.statuses.get(&repo.path);
                    let indent = if *nested { "   " } else { " " };
                    let left = vec![Span::raw(format!("{indent}{}", repo.name()))];
                    let right = with_op(
                        app,
                        &repo.path,
                        badges(tree, app.scan.is_scanning(&repo.path), th),
                    );
                    row_line(left, right, w, sel, Style::default())
                }
                Row::Worktree {
                    path,
                    branch,
                    last,
                    nested,
                    ..
                } => {
                    let tree = app.statuses.get(path);
                    let indent = if *nested { "   " } else { " " };
                    let elbow = if *last { "└ " } else { "├ " };
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let mut left = vec![
                        Span::styled(format!("{indent}{elbow}"), Style::default().fg(th.dim)),
                        Span::raw(name),
                    ];
                    if let Some(b) = branch {
                        left.push(Span::styled(format!(" {b}"), th.soft_dim()));
                    }
                    let right = with_op(app, path, badges(tree, app.scan.is_scanning(path), th));
                    row_line(left, right, w, sel, Style::default())
                }
            };
            ListItem::new(line)
        })
        .collect();
    let mut state = app.repo_list.borrow_mut();
    state.select(Some(app.repo_sel));
    f.render_stateful_widget(List::new(items), list_area, &mut state);
    let offset = state.offset();
    drop(state);
    draw_vscrollbar(
        f,
        area,
        tone == Tone::Focused,
        offset,
        app.rows.len(),
        view,
        th,
    );
}

/// A running push / pull leads the badges (`push…`).
fn with_op(app: &App, path: &std::path::Path, badges: Vec<Span<'static>>) -> Vec<Span<'static>> {
    let Some(op) = app.remote_ops.get(path) else {
        return badges;
    };
    let th = &app.theme;
    let mut out = vec![
        Span::styled(format!("{}…", op.label()), Style::default().fg(th.accent)),
        Span::raw(" "),
    ];
    out.extend(badges);
    out
}
