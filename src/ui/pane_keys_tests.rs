//! Pane-scoped keys: a key that is not common works only in a pane whose
//! header lists it, and every pane header lists only keys that work there.

use super::keys;
use super::test_support::*;
use super::{Pane, UiMode};
use crate::git::test_repo::TestRepo;
use crossterm::event::KeyCode;

fn header_keys(app: &super::App) -> Vec<&'static str> {
    let [a, b] = app.pane_hints();
    a.into_iter().chain(b).map(|(k, _)| k).collect()
}

#[test]
fn repo_list_keys_work_only_in_the_repos_pane() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    for pane in [Pane::Files, Pane::Diff] {
        app.focus = pane;
        app.press(&[ch('+')]);
        assert_eq!(app.mode, UiMode::Normal, "`+` on {pane:?}");
        app.press(&[ch('/')]);
        assert_eq!(app.mode, UiMode::Normal, "`/` on {pane:?}");
        let keys = header_keys(&app);
        assert!(!keys.contains(&"+") && !keys.contains(&"/"), "{keys:?}");
    }
    app.focus = Pane::Repos;
    assert!(header_keys(&app).contains(&"+"));
    app.press(&[ch('+')]);
    assert_eq!(app.mode, UiMode::AddRepo);
}

#[test]
fn zero_takes_the_filter_row_while_a_filter_is_applied() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    assert!(header_keys(&app).contains(&"/"));
    assert!(!header_keys(&app).contains(&"0"));
    app.press(&[ch('.')]);
    assert!(app.pending_only);
    let keys = header_keys(&app);
    assert!(keys.contains(&"0") && !keys.contains(&"/"), "{keys:?}");
    // Help still lists `/` beside the header keys.
    assert!(app.pane_help_only().iter().any(|(k, _)| *k == "/"));
    app.focus = Pane::Files;
    app.press(&[ch('0')]);
    assert!(app.pending_only, "`0` does nothing outside the Repos pane");
    app.focus = Pane::Repos;
    app.press(&[ch('0')]);
    assert!(!app.pending_only);
}

#[test]
fn view_keys_work_only_where_their_header_shows_them() {
    let a = TestRepo::new();
    let mut app = scanned_app(store_of(&[&a.root]));
    let split = app.diff.split;
    for pane in [Pane::Repos, Pane::Files] {
        app.focus = pane;
        app.press(&[ch('s')]);
        assert_eq!(app.diff.split, split, "`s` on {pane:?}");
    }
    app.focus = Pane::Diff;
    assert!(header_keys(&app).contains(&"s"));
    app.press(&[ch('s')]);
    assert_ne!(app.diff.split, split);

    let labels = app.history.labels;
    for pane in [Pane::Repos, Pane::Diff] {
        app.focus = pane;
        app.press(&[ch('>')]);
        assert_eq!(app.history.labels, labels, "`>` on {pane:?}");
        assert!(!header_keys(&app).contains(&">"));
    }
    app.focus = Pane::Files;
    assert!(header_keys(&app).contains(&">"));
    app.press(&[ch('>')]);
    assert_ne!(app.history.labels, labels);

    // `.` wraps lines only on the diff; on Files it cycles date / author.
    let meta = app.history.meta;
    app.press(&[ch('.')]);
    assert!(!app.diff.wrap);
    assert_ne!(app.history.meta, meta);
    app.focus = Pane::Diff;
    app.press(&[ch('.')]);
    assert!(app.diff.wrap);
}

#[test]
fn common_keys_never_repeat_a_pane_key() {
    let a = TestRepo::new();
    a.write("a.txt", "changed\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    let common: Vec<_> = keys::common().into_iter().map(|(k, _)| k).collect();
    for pane in Pane::ALL {
        app.focus = pane;
        for k in header_keys(&app) {
            assert!(!common.contains(&k), "`{k}` is both common and on {pane:?}");
        }
    }
    // Commit, push, and pull stay in every pane header.
    for pane in Pane::ALL {
        app.focus = pane;
        let keys = header_keys(&app);
        for k in ["c", "P", "p"] {
            assert!(keys.contains(&k), "`{k}` missing on {pane:?}");
        }
    }
    app.focus = Pane::Diff;
    app.press(&[key(KeyCode::Char('c'))]);
    assert_eq!(app.mode, UiMode::Commit);
}

#[test]
fn alt_arrows_reorder_groups_and_repositories_but_not_while_filtered() {
    use crate::store::RepoStore;
    use crossterm::event::{KeyEvent, KeyModifiers};
    let alt = |code| KeyEvent::new(code, KeyModifiers::ALT);
    let mut store = RepoStore::default();
    for g in ["g1", "g2"] {
        store.add_group(g).unwrap();
    }
    for (p, g) in [("/x/a", None), ("/x/b", None), ("/x/c", Some("g1"))] {
        store.add_repo(p.into(), g.map(Into::into));
    }
    let mut app = super::App::with_store(crate::config::Config::default(), store, None);
    app.rebuild_rows();
    let order = |app: &super::App| -> Vec<String> {
        let groups = app.store.groups.iter().map(|g| g.name.clone());
        groups
            .chain(app.store.repos.iter().map(|r| r.name()))
            .collect()
    };
    // `b` up past `a`; the cursor stays on `b`.
    app.press(&[key(KeyCode::Down), alt(KeyCode::Up)]);
    assert_eq!(order(&app), ["g1", "g2", "b", "a", "c"]);
    assert_eq!(app.current_target().as_deref(), Some("/x/b".as_ref()));
    // `a` is last of the ungrouped ones: it does not cross into `g1`.
    app.press(&[key(KeyCode::Down), alt(KeyCode::Down)]);
    assert_eq!(order(&app), ["g1", "g2", "b", "a", "c"]);
    // The `g1` header moves below `g2`.
    app.press(&[key(KeyCode::Down), alt(KeyCode::Down)]);
    assert_eq!(order(&app), ["g2", "g1", "b", "a", "c"]);
    // Nothing moves while a filter hides `a`.
    app.press(&[ch('/')]);
    app.type_text("b");
    app.press(&[key(KeyCode::Enter), alt(KeyCode::Down)]);
    assert_eq!(order(&app), ["g2", "g1", "b", "a", "c"]);
}
