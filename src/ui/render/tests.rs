//! Rendering tests: layout at several widths, header, footer, overlays.

use super::{pane_layout, visible_panes, LOGO};
use crate::git::test_repo::TestRepo;
use crate::ui::test_support::*;
use crate::ui::{Level, Pane, UiMode};
use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

#[test]
fn logo_rows_share_one_width_and_fit_the_header() {
    let w = LOGO[0].width();
    assert!(LOGO.iter().all(|l| l.width() == w));
    assert!(LOGO.len() <= 5);
}

#[test]
fn two_panes_follow_the_focus_and_wide_screens_show_three() {
    assert_eq!(visible_panes(160), 3);
    assert_eq!(visible_panes(159), 2);
    assert_eq!(visible_panes(64), 2);
    assert_eq!(visible_panes(50), 1);
    let wide = Rect::new(0, 0, 200, 20);
    for focus in Pane::ALL {
        let panes: Vec<Pane> = pane_layout(wide, focus)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(panes, Pane::ALL, "{focus:?}");
    }
    assert_eq!(
        pane_layout(wide, Pane::Repos)
            .iter()
            .map(|(_, r)| r.width)
            .sum::<u16>(),
        200
    );
    for width in [100, 150] {
        let area = Rect::new(0, 0, width, 20);
        let shown = |focus| -> Vec<Pane> {
            pane_layout(area, focus)
                .into_iter()
                .map(|(p, _)| p)
                .collect()
        };
        assert_eq!(shown(Pane::Repos), [Pane::Repos, Pane::Files]);
        assert_eq!(shown(Pane::Files), [Pane::Files, Pane::Diff]);
        assert_eq!(shown(Pane::Diff), [Pane::Files, Pane::Diff]);
        let total: u16 = pane_layout(area, Pane::Repos)
            .iter()
            .map(|(_, r)| r.width)
            .sum();
        assert_eq!(total, width, "widths add up to the area");
    }
    let narrow = Rect::new(0, 0, 50, 20);
    assert_eq!(pane_layout(narrow, Pane::Files)[0].0, Pane::Files);
    assert_eq!(pane_layout(narrow, Pane::Diff).len(), 1);
}

#[test]
fn full_screen_shows_header_two_panes_and_one_footer_row() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    let rows = screen(&app, 150, 30);
    assert!(shows(&rows, "Repos[1]"));
    assert!(
        shows(&rows, "─ repo ─"),
        "the file pane is titled by the repository"
    );
    assert!(!shows(&rows, "─ a.txt ─"), "Repos focus hides the diff");
    app.focus = Pane::Files;
    let rows = screen(&app, 150, 30);
    assert!(
        !shows(&rows, "Repos[1]"),
        "Files focus hides the repositories"
    );
    assert!(
        shows(&rows, "─ a.txt ─"),
        "the diff is titled by the file name"
    );
    assert!(shows(&rows, "<ctrl+u> Refresh All"));
    assert!(
        shows(&screen(&app, 180, 30), "\\___/|_\\__,_|"),
        "logo shown when wide"
    );
    assert!(
        rows[29].contains("<1> Repos"),
        "footer falls back to pane keys"
    );
    // Exactly one row below the pane bottoms.
    let bottom = rows
        .iter()
        .rposition(|r| r.contains('┘') || r.contains('┛'))
        .unwrap();
    assert_eq!(bottom, 28);

    app.notify(Level::Ok, "done");
    assert!(screen(&app, 160, 30)[29].contains("[OK] done"));
}

#[test]
fn narrow_header_drops_the_logo_before_shortcut_columns() {
    let a = TestRepo::new();
    let app = scanned_app(store_of(&[&a.root]));
    let rows = screen(&app, 140, 24);
    assert!(!shows(&rows, "\\___/|_\\__,_|"));
    assert!(shows(&rows, "<:>"));
    let rows = screen(&app, 100, 24);
    assert!(!shows(&rows, "<:>"), "rightmost column goes first");
    assert!(shows(&rows, "</>"));
}

#[test]
fn tiny_terminals_get_a_notice_instead_of_panes() {
    let app = scanned_app(Default::default());
    let rows = screen(&app, 30, 8);
    assert!(rows[0].contains("Terminal too small"));
}

#[test]
fn empty_list_explains_how_to_add() {
    let app = scanned_app(Default::default());
    let rows = screen(&app, 120, 24);
    assert!(shows(&rows, "No repositories."), "{rows:#?}");
    assert!(shows(&rows, "add one."));
}

#[test]
fn help_fits_the_default_terminal_and_scrolls() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('?')]);
    let rows = screen(&app, 80, 24);
    assert!(shows(&rows, "Help — Repos"));
    assert!(
        app.help_max.get() > 0,
        "80x24 needs scrolling in one column"
    );
    app.press(&[key(KeyCode::End)]);
    let rows = screen(&app, 80, 24);
    assert!(shows(&rows, "Quit (press twice)") || shows(&rows, "Close the dropdown"));
    app.press(&[key(KeyCode::Esc)]);
    assert_eq!(app.mode, UiMode::Normal);
}

#[test]
fn every_dialog_renders_at_the_smallest_supported_size() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    for open in [
        vec![ch('+')],
        vec![ctrl('n')],
        vec![ctrl('e')],
        vec![ctrl('d')],
        vec![ch('c')],
        vec![ch(':')],
        vec![ch('?')],
    ] {
        app.press(&open);
        assert_ne!(app.mode, UiMode::Normal, "{open:?} opens a layer");
        for (w, h) in [(40, 10), (80, 24), (200, 60)] {
            let _ = screen(&app, w, h);
        }
        app.press(&[key(KeyCode::Esc)]);
        assert_eq!(app.mode, UiMode::Normal);
    }
}

#[test]
fn worktree_rows_are_indented_with_a_branch() {
    let a = TestRepo::new();
    let wt = a.root.parent().unwrap().join("repo-feat");
    a.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feat/x",
        wt.to_str().unwrap(),
    ]);
    let app = scanned_app(store_of(&[&a.root]));
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, "└ repo-feat feat/x"), "{rows:#?}");
}

#[test]
fn group_headers_show_changes_only_while_folded() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    a.write("b.txt", "new\n");
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    store.set_group(&a.root, Some("work".into()));
    let mut app = scanned_app(store);
    let header = |app: &crate::ui::App| {
        screen(app, 160, 20)
            .into_iter()
            .find(|r| r.contains("▾ work") || r.contains("▸ work"))
            .unwrap()
    };
    let open = header(&app);
    assert!(!open.contains('Ⓜ'), "{open}");
    assert!(!open.contains(" 1 "), "no repository count: {open}");
    app.press(&[key(KeyCode::Home), ch(' ')]);
    let folded = header(&app);
    assert!(folded.contains("Ⓜ 2"), "{folded}");
}

#[test]
fn the_selection_bar_reaches_the_right_border() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let app = scanned_app(store_of(&[&a.root]));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 20)).unwrap();
    terminal.draw(|f| crate::ui::render::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = Rect::new(0, 5, 160, 14);
    for (pane, rect) in pane_layout(area, app.focus).into_iter().take(2) {
        let y = rect.y + 1;
        // Left padding cell through the cell before the right border.
        for x in rect.x + 1..rect.right() - 1 {
            assert_eq!(
                buf[(x, y)].bg,
                if pane == Pane::Repos {
                    app.theme.selection_bg
                } else {
                    app.theme.selection_inactive_bg
                },
                "{pane:?} cell {x} of the selected row"
            );
        }
    }
}

#[test]
fn circled_badges_keep_a_space_before_their_count() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    a.git(&["stash", "-q"]);
    a.write("b.txt", "new\n");
    let app = scanned_app(store_of(&[&a.root]));
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, "Ⓜ 1 Ⓢ 1"), "{rows:#?}");
}

#[test]
fn confirm_dialogs_keep_their_blank_lines() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.confirm = Some(crate::ui::overlays::confirm::ConfirmDialog {
        action: crate::ui::overlays::confirm::ConfirmAction::RemoveRepo(a.root.clone()),
        title: " T ".into(),
        lines: vec!["first".into(), String::new(), "third".into()],
        ok_label: "Ok".into(),
        danger: false,
        ok_focused: false,
    });
    app.mode = UiMode::Confirm;
    let rows = screen(&app, 120, 30);
    let first = rows.iter().position(|r| r.contains("first")).unwrap();
    assert!(rows[first + 2].contains("third"), "{rows:#?}");
}

#[test]
fn the_header_names_the_selected_repository_above_its_branch() {
    let a = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    store.set_group(&a.root, Some("work".into()));
    let mut app = scanned_app(store);
    // The group header is selected: no repository.
    let rows = screen(&app, 150, 20);
    assert!(rows[1].starts_with(" Repo    -"), "{:?}", &rows[..4]);
    app.press(&[key(KeyCode::Down)]);
    let rows = screen(&app, 150, 20);
    assert!(rows[1].starts_with(" Repo    repo "), "{:?}", &rows[..4]);
    assert!(rows[2].starts_with(" Branch  main"), "{:?}", &rows[..4]);
    assert!(rows[3].starts_with(" Path"));
}

#[test]
fn a_dimmed_pane_selection_is_a_light_bar_with_plain_text() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    // Repos focus dims the file pane, whose first row (` M a.txt`) is selected.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 20)).unwrap();
    terminal.draw(|f| super::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16| (0..150).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let y = (0..20).find(|&y| row(y).contains(" M a.txt")).unwrap();
    let x = (0..150)
        .find(|&x| buf[(x, y)].symbol() == "a" && buf[(x + 1, y)].symbol() == ".")
        .unwrap();
    let cell = &buf[(x, y)];
    assert_eq!(cell.bg, app.theme.selection_inactive_bg);
    assert_eq!(cell.fg, app.theme.fg);
    assert!(!cell.modifier.contains(ratatui::style::Modifier::BOLD));
    assert!(!cell.modifier.contains(ratatui::style::Modifier::REVERSED));
}
