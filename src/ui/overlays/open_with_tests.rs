//! Open With: the app lists, the terminal handover, and GUI starts.

use crate::config::Launcher;
use crate::git::test_repo::TestRepo;
use crate::ui::test_support::{key, scanned_app, store_of};
use crate::ui::{Pane, UiMode};
use crossterm::event::KeyCode;

fn app_of(name: &str, command: &str, gui: bool) -> Launcher {
    Launcher {
        name: name.into(),
        command: command.into(),
        gui,
    }
}

#[test]
fn enter_on_a_repository_runs_the_picked_terminal_app_in_it() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.cfg.launchers = vec![
        app_of("Open Folder", "open .", true),
        app_of("tool", "tool --dir {path}", false),
    ];
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::OpenWith);
    assert_eq!(app.focus, Pane::Repos, "enter no longer moves to Files");
    app.press(&[key(KeyCode::Down), key(KeyCode::Down), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Normal);
    let req = app.terminal_request.take().expect("terminal handover");
    assert_eq!(req.cwd, a.root);
    assert_eq!(req.command, format!("tool --dir '{}'", a.root.display()));
    assert!(!req.pause);
    assert_eq!(
        req.name.as_deref(),
        Some("tool"),
        "the notice names the app"
    );
    // The next Open With starts on the app used last.
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.open_with.as_ref().unwrap().cursor, 1);
    app.press(&[key(KeyCode::Esc)]);
    assert_eq!(app.mode, UiMode::Normal);
    assert!(app.terminal_request.is_none());
}

#[test]
fn gui_apps_start_in_the_background_and_report_failures() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.cfg.launchers = vec![app_of("ok", "true", true), app_of("bad", "exit 3", true)];
    app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
    assert!(app.terminal_request.is_none(), "GUI apps keep the screen");
    app.wait_background(10);
    assert_eq!(app.notice.as_ref().unwrap().text, "Opened ok");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Down), key(KeyCode::Enter)]);
    app.wait_background(10);
    assert_eq!(app.notice.as_ref().unwrap().text, "bad: exited with code 3");
}

#[test]
fn enter_on_a_changed_file_opens_it_and_on_a_commit_moves_to_the_diff() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.focus = Pane::Files;
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::OpenWith);
    app.press(&[key(KeyCode::Enter)]);
    let req = app.terminal_request.take().expect("vim by default");
    assert_eq!(req.cwd, a.root);
    assert_eq!(
        req.command,
        format!("vim '{}'", a.root.join("a.txt").display())
    );
    // The row below the changed file is a history commit.
    app.press(&[key(KeyCode::Down), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Normal);
    assert_eq!(app.focus, Pane::Diff);
}

fn labels(app: &crate::ui::App) -> Vec<String> {
    let d = app.open_with.as_ref().expect("dialog");
    d.tabs.iter().map(|t| t.label.clone()).collect()
}

/// The two quoted paths of a `vimdiff 'old' 'new'` command.
fn diff_paths(command: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let parts: Vec<&str> = command.split('\'').collect();
    (parts[1].into(), parts[3].into())
}

#[test]
fn a_changed_file_offers_a_diff_head_and_the_working_tree() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    let scratch = tempfile::tempdir().unwrap();
    app.versions_dir = scratch.path().to_path_buf();
    app.wait_background(10);
    app.focus = Pane::Files;
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(labels(&app), ["Diff", "HEAD", "Working Tree"]);
    assert_eq!(
        app.open_with.as_ref().unwrap().tab,
        2,
        "the file as it is now"
    );
    // HEAD's version is a read-only copy.
    app.press(&[key(KeyCode::Left), key(KeyCode::Enter)]);
    assert!(
        app.terminal_request.is_none(),
        "written in the background first"
    );
    app.wait_background(10);
    let req = app.terminal_request.take().expect("vim on HEAD's copy");
    let copy = scratch.path().join("HEAD/a.txt");
    assert_eq!(req.command, format!("vim '{}'", copy.display()));
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "one\ntwo\nthree\n");
    assert!(std::fs::metadata(&copy).unwrap().permissions().readonly());
    // The Diff tab lists the diff apps: HEAD's copy against the file.
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.open_with.as_ref().unwrap().tab, 1, "the last tab");
    app.press(&[key(KeyCode::Left), key(KeyCode::Enter)]);
    app.wait_background(10);
    let req = app.terminal_request.take().expect("vimdiff");
    assert!(req.command.starts_with("vimdiff "), "{}", req.command);
    let (old, new) = diff_paths(&req.command);
    assert_eq!(old, copy);
    assert_eq!(new, a.root.join("a.txt"));
}

#[test]
fn a_commit_file_opens_its_parent_its_commit_or_a_diff_between_them() {
    let a = TestRepo::new();
    a.write("a.txt", "edited\n");
    a.write("b.txt", "bee\n");
    a.git(&["add", "-A"]);
    a.git(&["commit", "-qm", "two"]);
    let short = a.git(&["rev-parse", "--short", "HEAD"]).trim().to_string();
    let mut app = scanned_app(store_of(&[&a.root]));
    let scratch = tempfile::tempdir().unwrap();
    app.versions_dir = scratch.path().to_path_buf();
    app.cfg.file_launchers = vec![app_of("cat", "cat {path}", false)];
    app.focus = Pane::Files;
    app.wait_background(10);
    // `→` opens the commit's files on the commit row; `↓` to the first file.
    app.press(&[key(KeyCode::Right)]);
    app.wait_background(10);
    app.press(&[key(KeyCode::Down)]);
    // `enter` on a commit's file opens the dialog, not the diff pane.
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::OpenWith);
    assert_eq!(
        labels(&app),
        ["Diff", "Parent", short.as_str(), "Working Tree"]
    );
    app.press(&[key(KeyCode::Left), key(KeyCode::Enter)]);
    app.wait_background(10);
    let req = app.terminal_request.take().expect("the commit's version");
    let at = |label: &str| scratch.path().join(label).join("a.txt");
    assert_eq!(req.command, format!("cat '{}'", at(&short).display()));
    assert_eq!(std::fs::read_to_string(at(&short)).unwrap(), "edited\n");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Left), key(KeyCode::Left)]);
    assert_eq!(app.open_with.as_ref().unwrap().tab, 0);
    app.press(&[key(KeyCode::Enter)]);
    app.wait_background(10);
    let (old, new) = diff_paths(&app.terminal_request.take().unwrap().command);
    assert_eq!(old, at(&format!("{short}^")));
    assert_eq!(std::fs::read_to_string(old).unwrap(), "one\ntwo\nthree\n");
    assert_eq!(new, at(&short));
    // An added file has no parent: the diff starts from an empty file.
    app.press(&[key(KeyCode::Down), key(KeyCode::Enter)]);
    assert_eq!(labels(&app), ["Diff", short.as_str(), "Working Tree"]);
    app.press(&[key(KeyCode::Enter)]);
    app.wait_background(10);
    let (old, _) = diff_paths(&app.terminal_request.take().unwrap().command);
    assert_eq!(old, scratch.path().join("empty/b.txt"));
    assert_eq!(std::fs::read(old).unwrap(), b"");
}

#[test]
fn enter_on_the_commit_row_of_its_view_still_moves_to_the_diff() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Files;
    app.wait_background(10);
    app.press(&[key(KeyCode::Right)]);
    app.wait_background(10);
    app.press(&[key(KeyCode::Home), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Normal);
    assert_eq!(app.focus, Pane::Diff);
}
