//! Repository pane row tests: groups, nesting, filter, folding, and the
//! pending-only view.

use super::*;
use crate::git::{RepoStatus, Worktree};

fn store() -> RepoStore {
    let mut s = RepoStore::default();
    s.add_group("work").unwrap();
    s.add_repo("/x/alpha".into(), Some("work".into()));
    s.add_repo("/x/zeta".into(), Some("work".into()));
    s.add_repo("/x/loose".into(), None);
    s
}

fn with_worktree(path: &str, branch: &str) -> HashMap<PathBuf, TreeState> {
    let status = RepoStatus {
        worktrees: vec![Worktree {
            path: path.into(),
            branch: Some(branch.into()),
            locked: false,
            prunable: false,
        }],
        ..RepoStatus::default()
    };
    let mut m = HashMap::new();
    m.insert(PathBuf::from("/x/alpha"), TreeState::loaded(status, 0));
    m
}

fn names(store: &RepoStore, rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|r| match r {
            Row::Group { name, .. } => format!("#{}", name.as_deref().unwrap_or("-")),
            Row::Repo { idx, .. } => store.repos[*idx].name(),
            Row::Worktree { path, .. } => format!("^{}", folder_name(path)),
        })
        .collect()
}

#[test]
fn groups_list_repositories_and_nest_worktrees() {
    let s = store();
    let rows = build_rows(&s, &with_worktree("/x/alpha-wt", "feat"), "", false, None);
    assert_eq!(
        names(&s, &rows),
        ["loose", "#work", "alpha", "^alpha-wt", "zeta"]
    );
}

#[test]
fn ungrouped_repositories_lead_without_header_or_indent() {
    let s = store();
    let rows = build_rows(&s, &HashMap::new(), "", false, None);
    assert!(matches!(rows[0], Row::Repo { nested: false, .. }));
    assert!(matches!(rows[2], Row::Repo { nested: true, .. }));
    assert!(rows
        .iter()
        .all(|r| !matches!(r, Row::Group { name: None, .. })));
}

#[test]
fn flat_without_groups_and_no_ungrouped_header_in_store_order() {
    let mut s = RepoStore::default();
    s.add_repo("/b".into(), None);
    s.add_repo("/a".into(), None);
    let rows = build_rows(&s, &HashMap::new(), "", false, None);
    assert_eq!(names(&s, &rows), ["b", "a"]);
    assert!(matches!(rows[0], Row::Repo { nested: false, .. }));
}

#[test]
fn collapsed_groups_hide_their_rows_but_a_filter_opens_them() {
    let mut s = store();
    s.toggle_collapsed(Some("work"));
    let st = with_worktree("/x/alpha-wt", "feat");
    assert_eq!(
        names(&s, &build_rows(&s, &st, "", false, None)),
        ["loose", "#work"]
    );
    assert_eq!(
        names(&s, &build_rows(&s, &st, "ZET", false, None)),
        ["#work", "zeta"]
    );
}

#[test]
fn a_worktree_branch_match_keeps_its_repository() {
    let s = store();
    let rows = build_rows(
        &s,
        &with_worktree("/x/alpha-wt", "feat"),
        "feat",
        false,
        None,
    );
    assert_eq!(names(&s, &rows), ["#work", "alpha", "^alpha-wt"]);
}

#[test]
fn empty_named_groups_stay_visible() {
    let mut s = store();
    s.add_group("empty").unwrap();
    let rows = build_rows(&s, &HashMap::new(), "", false, None);
    assert!(names(&s, &rows).contains(&"#empty".to_string()));
}

#[test]
fn group_counts_sum_files_and_commits_of_repositories_and_worktrees() {
    let s = store();
    let file = |p: &str| crate::git::FileEntry {
        path: p.into(),
        orig_path: None,
        x: '.',
        y: 'M',
        kind: crate::git::EntryKind::Tracked,
    };
    let mut st = with_worktree("/x/alpha-wt", "feat");
    st.get_mut(&PathBuf::from("/x/alpha"))
        .unwrap()
        .status
        .as_mut()
        .unwrap()
        .files = vec![file("a"), file("b")];
    let mut wt = RepoStatus {
        files: vec![file("c")],
        ..RepoStatus::default()
    };
    (wt.branch.ahead, wt.branch.behind) = (2, 1);
    st.insert(PathBuf::from("/x/alpha-wt"), TreeState::loaded(wt, 0));
    let mut zeta = RepoStatus::default();
    zeta.branch.ahead = 1;
    st.insert(PathBuf::from("/x/zeta"), TreeState::loaded(zeta, 0));
    let rows = build_rows(&s, &st, "", false, None);
    assert!(
        matches!(
            rows[1],
            Row::Group {
                changed: 3,
                ahead: 3,
                behind: 1,
                total: 2,
                ..
            }
        ),
        "{:?}",
        rows[1]
    );
}

#[test]
fn pending_only_keeps_dirty_trees_and_the_parent_of_a_dirty_worktree() {
    let s = store();
    let file = crate::git::FileEntry {
        path: "f".into(),
        orig_path: None,
        x: '.',
        y: 'M',
        kind: crate::git::EntryKind::Tracked,
    };
    let mut st = with_worktree("/x/alpha-wt", "feat");
    let dirty = RepoStatus {
        files: vec![file],
        ..RepoStatus::default()
    };
    // alpha is clean but its worktree is dirty; zeta is dirty; loose is clean.
    st.insert(
        PathBuf::from("/x/alpha-wt"),
        TreeState::loaded(dirty.clone(), 0),
    );
    st.insert(PathBuf::from("/x/zeta"), TreeState::loaded(dirty, 0));
    let mut folded = s.clone();
    folded.toggle_collapsed(Some("work"));
    let rows = build_rows(&folded, &st, "", true, None);
    assert_eq!(names(&s, &rows), ["#work", "alpha", "^alpha-wt", "zeta"]);
    // Combined with a text filter, both must match.
    assert_eq!(
        names(&s, &build_rows(&s, &st, "zet", true, None)),
        ["#work", "zeta"]
    );
    assert!(build_rows(&s, &st, "loose", true, None).is_empty());
}

#[test]
fn pending_only_includes_trees_behind_or_ahead_of_their_upstream() {
    let s = store();
    let mut st = HashMap::new();
    let mut behind = RepoStatus::default();
    behind.branch.behind = 2;
    let mut ahead = RepoStatus::default();
    ahead.branch.ahead = 1;
    st.insert(PathBuf::from("/x/loose"), TreeState::loaded(behind, 0));
    st.insert(PathBuf::from("/x/zeta"), TreeState::loaded(ahead, 0));
    // Behind (something to pull) and ahead (something to push) count.
    assert_eq!(
        names(&s, &build_rows(&s, &st, "", true, None)),
        ["loose", "#work", "zeta"]
    );
}

#[test]
fn pending_only_counts_a_stash_on_the_repository_row_only() {
    let s = store();
    let mut st = with_worktree("/x/alpha-wt", "feat");
    let mut ahead = RepoStatus::default();
    ahead.branch.ahead = 1;
    let stashed = RepoStatus {
        stashes: 2,
        ..RepoStatus::default()
    };
    st.insert(PathBuf::from("/x/zeta"), TreeState::loaded(ahead, 0));
    st.insert(
        PathBuf::from("/x/loose"),
        TreeState::loaded(stashed.clone(), 0),
    );
    // A worktree reports its repository's stash too; it does not count there.
    st.insert(PathBuf::from("/x/alpha-wt"), TreeState::loaded(stashed, 0));
    assert_eq!(
        names(&s, &build_rows(&s, &st, "", true, None)),
        ["loose", "#work", "zeta"]
    );
}

#[test]
fn pending_only_keeps_the_tree_being_looked_at() {
    let s = store();
    let st = HashMap::new();
    assert!(build_rows(&s, &st, "", true, None).is_empty());
    let keep = PathBuf::from("/x/zeta");
    assert_eq!(
        names(&s, &build_rows(&s, &st, "", true, Some(&keep))),
        ["#work", "zeta"]
    );
    // A text filter still applies to it.
    assert!(build_rows(&s, &st, "alp", true, Some(&keep)).is_empty());
}
