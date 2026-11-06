//! Fetch through the keys (`ctrl+f`, `ctrl+u`), against local bare remotes.

use super::overlays::message::MessageKind;
use super::test_support::*;
use super::{Pane, UiMode};
use crate::git::test_repo::{git, TestRepo};
use crate::store::RepoStore;
use crossterm::event::KeyCode;

/// Gives `repo` a bare `origin` and pushes one commit to it from another
/// clone, so `repo` is one behind once it fetches.
fn behind_once_fetched(repo: &TestRepo) {
    let parent = repo.root.parent().unwrap();
    let remote = parent.join("remote.git");
    let remote = remote.to_str().unwrap();
    git(parent, &["init", "-q", "--bare", "-b", "main", remote]);
    repo.git(&["remote", "add", "origin", remote]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    let other = parent.join("other");
    git(parent, &["clone", "-q", remote, other.to_str().unwrap()]);
    std::fs::write(other.join("n.txt"), "n\n").unwrap();
    git(&other, &["add", "n.txt"]);
    git(&other, &["commit", "-qm", "theirs"]);
    git(&other, &["push", "-q"]);
}

fn behind(app: &super::App, path: &std::path::Path) -> u32 {
    app.statuses[path].status.as_ref().unwrap().branch.behind
}

fn notice(app: &super::App) -> String {
    app.notice
        .as_ref()
        .map(|n| n.text.clone())
        .unwrap_or_default()
}

fn grouped(paths: &[&std::path::PathBuf], group: &str) -> RepoStore {
    let mut store = RepoStore::default();
    store.add_group(group).unwrap();
    for p in paths {
        store.add_repo((*p).clone(), Some(group.into()));
    }
    store
}

#[test]
fn ctrl_f_fetches_the_selected_repository_and_holds_push_and_pull() {
    let a = TestRepo::new();
    behind_once_fetched(&a);
    let mut app = scanned_app(store_of(&[&a.root]));
    assert_eq!(behind(&app, &a.root), 0, "not fetched yet");
    app.press(&[ctrl('f')]);
    assert!(app.fetching(&a.root), "row shows fetch…");
    app.press(&[ch('p')]);
    assert!(app.remote_ops.is_empty(), "no pull while fetching");
    assert!(notice(&app).contains("A fetch is still running"));
    app.wait_background(30);
    assert!(!app.fetching(&a.root));
    assert_eq!(behind(&app, &a.root), 1);
    assert!(
        notice(&app).contains("fetched, 1 ref updated"),
        "{}",
        notice(&app)
    );
}

#[test]
fn ctrl_f_on_a_worktree_fetches_its_repository_in_any_pane() {
    let a = TestRepo::new();
    behind_once_fetched(&a);
    let wt = a.root.parent().unwrap().join("wt");
    a.git(&["worktree", "add", "-q", "-b", "wt", wt.to_str().unwrap()]);
    a.git(&["branch", "-q", "--set-upstream-to=origin/main", "wt"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[key(KeyCode::Down)]);
    assert_eq!(app.current_target().as_deref(), Some(wt.as_path()));
    app.focus = Pane::Diff;
    app.press(&[ctrl('f')]);
    assert!(app.fetching(&a.root));
    app.wait_background(30);
    assert_eq!(behind(&app, &wt), 1, "the worktree is rescanned too");
}

#[test]
fn ctrl_f_on_a_group_fetches_its_repositories_in_one_round() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    behind_once_fetched(&a);
    let mut app = scanned_app(grouped(&[&a.root, &b.root], "g"));
    assert!(matches!(
        app.current_row(),
        Some(super::tree::Row::Group { .. })
    ));
    app.press(&[ctrl('f')]);
    assert!(app.fetching(&a.root) && app.fetching(&b.root));
    assert!(notice(&app).contains("Fetching 2 repositories in g"));
    app.wait_background(30);
    assert_eq!(behind(&app, &a.root), 1);
    // `b` has no remote: neither fetched nor failed.
    assert_eq!(notice(&app), "Fetched 1 repository");
}

#[test]
fn a_folded_group_shows_fetch_while_its_repositories_fetch() {
    let a = TestRepo::new();
    behind_once_fetched(&a);
    let mut store = grouped(&[&a.root], "g");
    store.toggle_collapsed(Some("g"));
    let mut app = scanned_app(store);
    app.press(&[ctrl('f')]);
    assert!(shows(&screen(&app, 100, 20), "fetch…"));
    app.wait_background(30);
    let rows = screen(&app, 100, 20);
    assert!(!shows(&rows, "fetch…") && shows(&rows, "↓1"), "{rows:?}");
}

#[test]
fn ctrl_u_rescans_and_fetches_every_repository() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    behind_once_fetched(&a);
    behind_once_fetched(&b);
    let mut app = scanned_app(store_of(&[&a.root, &b.root]));
    app.press(&[ctrl('u')]);
    assert!(notice(&app).contains("Rescanning and fetching 2 repositories"));
    app.wait_background(30);
    assert_eq!((behind(&app, &a.root), behind(&app, &b.root)), (1, 1));
    assert_eq!(notice(&app), "Fetched 2 repositories");
}

#[test]
fn failures_are_summed_up_in_a_round_and_shown_for_one_repository() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    behind_once_fetched(&a);
    let missing = b.root.parent().unwrap().join("missing.git");
    b.git(&["remote", "add", "origin", missing.to_str().unwrap()]);
    let mut app = scanned_app(store_of(&[&a.root, &b.root]));
    app.press(&[ctrl('u')]);
    app.wait_background(30);
    assert_eq!(notice(&app), "Fetch failed in 1 of 2 repositories: repo");
    assert_eq!(app.mode, UiMode::Normal, "a round never opens a dialog");

    app.press(&[key(KeyCode::Down), ctrl('f')]);
    app.wait_background(30);
    assert_eq!(app.mode, UiMode::Message);
    let m = app.message.as_ref().unwrap();
    assert_eq!(m.kind, MessageKind::Error);
    assert!(m.title.contains("Fetch Failed"));
}
