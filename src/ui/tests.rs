//! Cross-feature UI tests driving `App` with keys against real temporary
//! repositories.

use super::test_support::*;
use super::tree::{ChangeView, Row};
use super::{Pane, UiMode};
use crate::git::test_repo::TestRepo;
use crossterm::event::KeyCode;

#[test]
fn arrows_walk_repos_files_and_diff() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    a.write("b.txt", "new\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    assert_eq!(app.rows.len(), 1);
    // Two changed files, then the history (one commit).
    assert_eq!(app.file_items().len(), 3);
    app.wait_background(10);
    let doc = app
        .diff
        .doc
        .as_ref()
        .expect("first file diff loads on selection");
    assert_eq!(doc.added, 1);

    app.press(&[key(KeyCode::Right), key(KeyCode::Down)]);
    assert_eq!(app.focus, Pane::Files);
    assert_eq!(app.file_sel, 1);
    app.wait_background(10);
    assert!(app.diff.key.as_deref().unwrap().ends_with("f:b.txt"));

    app.press(&[key(KeyCode::End), key(KeyCode::Home)]);
    assert_eq!(app.file_sel, 0);
    app.press(&[key(KeyCode::Right), key(KeyCode::Right)]);
    assert_eq!(app.focus, Pane::Diff, "focus stays on the last pane");
    app.press(&[ch('h'), ch('h'), ch('h')]);
    assert_eq!(app.focus, Pane::Repos);
}

#[test]
fn the_file_cursor_stays_on_its_file_across_a_rescan() {
    let a = TestRepo::new();
    a.write("b.txt", "b\n");
    a.write("c.txt", "c\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('2'), key(KeyCode::Down)]);
    assert_eq!(app.file_key.as_deref(), Some("f:c.txt"));
    a.write("0.txt", "sorts first\n");
    app.refresh_all(false);
    app.wait_background(10);
    let files = app
        .current_tree()
        .unwrap()
        .status
        .as_ref()
        .unwrap()
        .files
        .len();
    assert_eq!(files, 3);
    assert_eq!(app.file_key.as_deref(), Some("f:c.txt"));
    assert_eq!(app.file_sel, 2);
}

#[test]
fn filter_selects_the_first_match_and_esc_restores() {
    let a = TestRepo::new();
    let b = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    let b_path = b.root.parent().unwrap().join("other");
    std::fs::rename(&b.root, &b_path).unwrap();
    store.add_repo(b_path.clone(), None);
    let mut app = scanned_app(store);
    // Rows sort by folder name: "other" before "repo".
    assert_eq!(app.current_target().as_deref(), Some(b_path.as_path()));
    app.press(&[ch('/')]);
    app.type_text("rep");
    assert_eq!(app.rows.len(), 1);
    assert_eq!(app.current_target().as_deref(), Some(a.root.as_path()));
    app.press(&[key(KeyCode::Esc)]);
    assert_eq!(app.mode, UiMode::Normal);
    assert!(app.filter.value.is_empty());
    assert_eq!(app.rows.len(), 2);
    app.press(&[ch('/')]);
    app.type_text("zzz");
    app.press(&[key(KeyCode::Enter)]);
    assert!(app.rows.is_empty());
    assert!(app.current_target().is_none());
    app.press(&[ch('0')]);
    assert_eq!(app.rows.len(), 2);
}

#[test]
fn worktrees_nest_under_their_repository_and_can_be_removed() {
    let a = TestRepo::new();
    let wt = a.root.parent().unwrap().join("repo-feat");
    a.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    std::fs::write(wt.join("dirty.txt"), "x").unwrap();
    let mut app = scanned_app(store_of(&[&a.root]));
    assert!(
        matches!(app.rows[1], Row::Worktree { .. }),
        "{:?}",
        app.rows
    );

    app.press(&[key(KeyCode::Down)]);
    assert_eq!(app.current_target().as_deref(), Some(wt.as_path()));
    app.press(&[ctrl('d')]);
    assert_eq!(app.mode, UiMode::Confirm);
    let dialog = app.confirm.as_ref().unwrap();
    assert!(!dialog.ok_focused, "destructive dialogs focus Cancel");
    assert_eq!(dialog.ok_label, "Force Remove");
    // Enter on Cancel does nothing.
    app.press(&[key(KeyCode::Enter)]);
    assert!(wt.exists());

    app.press(&[ctrl('d'), key(KeyCode::Left), key(KeyCode::Enter)]);
    app.wait_background(20);
    assert!(!wt.exists());
    assert_eq!(app.rows.len(), 1);
    assert_eq!(app.current_target().as_deref(), Some(a.root.as_path()));
    assert!(app.diff.error.is_none(), "{:?}", app.diff.error);
}

#[test]
fn commit_dialog_commits_everything_and_rescans() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    a.write("new.txt", "n\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('c')]);
    assert_eq!(app.mode, UiMode::Commit);
    let d = app.commit.as_ref().unwrap();
    assert_eq!(
        d.scope,
        crate::git::ops::CommitScope::All,
        "nothing staged → All"
    );
    // Enter in the message field moves to the buttons instead of submitting.
    app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Commit);
    assert!(
        app.commit.as_ref().unwrap().error.is_some(),
        "empty message is refused"
    );
    app.type_text("feat: everything");
    app.press(&[key(KeyCode::Tab), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Normal);
    app.wait_background(20);
    assert!(app
        .current_tree()
        .unwrap()
        .status
        .as_ref()
        .unwrap()
        .is_clean());
    assert!(app
        .notice
        .as_ref()
        .unwrap()
        .text
        .contains("feat: everything"));
}

#[test]
fn a_clean_repository_has_nothing_to_commit() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('c')]);
    assert_eq!(app.mode, UiMode::Normal);
    assert_eq!(app.notice.as_ref().unwrap().text, "Nothing to commit");
}

#[test]
fn groups_are_created_assigned_folded_and_deleted() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ctrl('n')]);
    app.type_text("work");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
    assert!(app.store.group_exists("work"));
    assert!(matches!(app.current_row(), Some(Row::Group { .. })));

    // Ungrouped repositories lead the list: repository row → Move to Group.
    app.press(&[key(KeyCode::Home), ctrl('e')]);
    assert_eq!(app.mode, UiMode::GroupPick);
    app.press(&[key(KeyCode::Down), key(KeyCode::Enter)]);
    assert_eq!(app.store.repos[0].group.as_deref(), Some("work"));

    app.press(&[key(KeyCode::Home), ch(' ')]);
    assert!(app.store.is_collapsed(Some("work")));
    assert_eq!(app.rows.len(), 1);
    app.press(&[ch(' ')]);
    assert_eq!(app.rows.len(), 2);

    app.press(&[
        key(KeyCode::Home),
        ctrl('d'),
        key(KeyCode::Left),
        key(KeyCode::Enter),
    ]);
    assert!(!app.store.group_exists("work"));
    assert_eq!(app.store.repos[0].group, None);
}

#[test]
fn a_duplicate_group_name_is_reported_in_the_dialog() {
    let a = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    let mut app = scanned_app(store);
    app.press(&[ctrl('n')]);
    app.type_text("work");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::GroupName);
    assert!(app.group_name.as_ref().unwrap().error.is_some());
}

#[test]
fn add_repository_validates_the_folder_then_adds_every_repo_below_a_parent() {
    let a = TestRepo::new();
    let mut app = scanned_app(Default::default());
    app.press(&[ch('+')]);
    assert_eq!(app.mode, UiMode::AddRepo);
    app.type_text("/no/such/folder");
    app.press(&[key(KeyCode::Tab), key(KeyCode::Tab), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::AddRepo);
    assert!(app.add_repo.as_ref().unwrap().error.is_some());

    // A failed submit returns focus to the folder field; a paste replaces it
    // once the old text is cleared.
    assert_eq!(
        app.add_repo.as_ref().unwrap().focus,
        super::overlays::add_repo::AddFocus::Path
    );
    app.add_repo.as_mut().unwrap().path = crate::ui::TextInput::new(String::new());
    app.on_paste(&a.root.parent().unwrap().to_string_lossy());
    app.press(&[key(KeyCode::Tab), key(KeyCode::Tab), key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Normal);
    app.wait_background(20);
    assert_eq!(app.store.repos.len(), 1);
    assert_eq!(app.store.repos[0].path, a.root);
    assert!(app.current_tree().is_some_and(|t| t.status.is_some()));
}

#[test]
fn front_layer_takes_every_key_but_ctrl_c_arms_quit() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch('?')]);
    assert_eq!(app.mode, UiMode::Help);
    app.press(&[ch('2'), ch('+')]);
    assert_eq!(app.mode, UiMode::Help);
    assert_eq!(app.focus, Pane::Repos);
    app.press(&[ctrl('c')]);
    assert!(!app.should_quit);
    app.press(&[ctrl('c')]);
    assert!(app.should_quit);
}

#[test]
fn q_quits_only_when_pressed_twice() {
    let mut app = scanned_app(Default::default());
    app.press(&[ch('q'), ch('j')]);
    assert!(!app.should_quit, "another key disarms");
    app.press(&[ch('q'), ch('q')]);
    assert!(app.should_quit);
}

#[test]
fn palette_runs_commands_and_greys_out_unavailable_ones() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[ch(':')]);
    app.type_text("remove worktree");
    let item = app.quick.as_ref().unwrap().items[0].clone();
    assert_eq!(item.label(), "Remove Worktree");
    assert!(!item.enabled, "no worktree is selected");
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(
        app.mode,
        UiMode::QuickCommand,
        "disabled commands do not run"
    );
    app.press(&[key(KeyCode::Esc), ch(':')]);
    app.type_text("focus diff");
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.focus, Pane::Diff);
}

#[test]
fn removing_a_repository_keeps_its_folder() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    app.press(&[
        key(KeyCode::Delete),
        key(KeyCode::Left),
        key(KeyCode::Enter),
    ]);
    assert!(app.store.repos.is_empty());
    assert!(app.rows.is_empty());
    assert!(a.root.join(".git").exists());
}

#[test]
fn dot_on_the_repo_pane_toggles_the_changed_only_view() {
    let clean = TestRepo::new();
    let dirty = TestRepo::new();
    let dirty_path = dirty.root.parent().unwrap().join("dirty");
    std::fs::rename(&dirty.root, &dirty_path).unwrap();
    std::fs::write(dirty_path.join("a.txt"), "changed\n").unwrap();
    let mut store = store_of(&[&clean.root]);
    store.add_repo(dirty_path.clone(), None);
    let mut app = scanned_app(store);
    assert_eq!(app.rows.len(), 2);

    app.press(&[ch('.')]);
    assert_eq!(app.change_view, ChangeView::Changed);
    assert_eq!(app.rows.len(), 1);
    assert_eq!(app.current_target().as_deref(), Some(dirty_path.as_path()));
    assert_eq!(app.filter_label().as_deref(), Some("changed"));
    let rows = screen(&app, 160, 20);
    assert!(shows(&rows, "Repos[changed: 1/2]"), "{rows:#?}");
    assert!(shows(&rows, "<.>     Pending Only"));

    // `.` elsewhere still toggles line wrap; `0` clears the view.
    app.press(&[key(KeyCode::Right), ch('.')]);
    assert!(app.change_view == ChangeView::Changed && app.diff.wrap);
    app.press(&[ch('0')]);
    assert_eq!(app.change_view, ChangeView::All);
    assert_eq!(app.rows.len(), 2);
}

#[test]
fn dot_cycles_changed_then_pending_which_adds_a_repository_with_a_stash() {
    let clean = TestRepo::new();
    let stashed = TestRepo::new();
    let path = stashed.root.parent().unwrap().join("stashed");
    std::fs::rename(&stashed.root, &path).unwrap();
    std::fs::write(path.join("a.txt"), "changed\n").unwrap();
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "stash", "-q"])
        .status()
        .unwrap()
        .success();
    assert!(ok, "git stash");
    let mut store = store_of(&[&clean.root]);
    store.add_repo(path.clone(), None);
    let mut app = scanned_app(store);

    app.press(&[ch('.')]);
    assert_eq!(app.change_view, ChangeView::Changed);
    assert!(app.rows.is_empty(), "a stash alone is not a change");
    app.press(&[ch('.')]);
    assert_eq!(app.change_view, ChangeView::Pending);
    assert_eq!(app.current_target().as_deref(), Some(path.as_path()));
    assert_eq!(app.filter_label().as_deref(), Some("pending"));
    assert!(shows(&screen(&app, 160, 20), "<.>     Show All"));
    app.press(&[ch('.')]);
    assert_eq!(app.change_view, ChangeView::All);
    assert_eq!(app.rows.len(), 2);
}

#[test]
fn right_is_blocked_on_a_group_header() {
    let a = TestRepo::new();
    let mut store = store_of(&[&a.root]);
    store.add_group("work").unwrap();
    store.set_group(&a.root, Some("work".into()));
    let mut app = scanned_app(store);
    assert!(matches!(app.current_row(), Some(Row::Group { .. })));
    app.press(&[key(KeyCode::Right), ch('l')]);
    assert_eq!(app.focus, Pane::Repos);
    let rows = screen(&app, 120, 20);
    let title = rows.iter().find(|r| r.contains("Repos[1]")).unwrap();
    assert!(!title.contains("→"), "no arrow toward Files: {title}");
    // On the repository row it moves on, and the arrow is back.
    app.press(&[key(KeyCode::Down)]);
    let rows = screen(&app, 120, 20);
    assert!(rows
        .iter()
        .find(|r| r.contains("Repos[1]"))
        .unwrap()
        .contains("→"));
    app.press(&[key(KeyCode::Right)]);
    assert_eq!(app.focus, Pane::Files);
}

#[test]
fn discard_file_changes_runs_from_the_palette_after_a_confirmation() {
    let a = TestRepo::new();
    a.write("a.txt", "edited\n");
    a.write("new.txt", "n\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    app.press(&[ch('2')]);
    assert_eq!(app.file_key.as_deref(), Some("f:a.txt"));
    app.press(&[ch(':')]);
    app.type_text("discard");
    let item = app.quick.as_ref().unwrap().items[0].clone();
    assert_eq!(item.label(), "Discard File Changes");
    assert!(item.enabled && item.shortcut().is_none());
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(app.mode, UiMode::Confirm);
    let d = app.confirm.as_ref().unwrap();
    assert!(!d.ok_focused, "Cancel is focused");
    assert!(d.lines[0].contains("a.txt"));
    // Enter on Cancel keeps the change.
    app.press(&[key(KeyCode::Enter)]);
    assert_eq!(
        std::fs::read_to_string(a.root.join("a.txt")).unwrap(),
        "edited\n"
    );

    app.press(&[ch(':')]);
    app.type_text("discard");
    app.press(&[key(KeyCode::Enter), key(KeyCode::Left), key(KeyCode::Enter)]);
    app.wait_background(20);
    assert_eq!(
        std::fs::read_to_string(a.root.join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    let files = &app.current_tree().unwrap().status.as_ref().unwrap().files;
    assert_eq!(files.len(), 1, "only new.txt is left");

    // On a history commit the command is unavailable.
    app.press(&[key(KeyCode::End)]);
    app.press(&[ch(':')]);
    app.type_text("discard");
    assert!(!app.quick.as_ref().unwrap().items[0].enabled);
}
