//! The local git writes ular-git performs: committing uncommitted changes,
//! discarding one file's changes, and removing a linked worktree (push and
//! pull live in `remote`).

use super::{run, run_ok, stderr_summary, EntryKind, FileEntry};
use std::path::Path;

/// What a commit includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitScope {
    /// Only what is already in the index.
    Staged,
    /// `git add -A` first: every tracked change and every untracked file.
    All,
}

/// Commits in `dir` and returns git's summary line (`[main 1a2b3c4] subject`).
/// Hooks run as usual; their failure output is returned as the error.
pub fn commit(dir: &Path, message: &str, scope: CommitScope) -> anyhow::Result<String> {
    if message.trim().is_empty() {
        anyhow::bail!("commit message is empty");
    }
    if scope == CommitScope::All {
        run_ok(dir, &["add", "-A"])?;
    }
    let out = run(dir, &["commit", "--quiet", "-m", message])?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = stdout.lines().map(str::trim).find(|l| !l.is_empty());
        let summary = stderr_summary(&out);
        anyhow::bail!(
            "{}",
            if summary == "git failed" {
                detail.unwrap_or("git commit failed").to_string()
            } else {
                summary
            }
        );
    }
    let head = run_ok(dir, &["log", "-1", "--format=%h %s"])?;
    Ok(String::from_utf8_lossy(&head).trim().to_string())
}

/// Throws away the uncommitted changes of one file, back to HEAD: staged and
/// unstaged edits, a deletion, a rename (both paths), a newly added file
/// (removed), or an untracked file (deleted). Conflicts are refused. Cannot
/// be undone.
pub fn discard(dir: &Path, file: &FileEntry) -> anyhow::Result<()> {
    match file.kind {
        EntryKind::Conflict => anyhow::bail!("resolve the merge conflict first"),
        EntryKind::Untracked => {
            run_ok(dir, &["clean", "-f", "-q", "--", &file.path])?;
        }
        EntryKind::Tracked => {
            let mut args = vec!["restore", "--source=HEAD", "--staged", "--worktree", "--"];
            if let Some(orig) = file.orig_path.as_deref() {
                args.push(orig);
            }
            args.push(&file.path);
            run_ok(dir, &args)?;
        }
    }
    Ok(())
}

/// Removes the linked worktree at `path` from the repository at `repo`.
/// `force` discards its uncommitted changes (and overrides a lock).
pub fn remove_worktree(repo: &Path, path: &Path, force: bool) -> anyhow::Result<()> {
    let path = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.extend(["--force", "--force"]);
    }
    args.push(&path);
    run_ok(repo, &args)?;
    Ok(())
}

/// Drops worktree records whose folder no longer exists.
pub fn prune_worktrees(repo: &Path) -> anyhow::Result<()> {
    run_ok(repo, &["worktree", "prune"])?;
    Ok(())
}
