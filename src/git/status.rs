//! `git status --porcelain=v2 --branch -z` parsing, unpushed commits, stashes.

use super::{run, run_ok, BranchInfo, Commit, EntryKind, FileEntry, RepoStatus};
use std::path::Path;

/// File entries kept per working tree. A tree with thousands of untracked files
/// (a forgotten build folder) would otherwise bloat the cache and the list.
pub const MAX_FILES: usize = 2000;

/// Unpushed commits listed per working tree.
pub const MAX_UNPUSHED: usize = 50;

pub fn read(dir: &Path) -> anyhow::Result<RepoStatus> {
    let out = run_ok(
        dir,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ],
    )?;
    Ok(parse(&out))
}

/// Parses NUL-separated porcelain v2 records. Unknown records are skipped so a
/// newer git adding a record type does not break the scan.
pub fn parse(raw: &[u8]) -> RepoStatus {
    let mut status = RepoStatus::default();
    let mut fields = raw
        .split(|b| *b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    while let Some(rec) = fields.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(header) = rec.strip_prefix("# ") {
            parse_header(header, &mut status.branch);
            continue;
        }
        let entry = match rec.as_bytes()[0] {
            b'1' => tracked(&rec, 8, None),
            // Rename/copy: the source path follows as its own NUL field.
            b'2' => {
                let orig = fields.next();
                tracked(&rec, 9, orig)
            }
            b'u' => xy_entry(&rec, 10, EntryKind::Conflict, None),
            b'?' => rec.get(2..).map(|path| FileEntry {
                path: path.to_string(),
                orig_path: None,
                x: '?',
                y: '?',
                kind: EntryKind::Untracked,
            }),
            _ => None,
        };
        if let Some(entry) = entry {
            if status.files.len() < MAX_FILES {
                status.files.push(entry);
            } else {
                status.files_omitted += 1;
            }
        }
    }
    sort_entries(&mut status.files);
    status
}

fn parse_header(header: &str, branch: &mut BranchInfo) {
    let (key, value) = header.split_once(' ').unwrap_or((header, ""));
    match key {
        "branch.oid" if value != "(initial)" => {
            branch.oid = Some(value.chars().take(7).collect());
        }
        "branch.head" if value != "(detached)" => branch.head = Some(value.to_string()),
        "branch.upstream" => branch.upstream = Some(value.to_string()),
        "branch.ab" => {
            for part in value.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    branch.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    branch.behind = n.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

fn tracked(rec: &str, path_field: usize, orig: Option<String>) -> Option<FileEntry> {
    xy_entry(rec, path_field, EntryKind::Tracked, orig)
}

/// Splits off `path_field` space-separated fields; the rest of the record is
/// the path (which may itself contain spaces).
fn xy_entry(
    rec: &str,
    path_field: usize,
    kind: EntryKind,
    orig: Option<String>,
) -> Option<FileEntry> {
    let mut parts = rec.splitn(path_field + 1, ' ');
    let _type = parts.next()?;
    let xy: Vec<char> = parts.next()?.chars().collect();
    let path = parts.nth(path_field - 2)?;
    if xy.len() != 2 || path.is_empty() {
        return None;
    }
    Some(FileEntry {
        path: path.to_string(),
        orig_path: orig.filter(|o| !o.is_empty()),
        x: xy[0],
        y: xy[1],
        kind,
    })
}

/// Conflicts first (they block a commit), then tracked changes, then untracked,
/// each by path.
fn sort_entries(files: &mut [FileEntry]) {
    files.sort_by(|a, b| {
        let rank = |f: &FileEntry| match f.kind {
            EntryKind::Conflict => 0,
            EntryKind::Tracked => 1,
            EntryKind::Untracked => 2,
        };
        rank(a).cmp(&rank(b)).then_with(|| a.path.cmp(&b.path))
    });
}

/// Commits on HEAD missing from its upstream, newest first.
pub fn unpushed(dir: &Path) -> anyhow::Result<Vec<Commit>> {
    let limit = format!("-n{MAX_UNPUSHED}");
    let out = run_ok(
        dir,
        &[
            "log",
            &limit,
            "--date=short",
            "--format=%H%x1f%h%x1f%s%x1f%ad",
            "@{upstream}..HEAD",
        ],
    )?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\x1f');
            Some(Commit {
                hash: f.next()?.to_string(),
                short: f.next()?.to_string(),
                subject: f.next().unwrap_or("").to_string(),
                date: f.next().unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// Number of stash entries (0 when there is no stash).
pub fn stash_count(dir: &Path) -> u32 {
    run(
        dir,
        &["rev-list", "--walk-reflogs", "--count", "refs/stash"],
    )
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(records: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for r in records {
            out.extend_from_slice(r.as_bytes());
            out.push(0);
        }
        out
    }

    #[test]
    fn parses_branch_headers() {
        let s = parse(&raw(&[
            "# branch.oid 0123456789abcdef",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -3",
        ]));
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.oid.as_deref(), Some("0123456"));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (2, 3));
    }

    #[test]
    fn detached_and_initial_heads_are_none() {
        let s = parse(&raw(&[
            "# branch.oid (initial)",
            "# branch.head (detached)",
        ]));
        assert_eq!(s.branch.head, None);
        assert_eq!(s.branch.oid, None);
    }

    #[test]
    fn parses_every_entry_kind_and_sorts_conflicts_first() {
        let s = parse(&raw(&[
            "? new file.txt",
            "1 .M N... 100644 100644 100644 aaa bbb src/a b.rs",
            "2 R. N... 100644 100644 100644 aaa bbb R100 new.rs",
            "old.rs",
            "u UU N... 100644 100644 100644 100644 aaa bbb ccc both.rs",
        ]));
        let paths: Vec<_> = s.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["both.rs", "new.rs", "src/a b.rs", "new file.txt"]);
        assert_eq!(s.files[0].kind, EntryKind::Conflict);
        assert_eq!(s.files[1].orig_path.as_deref(), Some("old.rs"));
        assert_eq!(s.files[1].code(), "R ");
        assert_eq!(s.files[2].code(), " M");
        assert_eq!(s.files[3].code(), "??");
        let c = s.counts();
        assert_eq!(
            (c.changed, c.staged, c.unstaged, c.untracked, c.conflicts),
            (4, 1, 1, 1, 1)
        );
    }

    #[test]
    fn caps_stored_entries_and_counts_the_rest() {
        let records: Vec<String> = (0..MAX_FILES + 5).map(|i| format!("? f{i}")).collect();
        let refs: Vec<&str> = records.iter().map(String::as_str).collect();
        let s = parse(&raw(&refs));
        assert_eq!(s.files.len(), MAX_FILES);
        assert_eq!(s.files_omitted, 5);
        assert_eq!(s.counts().changed, MAX_FILES + 5);
    }
}
