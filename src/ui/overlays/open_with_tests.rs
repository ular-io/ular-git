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
