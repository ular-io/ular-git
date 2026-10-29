//! `?` help: a centered box listing what works in the focused pane and
//! everywhere. Two columns when wide, one when narrow, scrolling when tall.
//! The screen behind is not faded, so it stays clear where help was opened.

use crate::theme::Theme;
use crate::ui::components::list_nav::{self, ListMove};
use crate::ui::components::modal::{centered_fixed_rect, modal_block, render_modal};
use crate::ui::components::scrollbar::draw_vscrollbar;
use crate::ui::components::text::{pad_w, truncate_w};
use crate::ui::keys::{self, Hint};
use crate::ui::{App, UiMode};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

impl App {
    pub(crate) fn open_help(&mut self) {
        self.help_scroll.set(0);
        self.mode = UiMode::Help;
    }

    pub(crate) fn on_key_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => self.mode = UiMode::Normal,
            _ => {
                if let Some(mv) = list_nav::list_move(&key, true) {
                    let max = self.help_max.get();
                    let cur = self.help_scroll.get();
                    let next = match mv {
                        ListMove::Up => cur.saturating_sub(1),
                        ListMove::Down => (cur + 1).min(max),
                        ListMove::PageUp => cur.saturating_sub(10),
                        ListMove::PageDown => (cur + 10).min(max),
                        ListMove::Home => 0,
                        ListMove::End => max,
                    };
                    self.help_scroll.set(next);
                }
            }
        }
    }
}

fn section_lines(sections: &[(&str, Vec<Hint>)], width: usize, th: &Theme) -> Vec<Line<'static>> {
    let key_w = sections
        .iter()
        .flat_map(|(_, items)| items.iter())
        .map(|(k, _)| format!("<{k}>").width())
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    for (i, (title, items)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            title.to_string(),
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        )));
        for (key, label) in items {
            let label_w = width.saturating_sub(key_w + 2);
            lines.push(Line::from(vec![
                Span::styled(pad_w(&format!("<{key}>"), key_w), th.key_style()),
                Span::raw("  "),
                Span::styled(truncate_w(label, label_w), th.soft_dim()),
            ]));
        }
    }
    lines
}

pub(crate) fn draw_help(f: &mut Frame, app: &App) {
    let th = &app.theme;
    let full = f.area();
    let mut here = Vec::new();
    let [a, b] = app.pane_hints();
    here.extend(a);
    here.extend(b);
    here.extend(app.pane_help_only());
    let left = vec![(app.focus.title(), here)];
    let right = vec![("Anywhere", keys::anywhere()), ("Dialogs", keys::dialog())];

    let w = 120.min(full.width);
    let two_cols = w >= 84;
    let inner_w = w.saturating_sub(6) as usize;
    let col_w = if two_cols { (inner_w - 2) / 2 } else { inner_w };
    let left_lines = section_lines(&left, col_w, th);
    let right_lines = section_lines(&right, col_w, th);
    let content_h = if two_cols {
        left_lines.len().max(right_lines.len())
    } else {
        left_lines.len() + 1 + right_lines.len()
    };
    // Borders (2) + padding top/bottom (1 each).
    let h = (content_h as u16 + 4).min(full.height);
    let area = centered_fixed_rect(w, h, full);
    let title = format!(" Help — {} ", app.focus.title());
    let block = modal_block(&title, th.accent).padding(Padding::new(1, 1, 1, 1));
    let inner = render_modal(f, area, block, th);
    let view = inner.height as usize;
    let max = content_h.saturating_sub(view);
    app.help_max.set(max);
    let scroll = app.help_scroll.get().min(max);
    app.help_scroll.set(scroll);

    if two_cols {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(col_w as u16),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(inner);
        for (lines, rect) in [(left_lines, cols[0]), (right_lines, cols[2])] {
            f.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), rect);
        }
    } else {
        let mut lines = left_lines;
        lines.push(Line::from(""));
        lines.extend(right_lines);
        f.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);
    }
    if max > 0 {
        let sb = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
        draw_vscrollbar(f, sb, true, scroll, content_h, view, th);
    }
}
