//! Header (no border, five rows): context column, the focused pane's two
//! shortcut columns, the common column, and the logo. When space runs out
//! the logo goes first, then shortcut columns from the right.

use crate::config::collapse_home;
use crate::ui::components::text::{pad_w, truncate_left_w, truncate_w};
use crate::ui::keys::{self, Hint};
use crate::ui::tree::Row;
use crate::ui::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

/// "ugit" (the short alias of ular-git) in ASCII, every row the same width
/// (fixed by a test).
pub(crate) const LOGO: [&str; 5] = [
    " _   _  __ _(_) |_ ",
    "| | | |/ _` | | __|",
    "| |_| | (_| | | |_ ",
    " \\__,_|\\__, |_|\\__|",
    "       |___/       ",
];
const LOGO_GAP: u16 = 2;
const LABEL_W: usize = 8;
const VALUE_W: usize = 34;

/// The `label value` rows of the context column: Repos, Repo, Branch, Path,
/// and Filter while one is applied (five at most, the header height).
fn context_lines(app: &App) -> Vec<Line<'static>> {
    let th = &app.theme;
    let label = |s: &str| Span::styled(pad_w(s, LABEL_W), th.soft_dim());
    let strong = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);

    let total = app.store.repos.len();
    let mut changed = 0;
    let mut ahead = 0;
    for r in &app.store.repos {
        if let Some(s) = app.statuses.get(&r.path).and_then(|t| t.status.as_ref()) {
            changed += usize::from(!s.is_clean());
            ahead += usize::from(s.branch.ahead > 0);
        }
    }
    let repos = Line::from(vec![
        label("Repos"),
        Span::styled(total.to_string(), strong),
        Span::raw(format!(" · {changed} changed · {ahead} ahead")),
    ]);

    // The selected repository; a worktree shows `repo › worktree`.
    let folder = |p: &std::path::Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let repo_name = match (app.current_row(), app.current_target()) {
        (Some(Row::Worktree { repo, path, .. }), _) => app
            .store
            .repos
            .get(*repo)
            .map(|r| format!("{} › {}", r.name(), folder(path))),
        (_, Some(target)) => Some(folder(&target)),
        _ => None,
    };
    let repo = Line::from(vec![
        label("Repo"),
        match repo_name {
            Some(name) => Span::styled(truncate_w(&name, VALUE_W), strong),
            None => Span::styled("-", th.soft_dim()),
        },
    ]);

    let tree = app.current_tree();
    let status = tree.and_then(|t| t.status.as_ref());
    let branch = match status {
        Some(s) => {
            let head = s.branch.head.clone().unwrap_or_else(|| {
                format!("detached {}", s.branch.oid.clone().unwrap_or_default())
            });
            let mut spans = vec![
                label("Branch"),
                Span::styled(
                    truncate_w(&head, VALUE_W),
                    Style::default().fg(th.branch).add_modifier(Modifier::BOLD),
                ),
            ];
            match &s.branch.upstream {
                Some(up) => spans.push(Span::styled(
                    truncate_w(&format!(" → {up}"), VALUE_W.saturating_sub(head.width())),
                    th.soft_dim(),
                )),
                None if s.branch.head.is_some() => {
                    spans.push(Span::styled(" (no upstream)", th.soft_dim()))
                }
                None => {}
            }
            Line::from(spans)
        }
        None => Line::from(vec![label("Branch"), Span::styled("-", th.soft_dim())]),
    };

    let path = Line::from(vec![
        label("Path"),
        Span::raw(
            app.current_target()
                .map(|p| truncate_left_w(&collapse_home(&p), VALUE_W))
                .unwrap_or_else(|| "-".into()),
        ),
    ]);

    let mut lines = vec![repos, repo, branch, path];
    if let Some(filter) = app.filter_label() {
        lines.push(Line::from(vec![
            label("Filter"),
            Span::styled(truncate_w(&filter, VALUE_W), strong),
        ]));
    }
    lines
}

fn hint_lines(col: &[Hint], app: &App) -> (Vec<Line<'static>>, u16) {
    let th = &app.theme;
    let key_w = col
        .iter()
        .map(|(k, _)| format!("<{k}>").width())
        .max()
        .unwrap_or(0);
    let label_w = col.iter().map(|(_, l)| l.width()).max().unwrap_or(0);
    let lines = col
        .iter()
        .map(|(k, l)| {
            Line::from(vec![
                Span::styled(pad_w(&format!("<{k}>"), key_w), th.key_style()),
                Span::styled(format!(" {l}"), th.soft_dim()),
            ])
        })
        .collect();
    (lines, (key_w + 1 + label_w) as u16 + 4)
}

pub(super) fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    const LEFT_PAD: u16 = 1;
    let context_w = (LABEL_W + VALUE_W + 4) as u16;
    let [a, b] = app.pane_hints();
    let columns: Vec<(Vec<Line>, u16)> = [a, b, keys::common()]
        .iter()
        .map(|c| hint_lines(c, app))
        .collect();
    let logo_w = LOGO[0].width() as u16;
    let mut total = LEFT_PAD + context_w + columns.iter().map(|(_, w)| *w).sum::<u16>();
    let show_logo = area.width >= total + LOGO_GAP + logo_w;
    let (key_area, logo_area) = if show_logo {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(LOGO_GAP), Constraint::Length(logo_w)])
            .split(area);
        (cols[0], Some(cols[1]))
    } else {
        (area, None)
    };
    let mut visible = columns.len();
    while visible > 0 && total > key_area.width {
        visible -= 1;
        total -= columns[visible].1;
    }
    let mut constraints = vec![Constraint::Length(LEFT_PAD), Constraint::Length(context_w)];
    constraints.extend(
        columns[..visible]
            .iter()
            .map(|(_, w)| Constraint::Length(*w)),
    );
    constraints.push(Constraint::Min(0));
    let cells = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(key_area);
    f.render_widget(Paragraph::new(context_lines(app)), cells[1]);
    for (i, (lines, _)) in columns.into_iter().take(visible).enumerate() {
        f.render_widget(Paragraph::new(lines), cells[2 + i]);
    }
    if let Some(logo_area) = logo_area {
        let style = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);
        let lines: Vec<Line> = LOGO
            .iter()
            .map(|l| Line::from(Span::styled(*l, style)))
            .collect();
        f.render_widget(Paragraph::new(lines), logo_area);
    }
}
