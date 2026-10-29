//! Repository pane rows: group headers, repositories, and their linked
//! worktrees indented beneath them. Pure functions over the store and the
//! scan results, so the tree shape is unit-testable without a terminal.

use super::scan::TreeState;
use crate::store::RepoStore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// Group header. Repositories without a group have no header, so `name`
    /// is always `Some`; it stays an `Option` to match `RepoEntry::group`.
    Group {
        name: Option<String>,
        /// Repositories listed under the header.
        total: usize,
        /// Changed files across its repositories and their worktrees (shown
        /// only while the group is folded, when those rows are hidden).
        changed: usize,
        collapsed: bool,
    },
    /// `idx` indexes `RepoStore::repos` as of the build; `path` is the
    /// identity, which stays valid after the store changes. `nested` = drawn
    /// under a group header.
    Repo {
        idx: usize,
        path: PathBuf,
        nested: bool,
    },
    Worktree {
        repo: usize,
        path: PathBuf,
        branch: Option<String>,
        /// Last worktree of its repository (`└` instead of `├`).
        last: bool,
        nested: bool,
    },
}

/// Stable identity of a row, used to keep the cursor on the same item when
/// the rows are rebuilt (rescan, filter, collapse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKey {
    Group(Option<String>),
    Repo(PathBuf),
    Worktree(PathBuf),
}

impl Row {
    pub fn key(&self) -> RowKey {
        match self {
            Row::Group { name, .. } => RowKey::Group(name.clone()),
            Row::Repo { path, .. } => RowKey::Repo(path.clone()),
            Row::Worktree { path, .. } => RowKey::Worktree(path.clone()),
        }
    }

    /// Working tree this row shows files for (`None` for group headers).
    pub fn target(&self) -> Option<&Path> {
        match self {
            Row::Group { .. } => None,
            Row::Repo { path, .. } => Some(path),
            Row::Worktree { path, .. } => Some(path),
        }
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Builds the visible rows: repositories without a group first, flat and
/// unindented (no header), then each group header with its repositories
/// indented beneath it. With a filter (case-insensitive substring of the
/// folder or branch name) or `changed_only` (working trees with uncommitted
/// changes), collapsed groups open and only groups with a match are kept. A
/// clean repository stays as the parent of a worktree that has changes.
pub fn build_rows(
    store: &RepoStore,
    statuses: &HashMap<PathBuf, TreeState>,
    filter: &str,
    changed_only: bool,
) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    let matches = |text: &str| needle.is_empty() || text.to_lowercase().contains(&needle);
    let has_changes = |p: &Path| {
        statuses
            .get(p)
            .and_then(|t| t.status.as_ref())
            .is_some_and(|s| !s.is_clean())
    };
    let wanted = |p: &Path| !changed_only || has_changes(p);
    let filtering = !needle.is_empty() || changed_only;

    // Repositories (with their visible worktrees) per group, name-sorted;
    // ungrouped first.
    let mut by_group: Vec<(Option<String>, Vec<usize>)> = vec![(None, Vec::new())];
    by_group.extend(
        store
            .groups
            .iter()
            .map(|g| (Some(g.name.clone()), Vec::new())),
    );
    let mut order: Vec<usize> = (0..store.repos.len()).collect();
    order.sort_by_cached_key(|&i| store.repos[i].name().to_lowercase());
    for i in order {
        let group = store.repos[i].group.clone();
        if let Some(slot) = by_group.iter_mut().find(|(g, _)| *g == group) {
            slot.1.push(i);
        }
    }

    let mut rows = Vec::new();
    for (group, repos) in by_group {
        let grouped = group.is_some();
        let mut body = Vec::new();
        let mut shown = 0;
        let mut changed = 0;
        for idx in repos {
            let repo = &store.repos[idx];
            let status = statuses.get(&repo.path).and_then(|t| t.status.as_ref());
            let worktrees: Vec<_> = status
                .map(|s| s.worktrees.iter())
                .into_iter()
                .flatten()
                .filter(|w| {
                    (matches(&folder_name(&w.path)) || w.branch.as_deref().is_some_and(matches))
                        && wanted(&w.path)
                })
                .collect();
            let repo_hit = (matches(&repo.name())
                || status
                    .and_then(|s| s.branch.head.as_deref())
                    .is_some_and(matches))
                && wanted(&repo.path);
            if !repo_hit && worktrees.is_empty() {
                continue;
            }
            shown += 1;
            let tree_changes = |p: &Path| {
                statuses
                    .get(p)
                    .and_then(|t| t.status.as_ref())
                    .map_or(0, |s| s.counts().changed)
            };
            changed += tree_changes(&repo.path);
            if let Some(s) = status {
                changed += s
                    .worktrees
                    .iter()
                    .map(|w| tree_changes(&w.path))
                    .sum::<usize>();
            }
            body.push(Row::Repo {
                idx,
                path: repo.path.clone(),
                nested: grouped,
            });
            let all_worktrees: Vec<_> = if repo_hit {
                status
                    .map(|s| s.worktrees.iter().filter(|w| wanted(&w.path)).collect())
                    .unwrap_or_default()
            } else {
                worktrees
            };
            let n = all_worktrees.len();
            for (i, w) in all_worktrees.into_iter().enumerate() {
                body.push(Row::Worktree {
                    repo: idx,
                    path: w.path.clone(),
                    branch: w.branch.clone(),
                    last: i + 1 == n,
                    nested: grouped,
                });
            }
        }
        if !grouped {
            rows.extend(body);
            continue;
        }
        // An empty group stays visible (unless filtering) so a new group can
        // be seen and used.
        if shown == 0 && filtering {
            continue;
        }
        let collapsed = !filtering && store.is_collapsed(group.as_deref());
        rows.push(Row::Group {
            name: group,
            total: shown,
            changed,
            collapsed,
        });
        if !collapsed {
            rows.extend(body);
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{RepoStatus, Worktree};

    fn store() -> RepoStore {
        let mut s = RepoStore::default();
        s.add_group("work").unwrap();
        s.add_repo("/x/zeta".into(), Some("work".into()));
        s.add_repo("/x/alpha".into(), Some("work".into()));
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
    fn groups_sort_repositories_and_nest_worktrees() {
        let s = store();
        let rows = build_rows(&s, &with_worktree("/x/alpha-wt", "feat"), "", false);
        assert_eq!(
            names(&s, &rows),
            ["loose", "#work", "alpha", "^alpha-wt", "zeta"]
        );
    }

    #[test]
    fn ungrouped_repositories_lead_without_header_or_indent() {
        let s = store();
        let rows = build_rows(&s, &HashMap::new(), "", false);
        assert!(matches!(rows[0], Row::Repo { nested: false, .. }));
        assert!(matches!(rows[2], Row::Repo { nested: true, .. }));
        assert!(rows
            .iter()
            .all(|r| !matches!(r, Row::Group { name: None, .. })));
    }

    #[test]
    fn flat_without_groups_and_no_ungrouped_header() {
        let mut s = RepoStore::default();
        s.add_repo("/b".into(), None);
        s.add_repo("/a".into(), None);
        let rows = build_rows(&s, &HashMap::new(), "", false);
        assert_eq!(names(&s, &rows), ["a", "b"]);
        assert!(matches!(rows[0], Row::Repo { nested: false, .. }));
    }

    #[test]
    fn collapsed_groups_hide_their_rows_but_a_filter_opens_them() {
        let mut s = store();
        s.toggle_collapsed(Some("work"));
        let st = with_worktree("/x/alpha-wt", "feat");
        assert_eq!(
            names(&s, &build_rows(&s, &st, "", false)),
            ["loose", "#work"]
        );
        assert_eq!(
            names(&s, &build_rows(&s, &st, "ZET", false)),
            ["#work", "zeta"]
        );
    }

    #[test]
    fn a_worktree_branch_match_keeps_its_repository() {
        let s = store();
        let rows = build_rows(&s, &with_worktree("/x/alpha-wt", "feat"), "feat", false);
        assert_eq!(names(&s, &rows), ["#work", "alpha", "^alpha-wt"]);
    }

    #[test]
    fn empty_named_groups_stay_visible() {
        let mut s = store();
        s.add_group("empty").unwrap();
        let rows = build_rows(&s, &HashMap::new(), "", false);
        assert!(names(&s, &rows).contains(&"#empty".to_string()));
    }

    #[test]
    fn group_change_count_sums_files_of_repositories_and_worktrees() {
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
        let wt = RepoStatus {
            files: vec![file("c")],
            ..RepoStatus::default()
        };
        st.insert(PathBuf::from("/x/alpha-wt"), TreeState::loaded(wt, 0));
        let rows = build_rows(&s, &st, "", false);
        assert!(
            matches!(
                rows[1],
                Row::Group {
                    changed: 3,
                    total: 2,
                    ..
                }
            ),
            "{:?}",
            rows[1]
        );
    }

    #[test]
    fn changed_only_keeps_dirty_trees_and_the_parent_of_a_dirty_worktree() {
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
        let rows = build_rows(&folded, &st, "", true);
        assert_eq!(names(&s, &rows), ["#work", "alpha", "^alpha-wt", "zeta"]);
        // Combined with a text filter, both must match.
        assert_eq!(
            names(&s, &build_rows(&s, &st, "zet", true)),
            ["#work", "zeta"]
        );
        assert!(build_rows(&s, &st, "loose", true).is_empty());
    }
}
