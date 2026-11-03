use super::test_support::{ctrl, key, store_of};
use super::{App, RowKey, UiMode};
use crate::config::Config;
use crate::git::test_repo::TestRepo;
use crate::store::RepoStore;
use crossterm::event::KeyCode;
use std::time::{Duration, Instant};

fn app_at(path: &std::path::Path) -> App {
    let mut app = App::with_store(
        Config {
            refresh_interval_secs: 0,
            ..Config::default()
        },
        RepoStore::load(path).unwrap(),
        Some(path.to_path_buf()),
    );
    app.rebuild_rows();
    app
}

fn until(app: &mut App, condition: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick(Instant::now());
        app.poll_background();
        if condition(app) {
            return;
        }
        assert!(Instant::now() < deadline, "store did not converge");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn two_instances_replay_stale_changes_and_sync_without_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let mut a = app_at(&path);
    let mut b = app_at(&path);
    // Both requests start from an empty view; neither may overwrite the other.
    for (app, name) in [(&mut a, "alpha"), (&mut b, "beta")] {
        app.change_store(
            move |store| store.add_group(name).map_err(anyhow::Error::msg),
            |_, ()| {},
        );
    }
    a.wait_background(10);
    b.wait_background(10);
    until(&mut a, |app| app.store.group_exists("beta"));
    until(&mut b, |app| app.store.group_exists("alpha"));
    assert_eq!(a.store, b.store);
    // A stale picker must not resurrect a group removed by the other instance.
    a.change_store(
        |store| {
            store.delete_group("alpha");
            Ok(())
        },
        |_, ()| {},
    );
    a.wait_background(10);
    b.change_store(
        |store| {
            store
                .rename_group("alpha", "resurrected")
                .map_err(anyhow::Error::msg)
        },
        |_, ()| panic!("conflict reported as success"),
    );
    b.wait_background(10);
    assert!(!RepoStore::read_existing(&path)
        .unwrap()
        .group_exists("resurrected"));
    assert!(!b.store.group_exists("alpha"));
}

#[test]
fn refresh_keeps_selection_filters_and_dialog_input() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    store_of(&[&a.root]).save(&path).unwrap();
    let mut app = app_at(&path);
    app.wait_background(10);
    let selected = app.current_target();
    RepoStore::update(&path, false, |store| {
        store.add_repo(b.root.clone(), None);
        store.add_group("external").map_err(anyhow::Error::msg)
    })
    .unwrap();
    app.on_key(ctrl('u'));
    app.wait_background(10);
    assert!(app.store.contains(&b.root));
    assert_eq!(app.current_target(), selected);
    app.filter.value = a.root.file_name().unwrap().to_string_lossy().into_owned();
    app.rebuild_rows_for_filter();
    app.open_new_group();
    app.type_text("unfinished");
    let filter = app.filter.value.clone();
    RepoStore::update(&path, false, |store| {
        store.add_group("another").map_err(anyhow::Error::msg)
    })
    .unwrap();
    until(&mut app, |app| app.store.group_exists("another"));
    assert_eq!(app.mode, UiMode::GroupName);
    assert_eq!(app.group_name.as_ref().unwrap().input.value, "unfinished");
    assert_eq!(app.filter.value, filter);
    assert_eq!(app.current_target(), selected);
    app.on_key(key(KeyCode::Esc));
    app.filter.value.clear();
    app.rebuild_rows_for_filter();
    app.select_row_key(Some(&RowKey::Repo(a.root.clone())));
    RepoStore::update(&path, false, |store| {
        store.remove_repo(&a.root);
        Ok(())
    })
    .unwrap();
    app.reload_store();
    app.wait_background(10);
    assert_eq!(app.current_target(), Some(b.root));
    assert!(!app.statuses.contains_key(&a.root));
}

#[test]
fn malformed_or_deleted_list_preserves_view_and_recovers_after_repair() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    let mut store = RepoStore::default();
    store.add_group("keep").unwrap();
    store.save(&path).unwrap();
    let mut app = app_at(&path);
    app.reload_store();
    app.wait_background(10);
    std::fs::write(&path, "{").unwrap();
    app.reload_store();
    app.wait_background(10);
    assert_eq!(app.store, store);
    app.change_store(
        |s| s.add_group("lost").map_err(anyhow::Error::msg),
        |_, ()| panic!("invalid JSON accepted"),
    );
    app.wait_background(10);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
    std::fs::remove_file(&path).unwrap();
    app.change_store(
        |s| s.add_group("lost").map_err(anyhow::Error::msg),
        |_, ()| panic!("missing file accepted"),
    );
    app.wait_background(10);
    assert_eq!(app.store, store);
    assert!(!path.exists());
    store.add_group("repaired").unwrap();
    store.save(&path).unwrap();
    until(&mut app, |app| app.store.group_exists("repaired"));
}

#[test]
fn queued_operations_preserve_order_and_do_not_block_on_file_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let lock = std::fs::File::create(path.with_extension("lock")).unwrap();
    lock.lock().unwrap();
    let mut app = app_at(&path);
    let start = Instant::now();
    app.change_store(
        |s| s.add_group("first").map_err(anyhow::Error::msg),
        |_, ()| {},
    );
    app.change_store(
        |s| {
            s.rename_group("first", "second")
                .map_err(anyhow::Error::msg)
        },
        |_, ()| {},
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(app.store_pending());
    drop(lock);
    app.wait_background(10);
    assert!(app.store.group_exists("second"));
    assert!(!app.store.group_exists("first"));
    assert!(!app.store_pending());
}

#[test]
fn rapid_fold_keys_preserve_each_requested_operation() {
    for keys in [
        [KeyCode::Enter, KeyCode::Enter],
        [KeyCode::Left, KeyCode::Right],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repos.json");
        let mut store = RepoStore::default();
        store.add_group("work").unwrap();
        store.save(&path).unwrap();
        let mut app = app_at(&path);
        let lock = std::fs::File::create(path.with_extension("lock")).unwrap();
        lock.lock().unwrap();
        for code in keys {
            app.on_key(key(code));
        }
        drop(lock);
        app.wait_background(10);
        assert!(
            !app.store.groups[0].collapsed,
            "final fold state for {keys:?}"
        );
    }
}

#[test]
fn initially_missing_list_reloads_quietly_and_checks_startup_gap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    let mut app = app_at(&path);
    app.on_key(ctrl('u'));
    app.wait_background(10);
    app.refresh_all(true);
    app.wait_background(10);
    assert!(!app
        .notice
        .as_ref()
        .is_some_and(|n| n.level == super::Level::Error));
    assert!(!path.exists());
    // Simulate a peer creating the list after initial load, before watch registration.
    let mut disk = RepoStore::default();
    disk.add_group("peer").unwrap();
    disk.save(&path).unwrap();
    let mut stale = App::with_store(Config::default(), RepoStore::default(), Some(path));
    until(&mut stale, |app| app.store.group_exists("peer"));
}

#[test]
fn malformed_startup_can_be_reset_without_overwriting_invalid_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    std::fs::write(&path, "{").unwrap();
    let mut app = App::with_store(Config::default(), RepoStore::default(), Some(path.clone()));
    app.store_sync.set_initially_loaded(false);
    app.reload_store();
    app.wait_background(10);
    assert!(app
        .notice
        .as_ref()
        .unwrap()
        .text
        .contains(&path.display().to_string()));
    app.change_store(
        |s| s.add_group("recovery").map_err(anyhow::Error::msg),
        |_, ()| panic!("invalid JSON overwritten"),
    );
    app.wait_background(10);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
    std::fs::remove_file(&path).unwrap();
    app.change_store(
        |s| s.add_group("recovery").map_err(anyhow::Error::msg),
        |_, ()| {},
    );
    app.wait_background(10);
    assert!(RepoStore::read_existing(&path)
        .unwrap()
        .group_exists("recovery"));
}

#[test]
fn peer_fold_or_move_preserves_selected_repository_and_diff() {
    let repo = TestRepo::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    let mut store = store_of(&[&repo.root]);
    store.add_group("work").unwrap();
    store.add_group("folded").unwrap();
    store.groups[1].collapsed = true;
    store.set_group(&repo.root, Some("work".into()));
    store.save(&path).unwrap();
    let mut app = app_at(&path);
    app.select_row_key(Some(&RowKey::Repo(repo.root.clone())));
    app.scan_all();
    app.wait_background(10);
    app.diff.scroll.set(17);
    RepoStore::update(&path, false, |s| {
        s.toggle_collapsed(Some("work"));
        Ok(())
    })
    .unwrap();
    app.reload_store();
    app.wait_background(10);
    assert_eq!(app.current_target(), Some(repo.root.clone()));
    assert_eq!(app.diff.scroll.get(), 17);
    assert!(app.store.is_collapsed(Some("work")));
    RepoStore::update(&path, false, |s| {
        s.set_group(&repo.root, Some("folded".into()));
        Ok(())
    })
    .unwrap();
    app.reload_store();
    app.wait_background(10);
    assert_eq!(app.current_target(), Some(repo.root.clone()));
    assert_eq!(app.diff.scroll.get(), 17);
    app.on_key(key(KeyCode::Up));
    assert!(app
        .rows
        .iter()
        .all(|row| row.target() != Some(repo.root.as_path())));
}

#[test]
fn peer_fold_preserves_selected_worktree_and_parent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    let mut store = RepoStore::default();
    store.add_group("work").unwrap();
    let repo = dir.path().join("main");
    let wt = dir.path().join("linked");
    store.add_repo(repo.clone(), Some("work".into()));
    store.save(&path).unwrap();
    let mut app = app_at(&path);
    let status = crate::git::RepoStatus {
        worktrees: vec![crate::git::Worktree {
            path: wt.clone(),
            branch: Some("topic".into()),
            locked: false,
            prunable: false,
        }],
        ..Default::default()
    };
    app.statuses
        .insert(repo.clone(), super::scan::TreeState::loaded(status, 1));
    app.rebuild_rows();
    app.select_row_key(Some(&RowKey::Worktree(wt.clone())));
    app.wait_background(10);
    app.diff.scroll.set(11);
    RepoStore::update(&path, false, |s| {
        s.toggle_collapsed(Some("work"));
        Ok(())
    })
    .unwrap();
    app.reload_store();
    app.wait_background(10);
    assert_eq!(app.current_target(), Some(wt.clone()));
    assert_eq!(app.diff.scroll.get(), 11);
    assert!(app
        .rows
        .iter()
        .any(|row| row.target() == Some(repo.as_path())));
}

#[test]
fn concurrent_name_conflict_keeps_the_actual_dialog_input() {
    for rename in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repos.json");
        let mut store = RepoStore::default();
        if rename {
            store.add_group("original").unwrap();
        }
        store.save(&path).unwrap();
        let mut app = app_at(&path);
        if rename {
            app.on_key(ctrl('e'));
        } else {
            app.on_key(ctrl('n'));
        }
        app.type_text("taken");
        RepoStore::update(&path, false, |s| {
            s.add_group("taken").map_err(anyhow::Error::msg)
        })
        .unwrap();
        app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
        assert!(app.group_name.as_ref().unwrap().pending);
        // A repeated submit while saving must not enqueue a second operation.
        app.on_key(key(KeyCode::Enter));
        app.wait_background(10);
        assert_eq!(app.mode, UiMode::GroupName);
        let dialog = app.group_name.as_ref().unwrap();
        assert!(!dialog.pending);
        assert_eq!(dialog.input.value, "taken");
        assert!(dialog.error.as_ref().unwrap().contains("already exists"));
        assert_eq!(
            RepoStore::read_existing(&path).unwrap().groups.len(),
            if rename { 2 } else { 1 }
        );
    }
}

#[test]
fn accepted_add_discovery_and_write_finish_after_quit_is_requested() {
    let repo = TestRepo::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let mut app = app_at(&path);
    app.open_add_repo();
    app.type_text(&repo.root.to_string_lossy());
    app.press(&[key(KeyCode::Tab), key(KeyCode::Tab), key(KeyCode::Enter)]);
    assert_eq!(app.store_sync.discoveries, 1);
    assert!(app.store_pending());
    app.should_quit = true;
    app.wait_background(10);
    assert!(!app.store_pending());
    assert!(RepoStore::read_existing(&path)
        .unwrap()
        .contains(&repo.root));
}

#[test]
fn group_save_keeps_an_intervening_error_visible_until_dismissed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let mut app = app_at(&path);
    let lock = std::fs::File::create(path.with_extension("lock")).unwrap();
    lock.lock().unwrap();
    app.open_new_group();
    app.type_text("saved");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
    app.show_message(
        "Push failed",
        vec!["Failure detail".into()],
        super::overlays::message::MessageKind::Error,
    );
    drop(lock);
    app.wait_background(10);
    assert!(app.store.group_exists("saved"));
    assert!(app.group_name.is_none());
    assert_eq!(app.mode, UiMode::Message);
    assert_eq!(app.message.as_ref().unwrap().title, "Push failed");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.mode, UiMode::Normal);
}

#[test]
fn quit_shares_one_lock_wait_budget_across_queued_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let lock = std::fs::File::create(path.with_extension("lock")).unwrap();
    lock.lock().unwrap();
    let mut app = app_at(&path);
    app.should_quit = true;
    app.prepare_store_quit();
    let started = Instant::now();
    for name in ["one", "two", "three"] {
        app.change_store(
            move |s| s.add_group(name).map_err(anyhow::Error::msg),
            |_, ()| panic!("locked write succeeded"),
        );
    }
    app.wait_background(8);
    assert!(started.elapsed() < Duration::from_secs(7));
    assert!(!app.store_pending());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn local_fold_all_selects_header_and_hides_selected_repository() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    let repo = TestRepo::new();
    let mut store = store_of(&[&repo.root]);
    store.add_group("work").unwrap();
    store.set_group(&repo.root, Some("work".into()));
    store.save(&path).unwrap();
    let mut app = app_at(&path);
    app.select_row_key(Some(&RowKey::Repo(repo.root.clone())));
    app.filter.value = "repo".into();
    app.open_quick_command();
    app.type_text("Fold All Groups");
    assert!(!app.quick.as_ref().unwrap().items[0].enabled);
    app.on_key(key(KeyCode::Esc));
    app.filter.value.clear();
    app.open_quick_command();
    app.type_text("Fold All Groups");
    app.on_key(key(KeyCode::Enter));
    app.wait_background(10);
    assert!(app.store.is_collapsed(Some("work")));
    assert!(matches!(
        app.current_row(),
        Some(super::tree::Row::Group { .. })
    ));
    assert!(app
        .rows
        .iter()
        .all(|row| row.target() != Some(repo.root.as_path())));
}
