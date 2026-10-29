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
    assert!(!shows(&rows, "a.txt +1 -3"), "Repos focus hides the diff");
    app.focus = Pane::Files;
    let rows = screen(&app, 150, 30);
    assert!(
        !shows(&rows, "Repos[1]"),
        "Files focus hides the repositories"
    );
    assert!(shows(&rows, "a.txt +1 -3"));
    assert!(shows(&rows, "<ctrl+u> Refresh All"));
    assert!(shows(&rows, "|___/"), "logo shown when wide");
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
    assert!(!shows(&rows, "|___/"));
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
fn only_code_lines_carry_the_line_number_gutter() {
    let a = TestRepo::new();
    a.write("a.txt", "one\n2\nthree\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let rows = screen(&app, 150, 30);
    let diff_x = pane_layout(Rect::new(0, 5, 150, 24), app.focus)[1].1.x as usize + 2;
    let starts = |needle: &str| {
        let row = rows.iter().find(|r| r.contains(needle)).unwrap();
        row[..row.find(needle).unwrap()].chars().count()
    };
    // The section title sits on a border-to-border line.
    let section = rows
        .iter()
        .find(|r| r.contains(" Unstaged changes ──"))
        .unwrap();
    assert!(
        section.contains("├────") && section.trim_end().ends_with('┤'),
        "{section}"
    );
    // Hunk headers are not shown; the first hunk follows the title directly.
    assert!(!shows(&rows, "@@"));
    let at = rows
        .iter()
        .position(|r| r.contains(" Unstaged changes "))
        .unwrap();
    assert!(
        rows[at + 1].contains("    1    1  one"),
        "{:?}",
        &rows[at..at + 2]
    );
    // git's redundant file header lines are not shown.
    assert!(!shows(&rows, "diff --git") && !shows(&rows, "index ") && !shows(&rows, "+++ b/"));
    // Code lines start after the number gutter.
    assert_eq!(starts("+2"), diff_x + 10);
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
fn unpushed_commits_show_date_and_subject_and_their_hash_titles_the_diff() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(parent, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    a.write("a.txt", "next\n");
    a.git(&["commit", "-qam", "second commit\n\nbody text"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let commit = app
        .current_tree()
        .unwrap()
        .status
        .as_ref()
        .unwrap()
        .unpushed[0]
        .clone();
    let rows = screen(&app, 160, 30);
    assert!(shows(&rows, " ↑ second commit"), "{rows:#?}");
    assert!(
        shows(&rows, &format!(" {} +1 -3 ", commit.hash)),
        "{rows:#?}"
    );
    let byline = rows
        .iter()
        .position(|r| r.contains("  t <t@t>"))
        .expect("byline");
    assert!(
        rows[byline + 1].contains("second commit"),
        "message right below"
    );
    assert!(!shows(&rows, "CommitDate"));
    assert!(shows(&rows, "body text"));
}

#[test]
fn commit_messages_wrap_even_with_line_wrap_off() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(parent, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    let long = "word ".repeat(40);
    a.write("a.txt", &format!("{long}\n"));
    a.git(&["commit", "-qam", &format!("subject\n\n{long}END")]);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    assert!(!app.diff.wrap);
    let rows = screen(&app, 160, 40);
    assert!(
        shows(&rows, "END"),
        "the message's tail is wrapped into view"
    );
    // A code line still ends at the pane edge.
    assert!(
        rows.iter().any(|r| r.contains("+word") && r.contains('…')),
        "{rows:#?}"
    );
}

#[test]
fn commit_files_get_a_full_width_title_line() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(parent, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    a.write("a.txt", "next\n");
    a.git(&["commit", "-qam", "second"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let rows = screen(&app, 160, 30);
    let title = rows
        .iter()
        .find(|r| r.contains(" a.txt ──"))
        .expect("file title, right-aligned and joined to the right border");
    assert!(
        title.contains("├────────────────────"),
        "runs from the left border: {title}"
    );
    assert!(!shows(&rows, "diff --git"));
    // git's `---` is a border-to-border rule carrying the stat summary on its
    // right, and no blank row separates the stat from the file title.
    let rule = rows
        .iter()
        .position(|r| r.contains("├──") && r.contains(" 1 file changed, ") && r.contains("──┤"))
        .expect("rule with the summary");
    assert!(!rows.iter().any(|r| r.contains(" --- ")));
    let title = rows.iter().position(|r| r.contains(" a.txt ──")).unwrap();
    assert_eq!(title, rule + 2, "rule, one stat row, then the file title");
    // Labels are centered: the dashes on both sides differ by at most one.
    let line = &rows[title];
    let start = line.find('├').unwrap();
    let seg: String = line[start..]
        .chars()
        .skip(1)
        .take_while(|c| *c != '┤')
        .collect();
    let left = seg.chars().take_while(|c| *c == '─').count();
    let right = seg.chars().rev().take_while(|c| *c == '─').count();
    assert!(left.abs_diff(right) <= 1, "{line}");
    assert!(
        rows[title - 1].contains("a.txt | "),
        "{:?}",
        &rows[rule..=title]
    );
}

#[test]
fn a_gap_line_separates_hunks() {
    let a = TestRepo::new();
    let body: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    a.write("a.txt", &body);
    a.git(&["commit", "-qam", "thirty lines"]);
    let changed = body
        .replace("line 2\n", "line two\n")
        .replace("line 28\n", "line 28b\n");
    a.write("a.txt", &changed);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let rows = screen(&app, 160, 40);
    let gaps: Vec<&String> = rows.iter().filter(|r| r.contains("   ⋯")).collect();
    assert_eq!(gaps.len(), 1, "one gap between the two hunks: {rows:#?}");
    assert!(!shows(&rows, "@@"));
}
