//! File pane while a commit's files are open: the commit row, a `Files`
//! divider, then one row per file (status letter, path, `+N -N`).

use super::files::{pane_title, WORKTREE_TITLE};
use super::{draw_divider, empty_line, pane_block, pane_tone, selection_style, Divider, Tone};
use crate::git::commit_files::CommitFile;
use crate::theme::Theme;
use crate::ui::commit_files::CommitView;
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{truncate_left_w, truncate_w};
use crate::ui::{App, Pane};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub(super) fn draw_commit_files(f: &mut Frame, app: &App, area: Rect, view: &CommitView) {
    let th = &app.theme;
    let tone = pane_tone(app, Pane::Files);
    // The commit's node and subject, or the uncommitted changes' `○`. The
    // title stays the repository's name; row 0 names what is open.
    let (node, subject) = match view.commit() {
        Some(c) => ("●", c.subject.as_str()),
        None => ("○", WORKTREE_TITLE),
    };
    let block = pane_block(app, pane_title(app), tone, Pane::Files);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let page = inner.height as usize;
    app.file_page.set(page);
    let list_area = Rect {
        x: inner.x.saturating_sub(1),
        width: inner.width + 2,
        ..inner
    };
    let w = list_area.width.saturating_sub(1) as usize;

    let mut rows: Vec<Vec<Span>> = vec![vec![
        Span::styled(format!(" {node} "), Style::default().fg(th.branch)),
        Span::styled(truncate_w(subject, w.saturating_sub(3)), Style::default()),
    ]];
    // The divider row stays empty; its line is drawn over it below.
    rows.push(Vec::new());
    let note = match (&view.files, &view.error) {
        (_, Some(err)) => Some((err.as_str(), Style::default().fg(th.error))),
        (None, None) => Some(("Loading files…", Style::default().fg(th.dim))),
        (Some(files), None) if files.is_empty() => {
            Some(("No files changed", Style::default().fg(th.dim)))
        }
        _ => None,
    };
    if let Some((text, style)) = note {
        let below = Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        };
        empty_line(f, below, &truncate_w(text, inner.width as usize), style);
    }
    for file in view.files.iter().flatten() {
        rows.push(file_spans(file, w, th));
    }

    // Visual row of the selection: the divider sits between the commit row
    // and the files.
    let selected_row = if view.sel == 0 { 0 } else { view.sel + 1 };
    let total = rows.len();
    let items: Vec<ListItem> = rows
        .into_iter()
        .enumerate()
        .map(|(row, mut spans)| {
            if row == 1 {
                return ListItem::new(Line::default());
            }
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            // Pad through the right padding cell so the selection bar reaches
            // the border.
            spans.push(Span::raw(" ".repeat((w + 1).saturating_sub(used))));
            if row == selected_row {
                let sel = selection_style(tone, th);
                for s in &mut spans {
                    s.style = sel;
                }
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = app.commit_list.borrow_mut();
    state.select(Some(selected_row));
    f.render_stateful_widget(List::new(items), list_area, &mut state);
    let offset = state.offset();
    drop(state);
    if offset <= 1 && 1 < offset + page {
        let d = Divider {
            y: inner.y + (1 - offset) as u16,
            label: "Files".into(),
            style: th.soft_dim(),
        };
        draw_divider(f, area, &d, tone, th);
    }
    draw_vscrollbar(f, area, tone == Tone::Focused, offset, total, page, th);
}

/// ` M src/app.rs        +3 -1`: the status letter in its color, the
/// folder soft-dim, the counts right-aligned in the diff colors (`Bin` for
/// a binary file).
fn file_spans(file: &CommitFile, w: usize, th: &Theme) -> Vec<Span<'static>> {
    let code = match file.status {
        'A' => th.success,
        'D' | 'U' => th.error,
        'R' | 'C' => th.branch,
        '?' => th.untracked,
        _ => th.warning,
    };
    let mut counts: Vec<Span> = match file.counts {
        Some((a, d)) => [(a, '+', th.diff_add), (d, '-', th.diff_del)]
            .into_iter()
            .filter(|(n, ..)| *n > 0)
            .map(|(n, sign, color)| Span::styled(format!(" {sign}{n}"), Style::default().fg(color)))
            .collect(),
        None => vec![Span::styled(" Bin", th.soft_dim())],
    };
    let counts_w: usize = counts.iter().map(|s| s.content.width()).sum();
    let path = match &file.orig_path {
        Some(orig) => format!("{orig} → {}", file.path),
        None => file.path.clone(),
    };
    let path = truncate_left_w(&path, w.saturating_sub(4 + counts_w));
    let (dir, name) = match path.rfind('/') {
        Some(i) => (path[..=i].to_string(), path[i + 1..].to_string()),
        None => (String::new(), path),
    };
    let used = 3 + dir.width() + name.width();
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(file.status.to_string(), Style::default().fg(code)),
        Span::raw(" "),
        Span::styled(dir, th.soft_dim()),
        Span::styled(name, Style::default()),
        Span::raw(" ".repeat(w.saturating_sub(used + counts_w))),
    ];
    spans.append(&mut counts);
    spans
}

/// Diff pane title while the commit view is open: the file name, or the
/// commit's hash (`Uncommitted Changes`) and counts on row 0.
pub(super) fn diff_title(app: &App, view: &CommitView) -> String {
    if let Some(file) = view.selected() {
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        return format!(" {} ", truncate_w(name, 48));
    }
    let stats = app
        .diff
        .doc
        .as_ref()
        .map(|d| format!(" +{} -{}", d.added, d.removed))
        .unwrap_or_default();
    match view.commit() {
        Some(c) => format!(" {}{stats} ", c.hash),
        None => format!(" {WORKTREE_TITLE}{stats} "),
    }
}
