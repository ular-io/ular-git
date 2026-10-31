//! `1` / `2` / `3`: how many panes are shown side by side.

use super::pane_layout;
use crate::git::test_repo::TestRepo;
use crate::ui::test_support::*;
use crate::ui::Pane;
use ratatui::layout::Rect;

fn shown(w: u16, focus: Pane, count: usize) -> Vec<Pane> {
    pane_layout(Rect::new(0, 0, w, 20), focus, count, u16::MAX)
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

#[test]
fn a_count_shows_the_focused_pane_and_the_ones_to_its_right() {
    use Pane::*;
    for w in [100, 200] {
        for focus in Pane::ALL {
            assert_eq!(shown(w, focus, 1), [focus], "{w} {focus:?}");
        }
        assert_eq!(shown(w, Repos, 2), [Repos, Files]);
        assert_eq!(shown(w, Files, 2), [Files, Diff]);
        assert_eq!(shown(w, Diff, 2), [Files, Diff], "Diff stays on the right");
    }
    for focus in Pane::ALL {
        assert_eq!(shown(200, focus, 3), Pane::ALL);
        // The width caps the count: two panes below 160, one below 64.
        assert_eq!(shown(100, focus, 3), shown(100, focus, 2));
        assert_eq!(shown(50, focus, 3), [focus]);
    }
    let total = |count| -> u16 {
        pane_layout(Rect::new(0, 0, 200, 20), Diff, count, u16::MAX)
            .iter()
            .map(|(_, r)| r.width)
            .sum()
    };
    assert!((1..=3).all(|count| total(count) == 200));
}

#[test]
fn number_keys_apply_only_the_counts_the_width_can_show() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    screen(&app, 170, 24);
    app.press(&[ch('1')]);
    assert_eq!(app.pane_count, 1);
    let rows = screen(&app, 170, 24);
    assert!(
        rows[5].starts_with("┏") && rows[5].ends_with("┓"),
        "{}",
        rows[5]
    );
    // The count stays while the focus moves.
    app.press(&[ch('l')]);
    assert_eq!((app.focus, app.pane_count), (Pane::Files, 1));
    app.press(&[ch('3')]);
    assert_eq!(app.pane_count, 3);
    // Two panes wide: `3` does nothing, `1` and `2` apply.
    screen(&app, 100, 24);
    app.press(&[ch('2'), ch('3')]);
    assert_eq!(app.pane_count, 2);
    app.press(&[ch('1')]);
    assert_eq!(app.pane_count, 1);
    // One pane wide: no number key applies.
    screen(&app, 50, 24);
    app.press(&[ch('2'), ch('3')]);
    assert_eq!(app.pane_count, 1);
}

#[test]
fn backslash_cycles_the_counts_the_width_can_show() {
    let mut app = scanned_app(Default::default());
    let cycle = |app: &mut crate::ui::App, w: u16| {
        screen(app, w, 24);
        app.press(&[ch('\\')]);
        app.pane_count
    };
    assert_eq!(app.pane_count, 3);
    assert_eq!(cycle(&mut app, 170), 1, "3 wraps to 1");
    assert_eq!(cycle(&mut app, 170), 2);
    assert_eq!(cycle(&mut app, 170), 3);
    // Two panes wide, 3 shows as 2: the next is 1, then back to 2.
    assert_eq!(cycle(&mut app, 100), 1);
    assert_eq!(cycle(&mut app, 100), 2);
    assert_eq!(cycle(&mut app, 100), 1);
    // One pane wide: nothing changes.
    assert_eq!(cycle(&mut app, 50), 1);
}
