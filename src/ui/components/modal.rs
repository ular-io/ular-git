//! Modal framing and backdrop primitives shared by every dialog: the thick
//! titled block, the clear-and-repaint renderer, navigation-arrow blocks, form
//! input boxes, closed combo boxes, dropdown frames, button styles, and the
//! behind-dialog backdrop fade.

use crate::theme::Theme;
use crate::ui::components::input::input_view;
use crate::ui::components::text::truncate_w;
use crate::ui::{TextInput, UiMode};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

pub(crate) fn modal_block(title: &str, color: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .padding(Padding::new(1, 1, 1, 1))
}

/// Dialog modes that fade the screen behind them. ThemeSelect is excluded because
/// the backdrop IS the live theme preview; Help keeps the screen readable so the
/// user sees where they came from; Normal/Filter are not dialogs.
pub(crate) fn backdrop_dimmed(mode: UiMode) -> bool {
    !matches!(
        mode,
        UiMode::Normal | UiMode::Filter | UiMode::ThemeSelect | UiMode::Help
    )
}

/// Percentage each backdrop cell moves toward the theme background while a dialog is open.
const BACKDROP_FADE_PCT: u32 = 50;

/// Fades every rendered cell toward the theme background so the dialog painted
/// afterwards stands out (dialogs repaint their own area via `render_modal`).
/// `Color::Reset` backgrounds are terminal-owned and can't be blended, so they
/// stay untouched and only foregrounds fade, using the dark-flag assumption —
/// same fallback as pulse fades (`Theme::bg_rgb`).
pub(crate) fn dim_backdrop(f: &mut Frame, th: &Theme) {
    let target = th.bg_rgb();
    let reset_fg = if th.dark {
        (235, 235, 235)
    } else {
        (16, 16, 16)
    };
    let area = f.area();
    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            let fg = match cell.fg {
                Color::Reset => reset_fg,
                c => crate::theme::color_rgb(c),
            };
            cell.fg = fade_toward(fg, target);
            if cell.bg != Color::Reset {
                cell.bg = fade_toward(crate::theme::color_rgb(cell.bg), target);
            }
        }
    }
}

/// Linear blend of `c` toward `target` by `BACKDROP_FADE_PCT` percent.
pub(crate) fn fade_toward((r, g, b): (u8, u8, u8), (tr, tg, tb): (u8, u8, u8)) -> Color {
    let mix = |c: u8, t: u8| -> u8 {
        ((u32::from(c) * (100 - BACKDROP_FADE_PCT) + u32::from(t) * BACKDROP_FADE_PCT) / 100) as u8
    };
    Color::Rgb(mix(r, tr), mix(g, tg), mix(b, tb))
}

/// Shared modal renderer: Clears the outer bounds and draws the Block inset by 1 margin cell laterally.
/// This margin absorbs overlapping double-width characters from the background behind,
/// preventing frame borders from getting clipped. Returns the inner content Rect.
/// `Clear` resets cells to the terminal default, so the theme base is repainted under the modal.
pub(crate) fn render_modal(f: &mut Frame, outer: Rect, block: Block<'static>, th: &Theme) -> Rect {
    f.render_widget(Clear, outer);
    f.render_widget(Block::default().style(th.base_style()), outer);
    let block_area = Rect {
        x: outer.x + 1,
        y: outer.y,
        width: outer.width.saturating_sub(2),
        height: outer.height,
    };
    let inner = block.inner(block_area);
    f.render_widget(block, block_area);
    inner
}

/// Single-line form input box titled with its label (three rows with borders).
/// Focused: Thick accent border and the hardware cursor; unfocused: a Plain `dim`
/// border, lighter than a panel because it sits inside a dialog. A whole-value
/// selection paints only the text, and only while focused. `placeholder` fills an
/// empty box only while it is not focused, because the cursor sits where it would
/// start.
pub(crate) fn form_input(
    f: &mut Frame,
    area: Rect,
    label: &str,
    input: &TextInput,
    focused: bool,
    placeholder: &str,
    th: &Theme,
) {
    let (border_type, style) = if focused {
        (
            BorderType::Thick,
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        )
    } else {
        (BorderType::Plain, Style::default().fg(th.dim))
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(style)
        .title(Span::styled(label.to_string(), style))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let (visible, cursor_x) = input_view(input, width);
    let value = if input.value.is_empty() && !focused {
        Span::styled(truncate_w(placeholder, width), th.soft_dim())
    } else if focused && input.select_all {
        Span::styled(
            visible,
            Style::default().fg(th.selection_fg).bg(th.selection_bg),
        )
    } else {
        Span::raw(visible)
    };
    f.render_widget(Paragraph::new(Line::from(value)), inner);
    if focused {
        f.set_cursor_position((inner.x.saturating_add(cursor_x), inner.y));
    }
}

/// Closed combo box (three rows with borders): the box framed like `form_input`
/// (`Thick` accent + bold while focused or open, `Plain` `dim` otherwise),
/// titled with the name alone, and a `▾` at the inner right end in the border's
/// style. Returns the value area — the inner width minus the `▾` and the one
/// gap cell before it — so values, right-aligned notes and an input cursor
/// never cover the arrow. Below three inner cells the arrow is dropped and the
/// whole inner area is returned.
pub(crate) fn draw_combo(
    f: &mut Frame,
    area: Rect,
    title: &str,
    focused: bool,
    th: &Theme,
) -> Rect {
    let (border_type, style) = if focused {
        (
            BorderType::Thick,
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        )
    } else {
        (BorderType::Plain, Style::default().fg(th.dim))
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(style)
        .title(Span::styled(format!(" {title} "), style))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 3 || inner.height == 0 {
        return inner;
    }
    let arrow = Rect {
        x: inner.right() - 1,
        width: 1,
        height: 1,
        ..inner
    };
    f.render_widget(Paragraph::new(Span::styled("▾", style)), arrow);
    Rect {
        width: inner.width - 2,
        ..inner
    }
}

/// Clears and frames a dropdown popup that overlaps the bottom border of its
/// combo box: a thick `border` frame whose top edge is redrawn as `┣━┫` to join
/// the combo. Returns the inner area. `dialog` is the dialog the combo sits in:
/// one extra column left of the popup is cleared only on rows below it, where a
/// backdrop double-width glyph could bleed into the border, so the dialog's own
/// bottom border keeps its `━` beside the popup.
pub(crate) fn dropdown_frame(
    f: &mut Frame,
    popup: Rect,
    dialog: Rect,
    border: Style,
    th: &Theme,
) -> Rect {
    f.render_widget(Clear, popup);
    f.render_widget(Block::default().style(th.base_style()), popup);
    let below_dialog = dialog.bottom().max(popup.y);
    if popup.x > 0 && popup.bottom() > below_dialog {
        let margin = Rect::new(popup.x - 1, below_dialog, 1, popup.bottom() - below_dialog);
        f.render_widget(Clear, margin);
        f.render_widget(Block::default().style(th.base_style()), margin);
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(border);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let width = popup.width as usize;
    let join = if width > 2 {
        format!("┣{}┫", "━".repeat(width - 2))
    } else {
        "━━".to_string()
    };
    f.render_widget(
        Paragraph::new(Span::styled(join, border)),
        Rect { height: 1, ..popup },
    );
    inner
}

/// Dialog button styles: `(focused, unfocused)`.
pub(crate) fn button_styles(th: &Theme) -> (Style, Style) {
    (
        Style::default()
            .fg(th.button_focus_fg)
            .bg(th.button_focus_bg)
            .add_modifier(Modifier::BOLD),
        Style::default().fg(th.button_fg).bg(th.button_bg),
    )
}

/// Centered `[ok]     [cancel]` button row (action left, cancel right).
/// `focus`: `Some(true)` = action, `Some(false)` = cancel, `None` = the row
/// does not have focus and neither button is highlighted.
pub(crate) fn button_row(
    ok: &str,
    cancel: &str,
    focus: Option<bool>,
    th: &Theme,
) -> Paragraph<'static> {
    let (on, off) = button_styles(th);
    let (ok_style, cancel_style) = match focus {
        Some(true) => (on, off),
        Some(false) => (off, on),
        None => (off, off),
    };
    Paragraph::new(Line::from(vec![
        Span::styled(format!("  {ok}  "), ok_style),
        Span::raw("     "),
        Span::styled(format!("  {cancel}  "), cancel_style),
    ]))
    .alignment(Alignment::Center)
}

/// Fixed-size rectangle centered in `area` (clamped to it).
pub(crate) fn centered_fixed_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Dialog width: `want`, but at most 90% of the screen.
pub(crate) fn dialog_width(want: u16, area: Rect) -> u16 {
    want.min(area.width.saturating_mul(9) / 10)
        .max(20.min(area.width))
}

#[cfg(test)]
mod tests {
    use super::{backdrop_dimmed, fade_toward};
    use crate::ui::UiMode;
    use ratatui::style::Color;

    #[test]
    fn backdrop_dim_applies_to_dialog_modes_only() {
        for mode in [
            UiMode::Normal,
            UiMode::Filter,
            UiMode::ThemeSelect,
            UiMode::Help,
        ] {
            assert!(!backdrop_dimmed(mode), "{mode:?} must not dim");
        }
        for mode in [
            UiMode::AddRepo,
            UiMode::GroupName,
            UiMode::GroupPick,
            UiMode::Confirm,
            UiMode::Commit,
            UiMode::QuickCommand,
            UiMode::Message,
        ] {
            assert!(backdrop_dimmed(mode), "{mode:?} must dim");
        }
    }

    /// Draws one combo into a `width`×3 buffer; returns the buffer and value area.
    fn combo(width: u16, focused: bool) -> (ratatui::buffer::Buffer, ratatui::layout::Rect) {
        let th = crate::theme::default_theme();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 3)).unwrap();
        let mut value = ratatui::layout::Rect::default();
        terminal
            .draw(|f| value = super::draw_combo(f, f.area(), "Profile", focused, &th))
            .unwrap();
        (terminal.backend().buffer().clone(), value)
    }

    fn row(buf: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    #[test]
    fn combo_titles_the_name_and_puts_the_arrow_at_the_inner_right_end() {
        let (buf, value) = combo(20, false);
        assert_eq!(row(&buf, 0), "┌ Profile ─────────┐");
        assert_eq!(row(&buf, 1), "│                ▾ │");
        // Border, padding, value area, gap, arrow, padding, border.
        assert_eq!((value.x, value.y, value.width, value.height), (2, 1, 14, 1));
        assert_eq!(buf[(17, 1)].symbol(), "▾");
    }

    #[test]
    fn combo_arrow_follows_the_border_style() {
        use ratatui::style::Modifier;
        let th = crate::theme::default_theme();
        let (buf, _) = combo(20, true);
        assert_eq!(row(&buf, 1), "┃                ▾ ┃");
        for cell in [&buf[(17, 1)], &buf[(0, 1)]] {
            assert_eq!(cell.fg, th.accent);
            assert!(cell.modifier.contains(Modifier::BOLD));
        }
        let (buf, _) = combo(20, false);
        for cell in [&buf[(17, 1)], &buf[(0, 1)]] {
            assert_eq!(cell.fg, th.dim);
            assert!(!cell.modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn combo_drops_the_arrow_below_three_inner_cells() {
        let (buf, value) = combo(6, false);
        assert!(!row(&buf, 1).contains('▾'));
        assert_eq!(value.width, 2);
        let (buf, value) = combo(7, false);
        assert_eq!(row(&buf, 1), "│   ▾ │");
        assert_eq!(value.width, 1);
    }

    #[test]
    fn fade_toward_moves_halfway_to_target() {
        assert_eq!(
            fade_toward((100, 100, 100), (0, 0, 0)),
            Color::Rgb(50, 50, 50)
        );
        assert_eq!(
            fade_toward((0, 0, 0), (200, 100, 50)),
            Color::Rgb(100, 50, 25)
        );
    }
}
