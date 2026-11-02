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
        let panes: Vec<Pane> = pane_layout(wide, focus, 3, u16::MAX)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(panes, Pane::ALL, "{focus:?}");
    }
    assert_eq!(
        pane_layout(wide, Pane::Repos, 3, u16::MAX)
            .iter()
            .map(|(_, r)| r.width)
            .sum::<u16>(),
        200
    );
    for width in [100, 150] {
        let area = Rect::new(0, 0, width, 20);
        let shown = |focus| -> Vec<Pane> {
            pane_layout(area, focus, 3, u16::MAX)
                .into_iter()
                .map(|(p, _)| p)
                .collect()
        };
        assert_eq!(shown(Pane::Repos), [Pane::Repos, Pane::Files]);
        assert_eq!(shown(Pane::Files), [Pane::Files, Pane::Diff]);
        assert_eq!(
            shown(Pane::Diff),
            [Pane::Files, Pane::Diff],
            "Diff on the right"
        );
        for focus in [Pane::Repos, Pane::Diff] {
            let total: u16 = pane_layout(area, focus, 3, u16::MAX)
                .iter()
                .map(|(_, r)| r.width)
                .sum();
            assert_eq!(total, width, "widths add up to the area");
        }
    }
    let narrow = Rect::new(0, 0, 50, 20);
    assert_eq!(
        pane_layout(narrow, Pane::Files, 3, u16::MAX)[0].0,
        Pane::Files
    );
    assert_eq!(pane_layout(narrow, Pane::Diff, 3, u16::MAX).len(), 1);
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
    assert!(
        !shows(&rows, "─ a.txt +1 -3 ─"),
        "Repos focus hides the diff"
    );
    app.focus = Pane::Files;
    let rows = screen(&app, 150, 30);
    assert!(
        !shows(&rows, "Repos[1]"),
        "Files focus hides the repositories"
    );
    assert!(
        shows(&rows, "─ a.txt +1 -3 ─"),
        "the diff is titled by the file name and its counts"
    );
    assert!(shows(&rows, "<ctrl+u> Refresh All"));
    assert!(
        shows(&screen(&app, 180, 30), "\\___/|_\\__,_|"),
        "logo shown when wide"
    );
    assert!(
        rows[29].trim().is_empty(),
        "no result, no footer text: {}",
        rows[29]
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
    app.press(&[key(KeyCode::Home), key(KeyCode::Left)]);
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
    for (pane, rect) in pane_layout(
        area,
        app.focus,
        3,
        super::repos::fit_width(&app.store, &app.statuses),
    )
    .into_iter()
    .take(2)
    {
        let y = rect.y + 1;
        // Left padding cell through the cell before the right border.
        for x in rect.x + 1..rect.right() - 1 {
            assert_eq!(
                buf[(x, y)].bg,
                if pane == Pane::Repos {
                    app.theme.selection_bg
                } else {
                    super::selection_style(super::Tone::Dimmed, &app.theme)
                        .bg
                        .unwrap()
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
fn a_dimmed_pane_selection_is_plain_text_on_a_faded_muted_bar() {
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
    let th = &app.theme;
    let (r, g, b) = super::history_row::mix(crate::theme::color_rgb(th.muted), th.bg_rgb(), 40);
    assert_eq!(cell.bg, ratatui::style::Color::Rgb(r, g, b));
    assert_eq!(cell.fg, th.fg);
    assert!(!cell.modifier.contains(ratatui::style::Modifier::BOLD));
    assert!(!cell.modifier.contains(ratatui::style::Modifier::REVERSED));
}

#[test]
fn the_file_pane_grows_to_eighty_columns() {
    let width = |w: u16, focus: Pane| {
        pane_layout(Rect::new(0, 0, w, 20), focus, 3, u16::MAX)
            .into_iter()
            .find(|(p, _)| *p == Pane::Files)
            .map(|(_, r)| r.width)
            .unwrap()
    };
    assert_eq!(width(200, Pane::Repos), 64, "32 % of 200");
    assert_eq!(width(300, Pane::Repos), 80, "capped at 80");
    assert_eq!(width(150, Pane::Files), 60, "40 % beside the diff");
}

#[test]
fn the_repository_pane_fits_its_longest_name_within_its_range() {
    use super::repos::fit_width;
    use crate::store::RepoStore;
    use std::collections::HashMap;
    use std::path::PathBuf;
    let store_with = |names: &[&str], group: Option<&str>| {
        let mut store = RepoStore::default();
        if let Some(g) = group {
            store.add_group(g).unwrap();
        }
        for n in names {
            store.add_repo(PathBuf::from("/code").join(n), group.map(Into::into));
        }
        store
    };
    let fit = |store: &RepoStore| fit_width(store, &HashMap::new());
    let repos_w = |w: u16, store: &RepoStore| {
        pane_layout(Rect::new(0, 0, w, 20), Pane::Repos, 3, fit(store))[0]
            .1
            .width
    };
    // Indent 1, the name, and 11 cells of badges, padding, and borders.
    let name25 = "a".repeat(25);
    let mid = store_with(&[&name25, "short"], None);
    assert_eq!(fit(&mid), 37);
    assert_eq!(repos_w(200, &mid), 37, "below the 44 cap");
    assert_eq!(repos_w(160, &mid), 35, "still 22 % at most");
    assert_eq!(repos_w(100, &mid), 37, "beside the file pane");
    let short = store_with(&["a"], None);
    assert_eq!(repos_w(200, &short), 30, "the three-pane minimum");
    assert_eq!(repos_w(100, &short), 28, "the two-pane minimum");
    let long = store_with(&[&"a".repeat(60)], None);
    assert_eq!(repos_w(200, &long), 44, "long names are cut");
    // Grouped rows are indented by three; the group header counts too.
    assert_eq!(fit(&store_with(&[&name25], Some("g"))), 39);
    assert_eq!(fit(&store_with(&["a"], Some(&name25))), 39);
    // The fitted pane shows the whole name before its badge.
    let mut app = crate::ui::App::with_store(crate::config::Config::default(), mid, None);
    app.rebuild_rows();
    let rows = screen(&app, 200, 20);
    // The first row, below the header and the top border.
    let cut: String = rows[6].chars().take(37).collect();
    assert!(cut.contains(&name25) && cut.contains('?'), "{cut}");
}

#[test]
fn a_selected_group_centers_its_message_on_a_small_fill() {
    let a = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    store.set_group(&a.root, Some("work".into()));
    let mut app = scanned_app(store);
    app.press(&[key(KeyCode::Home)]);
    let (w, h) = (150u16, 30u16);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| super::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let fit = super::repos::fit_width(&app.store, &app.statuses);
    let files = pane_layout(Rect::new(0, 5, w, h - 6), Pane::Repos, 3, fit)[1].1;
    let y = (0..h)
        .find(|&y| row(y).contains("Select a repository"))
        .unwrap();
    let mid = files.y + files.height / 2;
    assert!(y.abs_diff(mid) <= 1, "vertically centered: {y} vs {mid}");
    // Byte offsets in `row` are off by the multi-byte frame glyphs; find the
    // cell instead.
    let start = (0..w)
        .find(|&x| buf[(x, y)].symbol() == "S" && buf[(x + 1, y)].symbol() == "e")
        .unwrap();
    let end = start + "Select a repository".len() as u16;
    let (left, right) = (start - files.x, files.right() - end);
    assert!(
        left.abs_diff(right) <= 1,
        "horizontally centered: {left} {right}"
    );
    // The fill reaches two rows above and below and eight cells to each side.
    let bg = |x: u16, y: u16| buf[(x, y)].bg == app.theme.group_bg;
    assert!(bg(start, y) && bg(end - 1, y));
    assert!(bg(start - 8, y - 2) && bg(end + 7, y + 2));
    assert!(!bg(start - 9, y) && !bg(end + 8, y));
    assert!(!bg(start, y - 3) && !bg(start, y + 3));
    assert!(
        !bg(files.x + 2, files.y + 2),
        "the rest of the pane keeps its color"
    );
    // A repository selected: no fill.
    app.press(&[key(KeyCode::Down)]);
    terminal.draw(|f| super::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    assert_ne!(buf[(start, y)].bg, app.theme.group_bg);
}

#[test]
fn the_file_pane_bottom_border_counts_the_history() {
    let a = TestRepo::new();
    for i in 0..2 {
        a.write("a.txt", &format!("{i}\n"));
        a.git(&["add", "-A"]);
        a.git(&["commit", "-qm", &format!("c{i}")]);
    }
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    assert_eq!(app.history.commits, Some(3));
    // A changed file: the count alone, on the bottom border.
    let rows = screen(&app, 150, 30);
    let bottom = rows.iter().rev().find(|r| r.contains('┛')).unwrap();
    assert!(bottom.contains("━ 3 commits ━┛"), "{bottom}");
    // Items: the file, Uncommitted Changes, then c1, c0, init.
    app.file_sel = 3;
    let rows = screen(&app, 150, 30);
    let bottom = rows.iter().rev().find(|r| r.contains('┛')).unwrap();
    assert!(bottom.contains("━ 2 / 3 commits ━┛"), "{bottom}");
    // Narrow panes drop the unit, then the label.
    assert_eq!(
        super::files::commits_label(&app, 22).as_deref(),
        Some(" 2 / 3 commits ")
    );
    assert_eq!(
        super::files::commits_label(&app, 21).as_deref(),
        Some(" 2 / 3 ")
    );
    assert_eq!(super::files::commits_label(&app, 13), None);
}

#[test]
fn a_group_header_keeps_its_band_when_the_repository_pane_loses_focus() {
    let a = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    store.set_group(&a.root, Some("work".into()));
    let mut app = scanned_app(store);
    // The cursor on the repository, so the header is not on the selection bar.
    app.press(&[key(KeyCode::Home), key(KeyCode::Down)]);
    app.focus = crate::ui::Pane::Files;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 20)).unwrap();
    terminal.draw(|f| super::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16| (0..200).map(|x| buf[(x, y)].symbol()).collect::<String>();
    // Three panes at 200 columns, so the repository pane stays on screen.
    let y = (0..20).find(|&y| row(y).contains("▾ work")).unwrap();
    let x = (0..200).find(|&x| buf[(x, y)].symbol() == "w").unwrap();
    assert_eq!(buf[(x, y)].bg, app.theme.group_bg);
    assert_eq!(buf[(x, y)].fg, app.theme.muted);
}
