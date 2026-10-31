//! Panes hidden by the user: Repos by `←`, everything but the diff by `→`.

use super::{pane_layout, Hidden};
use crate::git::test_repo::TestRepo;
use crate::ui::test_support::*;
use crate::ui::Pane;
use crossterm::event::KeyCode;
use ratatui::layout::Rect;

#[test]
fn hiding_repos_gives_its_width_to_files_on_wide_screens() {
    let area = Rect::new(0, 0, 170, 20);
    let widths = |hidden| -> Vec<(Pane, u16)> {
        pane_layout(area, Pane::Files, hidden, u16::MAX)
            .iter()
            .map(|(p, r)| (*p, r.width))
            .collect()
    };
    assert_eq!(
        widths(Hidden::None),
        [(Pane::Repos, 37), (Pane::Files, 47), (Pane::Diff, 86)]
    );
    assert_eq!(
        widths(Hidden::Repos),
        [(Pane::Files, 37 + 47), (Pane::Diff, 86)],
        "Diff keeps its width"
    );
    // Narrower, Files takes the place of Repos + Files and stands alone; the
    // diff is alone either way.
    for w in [100, 50] {
        let area = Rect::new(0, 0, w, 20);
        assert_eq!(
            pane_layout(area, Pane::Files, Hidden::Repos, u16::MAX),
            [(Pane::Files, area)],
            "{w}"
        );
        assert_eq!(
            pane_layout(area, Pane::Diff, Hidden::Repos, u16::MAX),
            pane_layout(area, Pane::Diff, Hidden::None, u16::MAX),
            "{w}"
        );
    }
}

#[test]
fn left_on_repos_hides_it_until_left_on_files_brings_it_back() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    let rows = screen(&app, 170, 24);
    assert!(rows[5].starts_with("┏━ ←"), "Repos offers ←: {}", rows[5]);
    app.press(&[key(KeyCode::Left)]);
    assert_eq!(app.focus, Pane::Files);
    assert!(!shows(&screen(&app, 170, 24), "Repos[1]"));
    // Moving between Files and Diff keeps Repos hidden.
    app.press(&[key(KeyCode::Right), key(KeyCode::Left)]);
    assert!(app.focus == Pane::Files && app.repos_hidden);
    assert!(!shows(&screen(&app, 170, 24), "Repos[1]"));
    app.press(&[ch('h')]);
    assert_eq!(app.focus, Pane::Repos);
    assert!(shows(&screen(&app, 170, 24), "Repos[1]"));
    // Below 160 columns Files is left alone, at the full width.
    screen(&app, 100, 24);
    app.press(&[key(KeyCode::Left)]);
    assert_eq!(app.focus, Pane::Files);
    let rows = screen(&app, 100, 24);
    assert!(
        rows[5].starts_with("┏") && rows[5].ends_with("┓"),
        "{}",
        rows[5]
    );
    assert_eq!(
        rows[5].matches('┐').count(),
        0,
        "no other pane: {}",
        rows[5]
    );
}

#[test]
fn left_on_a_group_header_still_folds_it() {
    let a = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    store.set_group(&a.root, Some("work".into()));
    let mut app = scanned_app(store);
    let rows = screen(&app, 170, 24);
    assert!(!rows[5].contains("←"), "{}", rows[5]);
    app.press(&[key(KeyCode::Left), key(KeyCode::Left)]);
    assert!(app.focus == Pane::Repos && !app.repos_hidden);
    assert_eq!(app.rows.len(), 1, "folded");
}
