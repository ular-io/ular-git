//! The uncommitted changes' view of the file pane: `→` on the Uncommitted
//! Changes row lists the working tree's files against HEAD, re-read on every
//! rescan.

use super::commit_files::ViewOf;
use super::test_support::*;
use super::{App, Pane, UiMode};
use crate::git::test_repo::TestRepo;
use crossterm::event::KeyCode;

fn diff_key(app: &App) -> String {
    let key = app.diff.key.clone().unwrap_or_default();
    key.rsplit('\0').next().unwrap_or_default().to_string()
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

fn view_paths(app: &App) -> Vec<(char, String)> {
    app.commit_view
        .as_ref()
        .and_then(|v| v.files.as_ref())
        .map(|f| f.iter().map(|f| (f.status, f.path.clone())).collect())
        .unwrap_or_default()
}

/// `a.txt` staged and edited again, `new.txt` untracked; the cursor on the
/// Uncommitted Changes row, its view opened.
fn open_view() -> (TestRepo, App) {
    let a = TestRepo::new();
    a.write("a.txt", "one\nSTAGED\nthree\n");
    a.git(&["add", "a.txt"]);
    a.write("a.txt", "one\nSTAGED\nthree\nunstaged\n");
    a.write("new.txt", "n\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    app.press(&[key(KeyCode::Down), key(KeyCode::Down)]);
    assert_eq!(app.file_key.as_deref(), Some("wt"));
    app.press(&[key(KeyCode::Right)]);
    app.wait_background(10);
    (a, app)
}

#[test]
fn right_on_uncommitted_changes_lists_its_files_with_combined_diffs() {
    let (_a, mut app) = open_view();
    assert_eq!(app.focus, Pane::Files);
    let view = app.commit_view.as_ref().expect("view");
    assert_eq!(view.of, ViewOf::WorkTree);
    assert_eq!(
        view_paths(&app),
        [('M', "a.txt".to_string()), ('?', "new.txt".to_string())]
    );
    // Staged and unstaged together, against HEAD.
    assert_eq!(diff_key(&app), "wf:a.txt");
    let text = diff_text(&app);
    assert!(
        text.contains("STAGED") && text.contains("unstaged"),
        "{text}"
    );
    assert!(!text.contains("Staged changes"), "no sections: {text}");
    app.press(&[key(KeyCode::Down)]);
    app.wait_background(10);
    assert_eq!(diff_key(&app), "wf:new.txt");
    // Row 0 is the summary.
    app.press(&[key(KeyCode::Home)]);
    app.wait_background(10);
    assert_eq!(diff_key(&app), "wt");
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, " ○ Uncommitted Changes"), "{rows:#?}");
    assert!(shows(&rows, " ? new.txt"), "{rows:#?}");
    // `enter` on a file: Open With with HEAD and the working tree's file.
    app.press(&[key(KeyCode::End), key(KeyCode::Up), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::OpenWith);
    let tabs: Vec<&str> = app
        .open_with
        .as_ref()
        .unwrap()
        .tabs
        .iter()
        .map(|t| t.label.as_str())
        .collect();
    assert_eq!(tabs, ["Diff", "HEAD", "Working Tree"]);
    app.press(&[key(KeyCode::Esc), key(KeyCode::Home)]);
    // `←` returns to the row it was opened from.
    app.press(&[key(KeyCode::Left)]);
    assert!(app.commit_view.is_none());
    assert_eq!(app.file_key.as_deref(), Some("wt"));
}

#[test]
fn a_rescan_rereads_the_files_and_keeps_the_cursor_on_its_file() {
    let (a, mut app) = open_view();
    app.press(&[key(KeyCode::Down)]);
    app.wait_background(10);
    assert_eq!(diff_key(&app), "wf:new.txt");
    // A file sorted before the cursor's appears.
    a.write("0.txt", "zero\n");
    app.request_scan(a.root.clone(), true);
    app.wait_background(20);
    let paths: Vec<String> = view_paths(&app).into_iter().map(|(_, p)| p).collect();
    assert_eq!(paths, ["a.txt", "0.txt", "new.txt"]);
    assert_eq!(diff_key(&app), "wf:new.txt", "the cursor stays on its file");
}

#[test]
fn the_uncommitted_files_are_the_working_trees_and_can_be_discarded() {
    let (a, mut app) = open_view();
    assert_eq!(app.selected_file().map(|f| f.path), Some("a.txt".into()));
    app.press(&[ch(':')]);
    app.type_text("discard");
    assert!(app.quick.as_ref().unwrap().items[0].enabled);
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Confirm);
    app.press(&[key(KeyCode::Left), key(KeyCode::Enter)]);
    app.wait_background(20);
    assert_eq!(view_paths(&app), [('?', "new.txt".to_string())]);
    // Once the tree is clean the view closes.
    std::fs::remove_file(a.root.join("new.txt")).unwrap();
    app.request_scan(a.root.clone(), true);
    app.wait_background(20);
    assert!(app.commit_view.is_none());
}
