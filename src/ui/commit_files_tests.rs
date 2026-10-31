//! The commit view of the file pane: `→` on a history commit lists its
//! files, each with its own diff.

use super::test_support::*;
use super::{App, Pane};
use crate::git::test_repo::TestRepo;
use crossterm::event::KeyCode;

/// A clean repository whose newest commit changes `a.txt` and adds `b.txt`.
fn two_file_commit() -> TestRepo {
    let a = TestRepo::new();
    a.write("a.txt", "one\nTWO\nthree\n");
    a.write("b.txt", "bee\n");
    a.git(&["add", "-A"]);
    a.git(&["commit", "-qm", "two files"]);
    a
}

fn diff_text(app: &App) -> String {
    app.diff
        .doc
        .as_ref()
        .map(|d| {
            d.lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn diff_key(app: &App) -> String {
    let key = app.diff.key.clone().unwrap_or_default();
    key.rsplit('\0').next().unwrap_or_default().to_string()
}

#[test]
fn right_lists_a_commits_files_and_each_shows_its_own_diff() {
    let a = two_file_commit();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    assert!(diff_key(&app).starts_with("c:"), "the newest commit");
    let hash = a.git(&["rev-parse", "HEAD"]).trim().to_string();

    app.press(&[key(KeyCode::Right)]);
    app.wait_background(10);
    assert_eq!(app.focus, Pane::Files, "the files list in the same pane");
    let view = app.commit_view.as_ref().expect("commit view");
    let files: Vec<(char, &str)> = view
        .files
        .iter()
        .flatten()
        .map(|f| (f.status, f.path.as_str()))
        .collect();
    assert_eq!(files, [('M', "a.txt"), ('A', "b.txt")]);
    // The cursor starts on row 0, the commit with its whole diff.
    assert_eq!(view.sel, 0);
    assert_eq!(view.selected(), None);
    assert_eq!(diff_key(&app), format!("c:{hash}"));
    assert!(diff_text(&app).contains("two files"), "the message");

    // Each file below shows its diff alone.
    app.press(&[key(KeyCode::Down)]);
    app.wait_background(10);
    assert_eq!(diff_key(&app), format!("cf:{hash}:a.txt"));
    assert!(diff_text(&app).contains("TWO"));
    assert!(!diff_text(&app).contains("bee"), "{}", diff_text(&app));

    app.press(&[key(KeyCode::Down)]);
    app.wait_background(10);
    assert!(diff_text(&app).contains("bee"));
    assert!(!diff_text(&app).contains("TWO"));

    // `→` moves on to the diff, `←` comes back to the files.
    app.press(&[key(KeyCode::End), key(KeyCode::Right)]);
    assert_eq!(app.focus, Pane::Diff);
    app.press(&[key(KeyCode::Left)]);
    assert_eq!(app.focus, Pane::Files);
    assert!(app.commit_view.is_some());

    // `←` again: back to the history, the cursor on the commit.
    app.press(&[key(KeyCode::Left)]);
    assert!(app.commit_view.is_none());
    assert_eq!(app.file_key.as_deref(), Some(format!("c:{hash}").as_str()));
    assert_eq!(diff_key(&app), format!("c:{hash}"));
    assert_eq!(app.focus, Pane::Files);
}

#[test]
fn enter_on_a_commit_still_opens_its_whole_diff() {
    let a = two_file_commit();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.focus, Pane::Diff);
    assert!(app.commit_view.is_none());
}

#[test]
fn commit_files_are_never_the_working_trees_and_survive_a_rescan() {
    let a = two_file_commit();
    a.write("a.txt", "dirty\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    // Down past the changed file and the working-tree row to the commit.
    app.press(&[key(KeyCode::Down), key(KeyCode::Down), key(KeyCode::Right)]);
    app.wait_background(10);
    app.press(&[key(KeyCode::Down)]);
    app.wait_background(10);
    let view = app.commit_view.as_ref().expect("commit view");
    assert_eq!(view.selected().map(|f| f.path.as_str()), Some("a.txt"));
    // Open With and discarding act on the working tree's files only.
    assert_eq!(app.selected_file(), None);
    let key_before = diff_key(&app);
    app.refresh_files();
    app.wait_background(10);
    assert!(app.commit_view.is_some(), "a rescan keeps the view");
    assert_eq!(diff_key(&app), key_before);
}

#[test]
fn selecting_another_repository_closes_the_commit_view() {
    let a = two_file_commit();
    let b = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root, &b.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    app.press(&[key(KeyCode::Right)]);
    assert!(app.commit_view.is_some());
    app.focus = Pane::Repos;
    app.press(&[key(KeyCode::Down)]);
    assert!(app.commit_view.is_none());
}

#[test]
fn the_commit_view_shows_the_commit_a_files_divider_and_the_files() {
    let a = two_file_commit();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    app.press(&[key(KeyCode::Right)]);
    app.wait_background(10);
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, " ● two files"), "{rows:#?}");
    assert!(shows(&rows, " Files "), "{rows:#?}");
    let row = |text: &str| rows.iter().find(|r| r.contains(text)).unwrap().clone();
    assert!(row(" M a.txt").contains("+1 -1"), "{}", row(" M a.txt"));
    assert!(row(" A b.txt").contains("+1"));
    // The diff pane is titled with the selected file.
    app.press(&[key(KeyCode::Down)]);
    app.wait_background(10);
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, " a.txt "));
}

#[test]
fn the_file_pane_keeps_the_repository_name_as_its_title_in_a_commit() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    a.git(&["commit", "-qam", "a subject"]);
    let name = a.root.file_name().unwrap().to_string_lossy().into_owned();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    app.press(&[key(KeyCode::Right)]);
    assert!(app.commit_view.is_some());
    let short = a.git(&["rev-parse", "--short", "HEAD"]).trim().to_string();
    for w in [160, 100, 56] {
        let rows = screen(&app, w, 20);
        // The Files pane's top border alone: the diff title carries the hash.
        let top = &rows[5];
        let start = top.find('┏').unwrap();
        let top = &top[start..start + top[start..].find('┓').unwrap()];
        assert!(top.contains(&format!(" {name} ")), "{w}: {top}");
        assert!(!top.contains(&short), "{w}: {top}");
        assert!(top.contains("━ ← ") && top.contains(" → ━"), "{w}: {top}");
    }
}
