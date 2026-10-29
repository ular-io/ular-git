//! Push and pull through the keys, against a local bare remote.

use super::test_support::*;
use super::UiMode;
use crate::git::test_repo::TestRepo;
use crossterm::event::KeyCode;

/// `repo` pushed to a bare `origin`, with one more local commit.
fn ahead_of_origin(repo: &TestRepo) -> std::path::PathBuf {
    let parent = repo.root.parent().unwrap();
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
    repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    repo.write("a.txt", "next\n");
    repo.git(&["commit", "-qam", "second"]);
    remote
}

#[test]
fn push_asks_first_lists_the_commits_and_pushes() {
    let a = TestRepo::new();
    ahead_of_origin(&a);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('P')]);
    assert_eq!(app.mode, UiMode::Confirm);
    let d = app.confirm.as_ref().unwrap();
    assert!(d.ok_focused, "push is not destructive: Push is focused");
    assert!(
        d.lines[0].contains("1 commit on main to origin/main"),
        "{:?}",
        d.lines
    );
    assert!(d.lines.iter().any(|l| l.contains("second")));
    app.press(&[key(KeyCode::Enter)]);
    assert!(app.remote_ops.contains_key(&a.root), "row shows push…");
    app.wait_background(30);
    assert!(app.remote_ops.is_empty());
    let s = app.current_tree().unwrap().status.as_ref().unwrap();
    assert_eq!(s.branch.ahead, 0);
    assert!(app.notice.as_ref().unwrap().text.contains("pushed"));

    app.press(&[ch('P')]);
    assert_eq!(app.mode, UiMode::Normal, "nothing left to push");
}

#[test]
fn push_offers_to_publish_a_branch_without_upstream() {
    let a = TestRepo::new();
    ahead_of_origin(&a);
    a.git(&["switch", "-q", "-c", "feat/y"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('P')]);
    app.wait_background(10);
    assert_eq!(app.mode, UiMode::Confirm);
    assert!(app.confirm.as_ref().unwrap().lines[1].contains("origin/feat/y"));
    app.press(&[key(KeyCode::Enter)]);
    app.wait_background(30);
    let s = app.current_tree().unwrap().status.as_ref().unwrap();
    assert_eq!(s.branch.upstream.as_deref(), Some("origin/feat/y"));
}

#[test]
fn pull_fast_forwards_without_asking() {
    let a = TestRepo::new();
    let remote = ahead_of_origin(&a);
    a.git(&["push", "-q"]);
    let other = a.root.parent().unwrap().join("other");
    crate::git::test_repo::git(
        a.root.parent().unwrap(),
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    std::fs::write(other.join("n.txt"), "n\n").unwrap();
    crate::git::test_repo::git(&other, &["add", "n.txt"]);
    crate::git::test_repo::git(&other, &["commit", "-qm", "theirs"]);
    crate::git::test_repo::git(&other, &["push", "-q"]);
    a.git(&["fetch", "-q"]);

    let mut app = scanned_app(store_of(&[&a.root]));
    assert_eq!(
        app.current_tree()
            .unwrap()
            .status
            .as_ref()
            .unwrap()
            .branch
            .behind,
        1
    );
    app.press(&[ch('p')]);
    assert_eq!(app.mode, UiMode::Normal);
    app.wait_background(30);
    assert!(a.root.join("n.txt").exists());
    assert_eq!(
        app.current_tree()
            .unwrap()
            .status
            .as_ref()
            .unwrap()
            .branch
            .behind,
        0
    );
    assert!(app
        .notice
        .as_ref()
        .unwrap()
        .text
        .contains("pulled fast-forward"));
}

#[test]
fn pull_without_upstream_is_explained() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('p')]);
    assert!(app.remote_ops.is_empty());
    assert!(app.notice.as_ref().unwrap().text.contains("no upstream"));
}
