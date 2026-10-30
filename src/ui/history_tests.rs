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
    // One changed file, then four commits.
    let items = app.file_items();
    assert_eq!(items.len(), 5);
    assert!(
        matches!(items[1], super::files::FileItem::Commit(c, super::files::Mark::Shared) if c.subject == "merge feat")
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

    // Down walks the commits.
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
    app.press(&[ch('2'), key(KeyCode::End)]);
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
fn the_graph_gets_fainter_not_stronger_when_its_pane_loses_focus() {
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
        let y = (0..20).find(|&y| row(y).contains("● init")).unwrap();
        let x = (0..150).find(|&x| buf[(x, y)].symbol() == "●").unwrap();
        crate::theme::color_rgb(buf[(x, y)].fg)
    };
    let distance = |(r, g, b): (u8, u8, u8)| {
        let (br, bg, bb) = app.theme.bg_rgb();
        (r as i32 - br as i32).abs() + (g as i32 - bg as i32).abs() + (b as i32 - bb as i32).abs()
    };
    app.focus = Pane::Files;
    let focused = distance(node_fg(&app));
    app.focus = Pane::Repos;
    let dimmed = distance(node_fg(&app));
    assert!(
        dimmed < focused,
        "dimmed {dimmed} must be fainter than focused {focused}"
    );
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
            row.contains("⎇ main"),
            row.contains("⎇ side"),
            row.contains("⚑ v1.0"),
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
    // Labels sit between the graph and the subject.
    let row = init_row(&app);
    assert!(
        row.find("⚑ v1.0").unwrap() < row.find(" init").unwrap(),
        "{row}"
    );
    app.press(&[ch('>')]);
    assert_eq!(labels(&init_row(&app)), (false, false, false));
}
