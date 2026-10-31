//! Frame rendering. Order is fixed: theme base → header / filter line / panes
//! / footer → backdrop fade → the active overlay.

mod commit_files;
mod diff;
mod files;
mod header;
mod repos;
mod split;

use super::components::input::input_view;
use super::components::modal::{backdrop_dimmed, dim_backdrop};
use super::components::text::truncate_w;
use super::{App, Level, Pane, UiMode};
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
pub(crate) use header::LOGO;

/// Below this the panes are not drawn at all.
const MIN_W: u16 = 40;
const MIN_H: u16 = 10;

pub fn draw(f: &mut Frame, app: &App) {
    let th = &app.theme;
    f.render_widget(Block::default().style(th.base_style()), f.area());
    let area = f.area();
    if area.width < MIN_W || area.height < MIN_H {
        f.render_widget(
            Paragraph::new(Span::styled(
                "Terminal too small for ular-git",
                th.soft_dim(),
            )),
            area,
        );
        return;
    }
    let header_h = 5.min(area.height.saturating_sub(4));
    let filter_h = if app.mode == UiMode::Filter { 3 } else { 0 };
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Length(filter_h),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);
    header::draw_header(f, app, root[0]);
    if filter_h > 0 {
        draw_filter_line(f, app, root[1]);
    }
    let repos_fit = repos::fit_width(&app.store, &app.statuses);
    for (pane, rect) in pane_layout(root[2], app.focus, repos_fit) {
        match pane {
            Pane::Repos => repos::draw_repos(f, app, rect),
            Pane::Files => match &app.commit_view {
                Some(view) => commit_files::draw_commit_files(f, app, rect, view),
                None => files::draw_files(f, app, rect),
            },
            Pane::Diff => diff::draw_diff(f, app, rect),
        }
    }
    draw_footer(f, app, root[3]);

    if backdrop_dimmed(app.mode) {
        dim_backdrop(f, th);
    }
    use super::overlays::*;
    match app.mode {
        UiMode::Normal | UiMode::Filter => {}
        UiMode::AddRepo => add_repo::draw_add_repo(f, app),
        UiMode::GroupName => group::draw_group_name(f, app),
        UiMode::GroupPick => group::draw_group_pick(f, app),
        UiMode::Confirm => confirm::draw_confirm(f, app),
        UiMode::Commit => commit::draw_commit(f, app),
        UiMode::OpenWith => open_with::draw_open_with(f, app),
        UiMode::QuickCommand => super::quick::draw_quick_command(f, app),
        UiMode::ThemeSelect => theme::draw_theme_select(f, app),
        UiMode::Help => help::draw_help(f, app),
        UiMode::Message => message::draw_message_modal(f, app),
    }
}

/// Columns from which all three panes show whatever has focus.
pub(crate) const THREE_PANES_W: u16 = 160;

/// How many panes fit side by side at `width`: three when wide, two, or one
/// when narrow.
pub(crate) fn visible_panes(width: u16) -> usize {
    match width {
        THREE_PANES_W.. => 3,
        64.. => 2,
        _ => 1,
    }
}

/// Visible panes and their areas. Wide screens show all three. Otherwise
/// Repos focus shows Repos + Files, Files focus Files + Diff, and Diff focus
/// the diff alone (the full width for reading); a narrow screen shows the
/// focused pane alone. The focused pane is always visible, so moving left or
/// right reveals the pane it moves to. Beside another pane, Repos shrinks to
/// `repos_fit` ([`repos::fit_width`]) within its usual range.
pub(crate) fn pane_layout(area: Rect, focus: Pane, repos_fit: u16) -> Vec<(Pane, Rect)> {
    let w = area.width;
    let k = match visible_panes(w) {
        2 if focus == Pane::Diff => 1,
        k => k,
    };
    let start = match k {
        3 => 0,
        1 => focus.index(),
        _ if focus == Pane::Repos => 0,
        _ => 1,
    };
    let panes = &Pane::ALL[start..start + k];
    let pct = |p: u16, lo: u16, hi: u16| (w * p / 100).clamp(lo, hi);
    let repos = |p: u16, lo: u16, hi: u16| repos_fit.clamp(lo, pct(p, lo, hi));
    let widths: Vec<Constraint> = match panes {
        [_, _, _] => vec![
            Constraint::Length(repos(22, 30, 44)),
            Constraint::Length(pct(28, 40, 80)),
            Constraint::Min(0),
        ],
        [Pane::Repos, _] => vec![Constraint::Length(repos(42, 28, 44)), Constraint::Min(0)],
        [_, _] => vec![Constraint::Length(pct(40, 30, 80)), Constraint::Min(0)],
        _ => vec![Constraint::Min(0)],
    };
    let rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(widths)
        .split(area);
    panes.iter().copied().zip(rects.iter().copied()).collect()
}

/// Pane look: focused → thick accent; another pane focused → plain soft-dim
/// (frame, title, and body); a dialog or the filter line owns input → plain
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Focused,
    Dimmed,
    Neutral,
}

pub(crate) fn pane_tone(app: &App, pane: Pane) -> Tone {
    if app.mode != UiMode::Normal {
        Tone::Neutral
    } else if app.focus == pane {
        Tone::Focused
    } else {
        Tone::Dimmed
    }
}

/// Framed pane block with the navigation arrows of a focused pane (only
/// toward panes the focus can actually move to).
pub(crate) fn pane_block(app: &App, title: String, tone: Tone, pane: Pane) -> Block<'static> {
    let th = &app.theme;
    let (border, style) = match tone {
        Tone::Focused => (
            BorderType::Thick,
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ),
        Tone::Dimmed => (BorderType::Plain, th.soft_dim()),
        Tone::Neutral => (BorderType::Plain, Style::default()),
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(border)
        .border_style(style)
        .title(Span::styled(title, style))
        .title_alignment(ratatui::layout::Alignment::Center)
        .padding(Padding::horizontal(1));
    if tone == Tone::Focused {
        if pane != Pane::Repos {
            block = block.title_top(Line::from("━ ← ").left_aligned().style(style));
        }
        if pane != Pane::Diff && app.can_move_right() {
            block = block.title_top(Line::from(" → ━").right_aligned().style(style));
        }
    }
    block
}

/// One title line to draw over the pane width.
pub(crate) struct Divider {
    pub y: u16,
    pub label: String,
    pub style: Style,
}

/// Full-width title line from the left border to the right border with the
/// label centered: `├──── Unstaged changes ────┤`, `├──── src/app.rs ────┤`,
/// or a bare rule. `┠` / `┨` join a focused, thick frame. The joints take the
/// border's style, the line and label the title's.
pub(crate) fn draw_divider(f: &mut Frame, area: Rect, d: &Divider, tone: Tone, th: &Theme) {
    let (y, label, style) = (d.y, d.label.as_str(), d.style);
    if area.width < 4 || y >= area.bottom() {
        return;
    }
    let (joints, border) = match tone {
        Tone::Focused => (
            ("┠", "┨"),
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ),
        Tone::Dimmed => (("├", "┤"), th.soft_dim()),
        Tone::Neutral => (("├", "┤"), Style::default()),
    };
    let inner_w = (area.width - 2) as usize;
    let label = format!(" {} ", truncate_w(label, inner_w.saturating_sub(6)));
    let text = if label.trim().is_empty() {
        "─".repeat(inner_w)
    } else {
        let fill = inner_w.saturating_sub(label.width());
        let left = fill / 2;
        format!("{}{label}{}", "─".repeat(left), "─".repeat(fill - left))
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(joints.0, border),
            Span::styled(text, style),
            Span::styled(joints.1, border),
        ])),
        Rect::new(area.x, y, area.width, 1),
    );
}

/// Body text style of a pane in `tone`: dimmed panes fade everything.
pub(crate) fn body_style(tone: Tone, style: Style, th: &Theme) -> Style {
    if tone == Tone::Dimmed {
        th.soft_dim()
    } else {
        style
    }
}

/// Selected-row style: focused → selection colors + bold; another pane
/// focused → the inactive bar with plain text at normal weight; a dialog owns
/// input → inactive bar + bold.
pub(crate) fn selection_style(tone: Tone, th: &Theme) -> Style {
    match tone {
        Tone::Focused => Style::default()
            .fg(th.selection_fg)
            .bg(th.selection_bg)
            .add_modifier(Modifier::BOLD),
        // Not the rust-tui weak reverse (muted text reversed over the
        // inactive color): on light themes that left pale text on a mid-tone
        // bar. The light bar with plain text keeps both readable.
        Tone::Dimmed => Style::default().fg(th.fg).bg(th.selection_inactive_bg),
        Tone::Neutral => Style::default()
            .fg(th.selection_fg)
            .bg(th.selection_inactive_bg)
            .add_modifier(Modifier::BOLD),
    }
}

/// Empty-state line drawn in place of a list.
pub(crate) fn empty_line(f: &mut Frame, area: Rect, text: &str, style: Style) {
    f.render_widget(
        Paragraph::new(Span::styled(text.to_string(), style))
            .wrap(ratatui::widgets::Wrap { trim: true }),
        area,
    );
}

fn draw_filter_line(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let accent = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(accent)
        .title(Span::styled(" Filter ", accent))
        .title_alignment(ratatui::layout::Alignment::Center)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let prompt = "/ ";
    let width = (inner.width as usize).saturating_sub(prompt.len());
    let (visible, cursor_x) = input_view(&app.filter, width);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prompt, accent),
            Span::raw(visible),
        ])),
        inner,
    );
    f.set_cursor_position((inner.x + prompt.len() as u16 + cursor_x, inner.y));
}

/// Footer: the latest result while it lasts, then the pane shortcuts with the
/// focused pane highlighted.
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let line = match &app.notice {
        Some(n) => {
            let color = match n.level {
                Level::Info => th.accent,
                Level::Ok => th.success,
                Level::Warn => th.warning,
                Level::Error => th.error,
            };
            Line::from(vec![
                Span::styled(
                    format!("{} ", n.level.tag()),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(super::components::text::sanitize_single_line(&n.text)),
            ])
        }
        None => {
            let mut spans = Vec::new();
            for (i, pane) in Pane::ALL.iter().enumerate() {
                let current = *pane == app.focus;
                let key_style = if current {
                    Style::default()
                        .fg(th.selection_fg)
                        .bg(th.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    th.key_style()
                };
                let name_style = if current {
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
                } else {
                    th.soft_dim()
                };
                if i > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(Span::styled(format!("<{}>", i + 1), key_style));
                spans.push(Span::styled(format!(" {}", pane.title()), name_style));
            }
            Line::from(spans)
        }
    };
    f.render_widget(Paragraph::new(line), area.inner(Margin::new(1, 0)));
}

#[cfg(test)]
mod tests;
