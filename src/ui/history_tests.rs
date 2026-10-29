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
        matches!(items[1], super::files::FileItem::Commit(c, false) if c.subject == "merge feat")
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
