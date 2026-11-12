use super::OpenTarget;
use crate::git::test_repo::TestRepo;
use crate::store::RepoStore;
use crate::ui::test_support::*;
use crate::ui::tree::{Row, RowKey};
use crate::ui::{App, UiMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

fn open(repo: &TestRepo) -> OpenTarget {
    OpenTarget {
        repo: repo.root.clone(),
        tree: repo.root.clone(),
    }
}

/// `listed` in the list (scanned), then `ular-git <unlisted>`.
fn opened_unlisted(listed: &RepoStore, unlisted: &TestRepo) -> App {
    let mut app = scanned_app(listed.clone());
    app.open_tree(open(unlisted));
    app.scan_all();
    app.wait_background(20);
    app
}

fn repo_paths(app: &App) -> Vec<PathBuf> {
    app.rows
        .iter()
        .filter_map(|r| match r {
            Row::Repo { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_unlisted_repository_shows_first_marked_and_selected_but_is_not_saved() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let app = opened_unlisted(&store_of(&[&a.root]), &b);
    assert_eq!(repo_paths(&app), [b.root.clone(), a.root.clone()]);
    assert_eq!(app.current_target(), Some(b.root.clone()));
    assert!(
        app.statuses[&b.root].status.is_some(),
        "scanned like any other"
    );
    assert!(!app.listed_store().contains(&b.root));
    assert!(app
        .notice
        .as_ref()
        .unwrap()
        .text
        .contains("Not in the list"));
    let rows = screen(&app, 100, 12);
    assert!(shows(&rows, "*repo"), "{rows:#?}");
    assert_eq!(rows.iter().filter(|r| r.contains("*repo")).count(), 1);
}

#[test]
fn a_listed_repository_in_a_folded_group_is_selected_without_unfolding_it() {
    let a = TestRepo::new();
    let g = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("g").unwrap();
    store.add_repo(g.root.clone(), Some("g".into()));
    store.toggle_collapsed(Some("g"));
    let mut app = scanned_app(store);
    app.open_tree(open(&g));
    assert_eq!(app.current_target(), Some(g.root.clone()));
    assert!(app.unlisted.is_none());
    assert!(app.store.is_collapsed(Some("g")), "shared fold state kept");
    app.rebuild_rows();
    assert_eq!(app.current_target(), Some(g.root.clone()));
    assert!(!shows(&screen(&app, 100, 12), "*repo"));
}

#[test]
fn an_opened_linked_worktree_is_selected_once_the_scan_lists_it() {
    let a = TestRepo::new();
    let wt = a.root.parent().unwrap().join("repo-feat");
    a.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let mut app = App::with_store(Default::default(), store_of(&[&a.root]), None);
    app.rebuild_rows();
    app.open_tree(OpenTarget {
        repo: a.root.clone(),
        tree: wt.clone(),
    });
    // No status yet: the repository row holds the cursor until the scan.
    assert_eq!(app.current_target(), Some(a.root.clone()));
    app.scan_all();
    app.wait_background(20);
    assert_eq!(app.current_target(), Some(wt.clone()));
    assert!(app.open_pending.is_none());
}

#[test]
fn plus_adds_the_unlisted_repository_where_it_was_shown() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let mut app = opened_unlisted(&store_of(&[&a.root]), &b);
    app.open_add_repo();
    let typed = crate::config::expand_home(&app.add_repo.as_ref().unwrap().path.value);
    assert_eq!(typed, b.root, "prefilled with the opened repository");
    app.press(&[key(KeyCode::Tab), key(KeyCode::Tab), key(KeyCode::Enter)]);
    app.wait_background(20);
    assert!(app.unlisted.is_none());
    assert_eq!(app.store.repos[0].path, b.root);
    assert_eq!(repo_paths(&app), [b.root.clone(), a.root.clone()]);
    assert_eq!(app.current_target(), Some(b.root.clone()));
    assert!(!shows(&screen(&app, 100, 12), "*repo"));
}

#[test]
fn ctrl_e_adds_the_unlisted_repository_to_a_group() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("g").unwrap();
    let mut app = opened_unlisted(&store, &b);
    app.press(&[ctrl('e')]);
    assert_eq!(app.mode, UiMode::GroupPick);
    app.press(&[key(KeyCode::Down), key(KeyCode::Enter)]);
    assert!(app.unlisted.is_none());
    assert_eq!(app.store.repos.len(), 2);
    let entry = app.store.repos.iter().find(|r| r.path == b.root).unwrap();
    assert_eq!(entry.group.as_deref(), Some("g"));
    assert_eq!(app.notice.as_ref().unwrap().text, "Added to g");
    assert_eq!(
        app.current_row().map(|r| r.key()),
        Some(RowKey::Repo(b.root.clone()))
    );
}

#[test]
fn ctrl_e_new_group_adds_the_unlisted_repository_to_it() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let mut app = opened_unlisted(&store_of(&[&a.root]), &b);
    // Ungrouped, then `[NEW GROUP]`.
    app.press(&[ctrl('e'), key(KeyCode::Down), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::GroupName);
    app.type_text("new");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
    assert!(app.unlisted.is_none());
    let entry = app.store.repos.iter().find(|r| r.path == b.root).unwrap();
    assert_eq!(entry.group.as_deref(), Some("new"));
}

#[test]
fn ctrl_d_hides_and_alt_arrows_leave_the_unlisted_repository_alone() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let mut app = opened_unlisted(&store_of(&[&a.root]), &b);
    app.press(&[KeyEvent::new(KeyCode::Down, KeyModifiers::ALT)]);
    assert_eq!(repo_paths(&app), [b.root.clone(), a.root.clone()]);
    assert!(app
        .notice
        .as_ref()
        .unwrap()
        .text
        .contains("no place to move"));
    app.press(&[ctrl('d')]);
    assert_eq!(app.mode, UiMode::Normal, "nothing to confirm");
    assert!(app.unlisted.is_none());
    assert_eq!(repo_paths(&app), std::slice::from_ref(&a.root));
    assert!(!app.statuses.contains_key(&b.root));
    assert_eq!(app.current_target(), Some(a.root.clone()));
}

#[test]
fn reloads_keep_the_unlisted_repository_until_the_file_lists_it() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let c = TestRepo::new();
    let mut app = opened_unlisted(&store_of(&[&a.root]), &b);
    // Another instance adds `c`; `b` stays first and unsaved.
    app.accept_store(store_of(&[&a.root, &c.root]));
    assert_eq!(app.store.repos[0].path, b.root);
    assert!(!app.listed_store().contains(&b.root));
    assert_eq!(app.current_target(), Some(b.root.clone()));
    // Another instance lists `b` in a group: it is an ordinary row now.
    let mut store = store_of(&[&a.root, &c.root]);
    store.add_group("g").unwrap();
    store.add_repo(b.root.clone(), Some("g".into()));
    app.accept_store(store.clone());
    assert!(app.unlisted.is_none());
    assert_eq!(app.store, store);
    assert_eq!(app.current_target(), Some(b.root.clone()));
}
