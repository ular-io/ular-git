//! The one list-movement contract every list shares: `↑`/`↓` (`k`/`j`),
//! `pageup`/`pagedown`, and `home`/`end` (`g`/`G`). Ends clamp; nothing wraps.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMove {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// Reads a list movement from `key`. `letters` enables the vim letters, which
/// lists that take typed text leave to the input.
pub fn list_move(key: &KeyEvent, letters: bool) -> Option<ListMove> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    Some(match key.code {
        KeyCode::Up => ListMove::Up,
        KeyCode::Down => ListMove::Down,
        KeyCode::PageUp => ListMove::PageUp,
        KeyCode::PageDown => ListMove::PageDown,
        KeyCode::Home => ListMove::Home,
        KeyCode::End => ListMove::End,
        KeyCode::Char('k') if letters => ListMove::Up,
        KeyCode::Char('j') if letters => ListMove::Down,
        KeyCode::Char('g') if letters => ListMove::Home,
        KeyCode::Char('G') if letters => ListMove::End,
        _ => return None,
    })
}

/// New index after `mv` in a list of `len` items (`page` = visible rows).
/// An empty list stays at 0.
pub fn apply(cur: usize, len: usize, mv: ListMove, page: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let step = page.saturating_sub(1).max(1);
    match mv {
        ListMove::Up => cur.saturating_sub(1),
        ListMove::Down => (cur + 1).min(last),
        ListMove::PageUp => cur.saturating_sub(step),
        ListMove::PageDown => (cur + step).min(last),
        ListMove::Home => 0,
        ListMove::End => last,
    }
    .min(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn ends_clamp_and_empty_lists_stay_put() {
        assert_eq!(apply(0, 3, ListMove::Up, 10), 0);
        assert_eq!(apply(2, 3, ListMove::Down, 10), 2);
        assert_eq!(apply(1, 3, ListMove::End, 10), 2);
        assert_eq!(apply(2, 3, ListMove::Home, 10), 0);
        assert_eq!(apply(0, 0, ListMove::End, 10), 0);
        assert_eq!(apply(0, 100, ListMove::PageDown, 10), 9);
    }

    #[test]
    fn letters_only_when_allowed() {
        assert_eq!(
            list_move(&key(KeyCode::Char('G')), true),
            Some(ListMove::End)
        );
        assert_eq!(list_move(&key(KeyCode::Char('G')), false), None);
        assert_eq!(list_move(&key(KeyCode::Home), false), Some(ListMove::Home));
        let ctrl = KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL);
        assert_eq!(list_move(&ctrl, true), None);
    }
}
