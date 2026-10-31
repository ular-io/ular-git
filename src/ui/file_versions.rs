//! Open With's tabs for a file: what can be opened (a diff, the file before,
//! after, or in the working tree) and which version each tab reads. Pure:
//! the versions come from the row's status letters, never from git.

use crate::git::commit_files::CommitFile;
use crate::git::{Commit, EntryKind, FileEntry};
use serde::{Deserialize, Serialize};

/// One version of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Version {
    /// The working tree's file at its path.
    WorkTree,
    /// As git stores it at `rev`; `label` names its temporary folder.
    Rev {
        rev: String,
        path: String,
        label: String,
    },
    /// No file on this side (added or deleted): an empty file.
    Empty,
}

/// What a tab opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    /// A diff app with the old and the new file.
    Diff(Version, Version),
    /// A file app with one version.
    File(Version),
}

/// A tab's role; the last one used is kept in `open_with.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabKind {
    Diff,
    /// The parent's file for a commit, HEAD's for the uncommitted changes.
    Before,
    /// The commit's file.
    After,
    WorkTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub kind: TabKind,
    pub label: String,
    pub target: OpenTarget,
}

const WORKTREE_LABEL: &str = "Working Tree";

/// Tabs of an uncommitted file: Diff (HEAD against the working tree), HEAD
/// unless the file is new, Working Tree unless it is deleted (`on_disk`).
/// `head_path` is the path in HEAD (the old path of a rename), `None` for a
/// file HEAD lacks.
fn uncommitted_tabs(head_path: Option<&str>, on_disk: bool) -> Vec<Tab> {
    let before = head_path.map(|p| Version::Rev {
        rev: "HEAD".into(),
        path: p.to_string(),
        label: "HEAD".into(),
    });
    let new = if on_disk {
        Version::WorkTree
    } else {
        Version::Empty
    };
    let mut tabs = vec![Tab {
        kind: TabKind::Diff,
        label: "Diff".into(),
        target: OpenTarget::Diff(before.clone().unwrap_or(Version::Empty), new),
    }];
    if let Some(v) = before {
        tabs.push(Tab {
            kind: TabKind::Before,
            label: "HEAD".into(),
            target: OpenTarget::File(v),
        });
    }
    if on_disk {
        tabs.push(worktree_tab());
    }
    tabs
}

fn worktree_tab() -> Tab {
    Tab {
        kind: TabKind::WorkTree,
        label: WORKTREE_LABEL.into(),
        target: OpenTarget::File(Version::WorkTree),
    }
}

/// A changed-file row.
pub fn changed_file_tabs(f: &FileEntry, on_disk: bool) -> Vec<Tab> {
    let new_in_index = f.kind == EntryKind::Untracked || f.x == 'A';
    let head_path = (!new_in_index).then(|| f.orig_path.as_deref().unwrap_or(&f.path));
    uncommitted_tabs(head_path, on_disk)
}

/// A file of the uncommitted changes' view.
pub fn worktree_file_tabs(f: &CommitFile, on_disk: bool) -> Vec<Tab> {
    let head_path =
        (!matches!(f.status, 'A' | '?')).then(|| f.orig_path.as_deref().unwrap_or(&f.path));
    uncommitted_tabs(head_path, on_disk)
}

/// A file of a commit's view: Diff (the first parent against the commit),
/// the parent's file unless the commit added it, the commit's unless it
/// deleted it, and the working tree's when the file is there.
pub fn commit_file_tabs(c: &Commit, f: &CommitFile, on_disk: bool) -> Vec<Tab> {
    let before = (f.status != 'A').then(|| Version::Rev {
        rev: format!("{}^", c.hash),
        path: f.orig_path.clone().unwrap_or_else(|| f.path.clone()),
        label: format!("{}^", c.short),
    });
    let after = (f.status != 'D').then(|| Version::Rev {
        rev: c.hash.clone(),
        path: f.path.clone(),
        label: c.short.clone(),
    });
    let mut tabs = vec![Tab {
        kind: TabKind::Diff,
        label: "Diff".into(),
        target: OpenTarget::Diff(
            before.clone().unwrap_or(Version::Empty),
            after.clone().unwrap_or(Version::Empty),
        ),
    }];
    if let Some(v) = before {
        tabs.push(Tab {
            kind: TabKind::Before,
            label: "Parent".into(),
            target: OpenTarget::File(v),
        });
    }
    if let Some(v) = after {
        tabs.push(Tab {
            kind: TabKind::After,
            label: c.short.clone(),
            target: OpenTarget::File(v),
        });
    }
    if on_disk {
        tabs.push(worktree_tab());
    }
    tabs
}

/// The tab to start on: the one used last when this file has it, else the
/// working tree's (the file as it is now), else Diff.
pub fn start_tab(tabs: &[Tab], last: Option<TabKind>) -> usize {
    let at = |k: TabKind| tabs.iter().position(|t| t.kind == k);
    last.and_then(at)
        .or_else(|| at(TabKind::WorkTree))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(tabs: &[Tab]) -> Vec<&str> {
        tabs.iter().map(|t| t.label.as_str()).collect()
    }

    fn cf(status: char, path: &str, orig: Option<&str>) -> CommitFile {
        CommitFile {
            status,
            path: path.into(),
            orig_path: orig.map(Into::into),
            counts: None,
        }
    }

    fn commit() -> Commit {
        Commit {
            hash: "abcdef0123".into(),
            short: "abcdef0".into(),
            subject: "s".into(),
        }
    }

    #[test]
    fn a_commit_file_has_the_versions_its_status_allows() {
        let c = commit();
        let tabs = commit_file_tabs(&c, &cf('M', "a.rs", None), true);
        assert_eq!(kinds(&tabs), ["Diff", "Parent", "abcdef0", "Working Tree"]);
        let rev = |rev: &str, label: &str| Version::Rev {
            rev: rev.into(),
            path: "a.rs".into(),
            label: label.into(),
        };
        assert_eq!(
            tabs[0].target,
            OpenTarget::Diff(rev("abcdef0123^", "abcdef0^"), rev("abcdef0123", "abcdef0"))
        );
        let added = commit_file_tabs(&c, &cf('A', "a.rs", None), false);
        assert_eq!(kinds(&added), ["Diff", "abcdef0"]);
        assert!(matches!(
            &added[0].target,
            OpenTarget::Diff(Version::Empty, _)
        ));
        assert_eq!(
            kinds(&commit_file_tabs(&c, &cf('D', "a.rs", None), false)),
            ["Diff", "Parent"]
        );
        // A rename's parent side reads the old path.
        let renamed = commit_file_tabs(&c, &cf('R', "new.rs", Some("old.rs")), true);
        assert!(matches!(
            &renamed[1].target,
            OpenTarget::File(Version::Rev { path, .. }) if path == "old.rs"
        ));
    }

    #[test]
    fn an_uncommitted_file_has_head_unless_new_and_the_working_tree_unless_deleted() {
        assert_eq!(
            kinds(&worktree_file_tabs(&cf('M', "a.rs", None), true)),
            ["Diff", "HEAD", "Working Tree"]
        );
        assert_eq!(
            kinds(&worktree_file_tabs(&cf('?', "n.rs", None), true)),
            ["Diff", "Working Tree"]
        );
        assert_eq!(
            kinds(&worktree_file_tabs(&cf('D', "a.rs", None), false)),
            ["Diff", "HEAD"]
        );
        let entry = |x: char, kind: EntryKind| FileEntry {
            path: "a.rs".into(),
            orig_path: None,
            x,
            y: '.',
            kind,
        };
        assert_eq!(
            kinds(&changed_file_tabs(&entry('A', EntryKind::Tracked), true)),
            ["Diff", "Working Tree"]
        );
        assert_eq!(
            kinds(&changed_file_tabs(&entry('M', EntryKind::Tracked), true)),
            ["Diff", "HEAD", "Working Tree"]
        );
    }

    #[test]
    fn the_start_tab_is_the_last_used_then_the_working_tree() {
        let c = commit();
        let tabs = commit_file_tabs(&c, &cf('M', "a.rs", None), true);
        assert_eq!(start_tab(&tabs, Some(TabKind::After)), 2);
        assert_eq!(start_tab(&tabs, None), 3);
        let deleted = commit_file_tabs(&c, &cf('D', "a.rs", None), false);
        assert_eq!(start_tab(&deleted, Some(TabKind::WorkTree)), 0, "Diff");
    }
}
