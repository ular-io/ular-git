//! Files one commit changed (`→` on a history commit): against its first
//! parent for a merge, against nothing for a root commit, renames detected.

use super::run_ok;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFile {
    /// git's status letter: `A` added, `M` modified, `D` deleted, `R`
    /// renamed, `C` copied, `T` type changed.
    pub status: char,
    /// The path after the commit.
    pub path: String,
    /// The path before a rename or copy.
    pub orig_path: Option<String>,
    /// Added and removed lines; `None` for a binary file.
    pub counts: Option<(usize, usize)>,
}

pub fn load(dir: &Path, hash: &str) -> anyhow::Result<Vec<CommitFile>> {
    let raw = run_ok(
        dir,
        &[
            "diff-tree",
            "--no-commit-id",
            "-r",
            "-M",
            "--root",
            "--diff-merges=first-parent",
            "--raw",
            "--numstat",
            "-z",
            hash,
        ],
    )?;
    Ok(parse(&raw))
}

/// `--raw --numstat -z` output: every raw record (`:modes oids STATUS`, then
/// one path, or two for a rename or copy), then the numstat records in the
/// same order (`added\tremoved\tpath`, or `added\tremoved\t` and two paths;
/// `-` counts for a binary file).
fn parse(raw: &[u8]) -> Vec<CommitFile> {
    let text = String::from_utf8_lossy(raw);
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());
    let mut files: Vec<CommitFile> = Vec::new();
    let mut counted = 0;
    while let Some(token) = tokens.next() {
        if let Some(meta) = token.strip_prefix(':') {
            let status = meta
                .rsplit(' ')
                .next()
                .and_then(|s| s.chars().next())
                .unwrap_or('M');
            let first = tokens.next().unwrap_or_default().to_string();
            let file = if matches!(status, 'R' | 'C') {
                CommitFile {
                    status,
                    path: tokens.next().unwrap_or_default().to_string(),
                    orig_path: Some(first),
                    counts: None,
                }
            } else {
                CommitFile {
                    status,
                    path: first,
                    orig_path: None,
                    counts: None,
                }
            };
            files.push(file);
            continue;
        }
        let mut parts = token.splitn(3, '\t');
        let (added, removed) = (parts.next(), parts.next());
        if parts.next().is_some_and(str::is_empty) {
            // A rename's two paths follow as their own records.
            tokens.next();
            tokens.next();
        }
        let n = |s: Option<&str>| s.and_then(|s| s.parse::<usize>().ok());
        if let Some(file) = files.get_mut(counted) {
            file.counts = n(added).zip(n(removed));
        }
        counted += 1;
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;

    #[test]
    fn parses_renames_binaries_and_counts_in_order() {
        let raw = b":100644 100644 aaa bbb R080\0a\0a_renamed\0\
:000000 100644 000 ccc A\0b\0\
:100644 100644 ddd eee M\0img.png\0\
1\t0\t\0a\0a_renamed\0\
2\t0\tb\0\
-\t-\timg.png\0";
        let files = parse(raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, 'R');
        assert_eq!(files[0].orig_path.as_deref(), Some("a"));
        assert_eq!(files[0].path, "a_renamed");
        assert_eq!(files[0].counts, Some((1, 0)));
        assert_eq!((files[1].status, files[1].path.as_str()), ('A', "b"));
        assert_eq!(files[1].counts, Some((2, 0)));
        assert_eq!(files[2].counts, None, "binary");
    }

    #[test]
    fn a_merge_lists_what_it_brought_in_and_a_root_commit_its_files() {
        let a = TestRepo::new();
        a.git(&["switch", "-q", "-c", "feat"]);
        a.write("f.txt", "f\n");
        a.git(&["add", "f.txt"]);
        a.git(&["commit", "-qm", "on feat"]);
        a.git(&["switch", "-q", "main"]);
        a.write("m.txt", "m\n");
        a.git(&["add", "m.txt"]);
        a.git(&["commit", "-qm", "on main"]);
        a.git(&["merge", "-q", "--no-ff", "-m", "merge feat", "feat"]);
        let merge = load(&a.root, "HEAD").unwrap();
        assert_eq!(merge.len(), 1, "{merge:?}");
        assert_eq!((merge[0].status, merge[0].path.as_str()), ('A', "f.txt"));
        let root = a.git(&["rev-list", "--max-parents=0", "HEAD"]);
        let first = load(&a.root, root.trim()).unwrap();
        assert!(!first.is_empty());
        assert!(first.iter().all(|f| f.status == 'A'), "{first:?}");
    }
}
