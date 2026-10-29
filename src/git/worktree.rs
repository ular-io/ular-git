//! `git worktree list --porcelain` parsing.

use super::{run_ok, Worktree};
use std::path::{Path, PathBuf};

/// Linked worktrees of the repository at `dir`. The first porcelain block is
/// the main worktree, which the repository row already represents, so it is
/// dropped; bare entries are dropped too.
pub fn list(dir: &Path) -> anyhow::Result<Vec<Worktree>> {
    let out = run_ok(dir, &["worktree", "list", "--porcelain", "-z"])?;
    Ok(parse(&out))
}

/// Parses `-z` output: attributes are NUL-terminated and a block ends with an
/// empty field.
pub fn parse(raw: &[u8]) -> Vec<Worktree> {
    let mut blocks: Vec<(Worktree, bool)> = Vec::new();
    for field in raw.split(|b| *b == 0) {
        let line = String::from_utf8_lossy(field);
        if let Some(path) = line.strip_prefix("worktree ") {
            blocks.push((
                Worktree {
                    path: PathBuf::from(path),
                    branch: None,
                    locked: false,
                    prunable: false,
                },
                false,
            ));
            continue;
        }
        let Some((wt, bare)) = blocks.last_mut() else {
            continue;
        };
        let (key, value) = line.split_once(' ').unwrap_or((&line, ""));
        match key {
            "branch" => {
                wt.branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).into());
            }
            "locked" => wt.locked = true,
            "prunable" => wt.prunable = true,
            "bare" => *bare = true,
            _ => {}
        }
    }
    blocks
        .into_iter()
        .skip(1)
        .filter(|(_, bare)| !bare)
        .map(|(wt, _)| wt)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_the_main_worktree_and_reads_attributes() {
        let raw = b"worktree /r\0HEAD aaa\0branch refs/heads/main\0\0\
worktree /r-feat\0HEAD bbb\0branch refs/heads/feat/x\0locked busy\0\0\
worktree /r-old\0HEAD ccc\0detached\0prunable gitdir file points to non-existent location\0\0";
        let wts = parse(raw);
        assert_eq!(wts.len(), 2);
        assert_eq!(wts[0].path, PathBuf::from("/r-feat"));
        assert_eq!(wts[0].branch.as_deref(), Some("feat/x"));
        assert!(wts[0].locked && !wts[0].prunable);
        assert_eq!(wts[1].branch, None);
        assert!(wts[1].prunable);
    }

    #[test]
    fn a_repository_without_linked_worktrees_lists_none() {
        assert!(parse(b"worktree /r\0HEAD aaa\0branch refs/heads/main\0\0").is_empty());
    }
}
