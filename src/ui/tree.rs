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
/// folder or branch name) or `pending_only` (working trees with something to
/// commit, pull, or push, or a stash), collapsed groups open and only groups
/// with a match are kept. A clean repository stays as the parent of a
/// worktree that matches. `keep` (the working tree being looked at) stays in
/// the pending view after it stops matching, so a commit made elsewhere does
/// not pull it from under the cursor.
pub fn build_rows(
    store: &RepoStore,
    statuses: &HashMap<PathBuf, TreeState>,
    filter: &str,
    pending_only: bool,
    keep: Option<&Path>,
) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    let matches = |text: &str| needle.is_empty() || text.to_lowercase().contains(&needle);
    // `main`: a repository row. The stash is shared by a repository and its
    // worktrees, so it only counts for the repository.
    let wanted = |p: &Path, main: bool| {
        !pending_only
            || keep == Some(p)
            || statuses
                .get(p)
                .and_then(|t| t.status.as_ref())
                .is_some_and(|s| {
                    !s.is_clean()
                        || s.branch.behind > 0
                        || s.branch.ahead > 0
                        || (main && s.stashes > 0)
                })
    };
    let filtering = !needle.is_empty() || pending_only;

    // Repositories (with their visible worktrees) per group, in store order;
    // ungrouped first.
    let mut by_group: Vec<(Option<String>, Vec<usize>)> = vec![(None, Vec::new())];
    by_group.extend(
        store
            .groups
            .iter()
            .map(|g| (Some(g.name.clone()), Vec::new())),
    );
    for i in 0..store.repos.len() {
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
                        && wanted(&w.path, false)
                })
                .collect();
            let repo_hit = (matches(&repo.name())
                || status
                    .and_then(|s| s.branch.head.as_deref())
                    .is_some_and(matches))
                && wanted(&repo.path, true);
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
                    .map(|s| {
                        s.worktrees
                            .iter()
                            .filter(|w| wanted(&w.path, false))
                            .collect()
                    })
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
        if collapsed {
            // A peer can fold this group while this instance is inspecting a
            // child. Keep that path (and a worktree's parent) visible locally.
            let owner = body.iter().find_map(|row| match row {
                Row::Repo { idx, path, .. } if keep == Some(path.as_path()) => Some(*idx),
                Row::Worktree { repo, path, .. } if keep == Some(path.as_path()) => Some(*repo),
                _ => None,
            });
            rows.extend(body.into_iter().filter(|row| {
                row.target() == keep || matches!(row, Row::Repo { idx, .. } if Some(*idx) == owner)
            }));
        } else {
            rows.extend(body);
        }
    }
    rows
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
