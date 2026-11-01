//! The history section of the file pane.

use super::test_support::*;
use super::Pane;
use crate::git::test_repo::TestRepo;
use crossterm::event::KeyCode;

#[test]
fn the_file_pane_lists_history_with_its_graph_below_the_changes() {
    let a = TestRepo::new();
    a.git(&["switch", "-q", "-c", "feat"]);
    a.write("f.txt", "f\n");
    a.git(&["add", "f.txt"]);
    a.git(&["commit", "-qm", "on feat"]);
    a.git(&["switch", "-q", "main"]);
    a.write("m.txt", "m\n");
    a.git(&["add", "m.txt"]);
    a.git(&["commit", "-qm", "on main"]);
    a.git(&["merge", "-q", "--no-ff", "-m", "merge feat", "feat"]);
    a.write("a.txt", "dirty\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    // One changed file, the working-tree row, then four commits.
    let items = app.file_items();
    assert_eq!(items.len(), 6);
    assert!(matches!(items[1], super::files::FileItem::WorkTree));
    assert!(
        matches!(items[2], super::files::FileItem::Commit(c, super::files::Mark::Shared) if c.subject == "merge feat")
    );
    drop(items);

    app.focus = Pane::Files;
    let rows = screen(&app, 150, 30);
    assert!(shows(&rows, " History "), "{rows:#?}");
    // One row per commit: the merge opens a lane on its own row and the
    // branch point closes it; no connector rows.
    let at = |text: &str| rows.iter().position(|r| r.contains(text)).unwrap();
    assert!(rows[at("merge feat")].contains("●─╮"), "{rows:#?}");
    assert!(rows[at(" init")].contains("●─╯"), "{rows:#?}");
    assert_eq!(at(" init") - at("merge feat"), 3, "four commits, four rows");

    // Down walks the working-tree row, then the commits.
    app.press(&[key(KeyCode::Down), key(KeyCode::Down)]);
    let key_now = app.file_key.clone().unwrap();
    assert!(key_now.starts_with("c:"), "{key_now}");
    app.wait_background(10);
    assert!(app
        .diff
        .doc
        .as_ref()
        .is_some_and(|d| d.lines[0].kind == crate::git::diff::LineKind::Byline));
}

#[test]
fn more_reads_a_longer_history_and_keeps_the_cursor_in_place() {
    let a = TestRepo::new();
    for i in 0..105 {
        a.write("a.txt", &format!("{i}\n"));
        a.git(&["commit", "-qam", &format!("c{i}")]);
    }
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    let items = app.file_items().len();
    assert_eq!(items, 101, "100 commits and `… more`");
    app.press(&[key(KeyCode::Right), key(KeyCode::End)]);
    assert_eq!(app.file_key.as_deref(), Some("more"));
    app.wait_background(10);
    assert_eq!(app.file_items().len(), 106, "all 106 commits, no more row");
    assert_eq!(app.file_sel, 100, "cursor on the first newly read commit");
    assert!(app.file_key.as_deref().unwrap().starts_with("c:"));
}

#[test]
fn commits_to_pull_are_marked_with_a_down_arrow() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(
        parent,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            remote.to_str().unwrap(),
        ],
    );
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    let other = parent.join("other");
    crate::git::test_repo::git(
        parent,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    std::fs::write(other.join("u.txt"), "u\n").unwrap();
    crate::git::test_repo::git(&other, &["add", "-A"]);
    crate::git::test_repo::git(&other, &["commit", "-qm", "from upstream"]);
    crate::git::test_repo::git(&other, &["push", "-q"]);
    a.git(&["fetch", "-q"]);

    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    let items = app.file_items();
    assert!(matches!(
        items[0],
        super::files::FileItem::Commit(c, super::files::Mark::Incoming) if c.subject == "from upstream"
    ));
    drop(items);
    app.focus = Pane::Files;
    let rows = screen(&app, 150, 30);
    assert!(
        rows.iter()
            .any(|r| r.contains("↓") && r.contains("from upstream")),
        "{rows:#?}"
    );
}

#[test]
fn the_graph_keeps_its_colors_when_its_pane_loses_focus() {
    let a = TestRepo::new();
    a.write("a.txt", "next\n");
    a.git(&["commit", "-qam", "second"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    // The `init` node: not selected (the cursor is on `second`).
    let node_fg = |app: &crate::ui::App| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 20)).unwrap();
        terminal.draw(|f| crate::ui::render::draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let row = |y: u16| (0..150).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let y = (0..20)
            .find(|&y| row(y).contains("●") && row(y).contains(" init"))
            .unwrap();
        let x = (0..150).find(|&x| buf[(x, y)].symbol() == "●").unwrap();
        crate::theme::color_rgb(buf[(x, y)].fg)
    };
    app.focus = Pane::Files;
    let focused = node_fg(&app);
    app.focus = Pane::Repos;
    assert_eq!(node_fg(&app), focused, "content is never dimmed");
}

#[test]
fn greater_than_cycles_branch_and_tag_labels_on_the_history() {
    let a = TestRepo::new();
    a.git(&["tag", "v1.0"]);
    a.git(&["branch", "side"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    let init_row = |app: &crate::ui::App| {
        screen(app, 150, 20)
            .into_iter()
            .find(|r| r.contains('●') && r.contains(" init"))
            .unwrap()
    };
    let labels = |row: &str| {
        (
            row.contains(" main "),
            row.contains(" side "),
            row.contains(" v1.0 "),
        )
    };
    assert_eq!(
        labels(&init_row(&app)),
        (false, false, false),
        "hidden at first"
    );
    app.press(&[ch('>')]);
    assert_eq!(
        labels(&init_row(&app)),
        (true, true, false),
        "{}",
        init_row(&app)
    );
    app.press(&[ch('>')]);
    assert_eq!(labels(&init_row(&app)), (true, true, true));
    app.press(&[ch('>')]);
    assert_eq!(labels(&init_row(&app)), (false, false, true));
    // Labels follow the subject and run to the pane border (or the
    // scrollbar drawn on it).
    let row = init_row(&app);
    let at = row.find(" v1.0 ").unwrap();
    assert!(row.find(" init").unwrap() < at, "{row}");
    let after = row[at + " v1.0 ".len()..].chars().next();
    assert!(matches!(after, Some('┃' | '↑')), "{row}");
    app.press(&[ch('>')]);
    assert_eq!(labels(&init_row(&app)), (false, false, false));
}

#[test]
fn ref_labels_keep_their_badge_colors_on_the_selected_row() {
    let a = TestRepo::new();
    a.git(&["tag", "v1.0"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.press(&[ch('>'), ch('>')]);
    // The cursor is on `init`, the only commit.
    let bgs = |app: &crate::ui::App| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 20)).unwrap();
        terminal.draw(|f| crate::ui::render::draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let row = |y: u16| (0..150).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let y = (0..20)
            .find(|&y| row(y).contains('●') && row(y).contains(" init"))
            .unwrap();
        let x = (1..150)
            .find(|&x| buf[(x - 1, y)].symbol() == " " && buf[(x, y)].symbol() == "v")
            .unwrap();
        // A branch and a tag differ in color: no line between them.
        // ` main ` then ` v1.0 `: the badges touch, the `n` three cells back.
        assert_eq!(buf[(x - 3, y)].symbol(), "n");
        assert_ne!(buf[(x - 2, y)].symbol(), "│");
        let strip = (0..x).rev().find(|&i| buf[(i, y)].symbol() == "m").unwrap() - 2;
        (buf[(x, y)].bg, buf[(strip, y)].bg)
    };
    app.focus = Pane::Files;
    let (label, bar) = bgs(&app);
    assert_eq!(bar, app.theme.selection_bg);
    assert_ne!(label, bar, "the tag keeps its own color on the bar");
    assert_ne!(label, app.theme.warning, "deepened, not the raw role color");
    // A dimmed pane shows the same faded colors.
    app.focus = Pane::Repos;
    let (dimmed, _) = bgs(&app);
    assert_eq!(
        dimmed, label,
        "same badge whether or not the pane has focus"
    );
}

#[test]
fn a_clean_tree_starts_with_the_newest_commit_and_no_divider() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.history.meta = crate::ui::history_view::CommitMeta::None;
    let rows = screen(&app, 150, 20);
    assert!(!shows(&rows, " History "), "{rows:#?}");
    assert!(!shows(&rows, "○"), "no working-tree row: {rows:#?}");
    assert!(rows[6].contains("● init"), "{rows:#?}");
    assert!(app.file_key.as_deref().unwrap().starts_with("c:"));
}

#[test]
fn a_dirty_tree_keeps_the_divider_and_summarizes_its_changes() {
    let a = TestRepo::new();
    a.write("a.txt", "one\ntwo\n");
    a.write("new.txt", "x\ny\nz\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    let rows = screen(&app, 150, 20);
    let divider = rows
        .iter()
        .position(|r| r.contains(" History "))
        .expect("divider kept");
    assert!(
        rows[divider + 1].contains("○ Uncommitted Changes"),
        "{rows:#?}"
    );
    assert!(
        rows[divider + 1].contains(" Ⓜ 2 ┃"),
        "the changed badge at the right edge: {rows:#?}"
    );
    // The first file is selected first; the working-tree row shows a stat.
    assert_eq!(app.file_key.as_deref(), Some("f:a.txt"));
    app.press(&[key(KeyCode::Down), key(KeyCode::Down)]);
    assert_eq!(app.file_key.as_deref(), Some("wt"));
    app.wait_background(10);
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, " Uncommitted Changes +"), "{rows:#?}");
    assert!(
        shows(&rows, "2 files changed, "),
        "summary on the rule: {rows:#?}"
    );
    assert!(shows(&rows, " new.txt | 3 +++"), "{rows:#?}");
}

#[test]
fn only_same_colored_labels_get_a_line_between_them() {
    let a = TestRepo::new();
    a.git(&["tag", "v1.0"]);
    a.git(&["tag", "v1.1"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.press(&[ch('>'), ch('>')]);
    app.focus = Pane::Files;
    let row = screen(&app, 150, 20)
        .into_iter()
        .find(|r| r.contains('●') && r.contains(" init"))
        .unwrap();
    // `main` → tag: no line; tag → tag: a line (git orders the tags).
    assert!(
        row.contains(" main  v1.1 │ v1.0 ") || row.contains(" main  v1.0 │ v1.1 "),
        "{row}"
    );
}

#[test]
fn history_subjects_use_the_file_rows_text_style() {
    let a = TestRepo::new();
    a.write("a.txt", "next\n");
    a.git(&["commit", "-qam", "second"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.history.meta = crate::ui::history_view::CommitMeta::None;
    // The `init` subject: not selected (the cursor is on `second`).
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 20)).unwrap();
    terminal.draw(|f| crate::ui::render::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16| (0..150).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let y = (0..20).find(|&y| row(y).contains("● init")).unwrap();
    let x = (0..150).find(|&x| buf[(x, y)].symbol() == "●").unwrap() + 2;
    assert_eq!(buf[(x, y)].symbol(), "i");
    assert_ne!(buf[(x, y)].fg, app.theme.muted);
    // The same text color as a changed file's name in a focused pane.
    assert_eq!(buf[(x, y)].fg, app.theme.fg);
}

#[test]
fn dot_on_the_files_pane_cycles_the_date_and_author_at_any_width() {
    let a = TestRepo::new();
    let log = |f: &str| {
        a.git(&["log", "-1", &format!("--format={f}")])
            .trim()
            .to_string()
    };
    let (date, author) = (log("%as"), log("%an"));
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.pane_count = 1;
    // A narrow pane still shows what is chosen; the subject is cut instead.
    // (The diff pane is hidden, so its byline cannot match.)
    let row = |app: &crate::ui::App| screen(app, 40, 20)[6].clone();
    assert!(
        row(&app).contains(&format!("● {date} init")),
        "{}",
        row(&app)
    );
    app.press(&[ch('.')]);
    let both = format!("● {date} {author} init");
    assert!(row(&app).contains(&both), "{}", row(&app));
    app.press(&[ch('.')]);
    assert!(
        row(&app).contains(&format!("● {author} init")),
        "{}",
        row(&app)
    );
    app.press(&[ch('.')]);
    assert!(row(&app).contains("● init"), "{}", row(&app));
    app.press(&[ch('.')]);
    assert!(row(&app).contains(&format!("● {date} init")));
    assert!(!app.diff.wrap, "`.` on Files never wraps the diff");
}

#[test]
fn subject_only_rows_give_the_selected_commit_a_date_and_author_line() {
    let a = TestRepo::new();
    a.git(&["switch", "-q", "-c", "feat"]);
    a.write("f.txt", "f\n");
    a.git(&["add", "f.txt"]);
    a.git(&["commit", "-qm", "on feat"]);
    a.git(&["switch", "-q", "main"]);
    a.git(&["merge", "-q", "--no-ff", "-m", "merge feat", "feat"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.history.meta = crate::ui::history_view::CommitMeta::None;
    let rows = screen(&app, 150, 20);
    let at = rows.iter().position(|r| r.contains("merge feat")).unwrap();
    // The merge is selected: its lanes run on beside `YYYY-MM-DD HH:MM  t`.
    let detail = &rows[at + 1];
    assert!(detail.contains("│ │ "), "{rows:#?}");
    let after = detail.split("│ │ ").nth(1).unwrap();
    let when: String = after.chars().take(16).collect();
    assert!(
        when.chars().enumerate().all(|(i, c)| match i {
            4 | 7 => c == '-',
            10 => c == ' ',
            13 => c == ':',
            _ => c.is_ascii_digit(),
        }),
        "{detail}"
    );
    assert!(after[16..].starts_with("  t "), "{detail}");
    assert!(rows[at + 2].contains("on feat"), "{rows:#?}");
    // Moving down moves the second line with the cursor.
    app.press(&[key(KeyCode::Down)]);
    let rows = screen(&app, 150, 20);
    assert!(rows[at + 1].contains("on feat"), "{rows:#?}");
    assert!(rows[at + 2].contains("  t "), "{rows:#?}");
    // With dates on every row, no row takes a second line.
    app.history.meta = crate::ui::history_view::CommitMeta::Date;
    let rows = screen(&app, 150, 20);
    assert!(rows[at + 1].contains("on feat"), "{rows:#?}");
    assert!(!rows[at + 2].contains("  t "), "{rows:#?}");
}
