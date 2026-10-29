//! Read-mostly git access through the `git` CLI.
//!
//! Every call runs `git -C <dir>` with optional locks disabled, so a status
//! scan never takes `index.lock` away from an editor or another git process.
//! The only writes are `commit` (optionally after `add -A`) and
//! `worktree remove` in [`ops`], and `push` / `pull --ff-only` in [`remote`].

pub mod diff;
pub mod discover;
pub mod ops;
pub mod remote;
pub mod status;
pub mod worktree;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Branch line of `git status --porcelain=v2 --branch`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchInfo {
    /// Branch name; `None` while HEAD is detached.
    pub head: Option<String>,
    /// Abbreviated HEAD commit; `None` before the first commit.
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// How a changed path appears in the status output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Tracked,
    Untracked,
    Conflict,
}

/// One changed path. `x` / `y` are the porcelain index / worktree codes
/// (`.` = unchanged, `?` for untracked).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    /// Source path of a rename or copy.
    pub orig_path: Option<String>,
    pub x: char,
    pub y: char,
    pub kind: EntryKind,
}

impl FileEntry {
    pub fn staged(&self) -> bool {
        self.kind == EntryKind::Tracked && self.x != '.'
    }

    pub fn unstaged(&self) -> bool {
        self.kind == EntryKind::Tracked && self.y != '.'
    }

    /// Two-letter status code shown in the file list (`M.`→`M `, `??`, `UU`).
    pub fn code(&self) -> String {
        match self.kind {
            EntryKind::Untracked => "??".to_string(),
            _ => [self.x, self.y]
                .iter()
                .map(|c| if *c == '.' { ' ' } else { *c })
                .collect(),
        }
    }
}

/// A commit on HEAD that its upstream does not have yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub subject: String,
}

/// Linked worktree of a repository (the main worktree is the repository row itself).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    pub path: PathBuf,
    /// Short branch name; `None` when detached.
    pub branch: Option<String>,
    pub locked: bool,
    /// git reports the worktree folder as missing.
    pub prunable: bool,
}

/// Everything one scan learns about one working tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoStatus {
    pub branch: BranchInfo,
    pub files: Vec<FileEntry>,
    /// Entries beyond the stored cap (see [`status::MAX_FILES`]).
    pub files_omitted: usize,
    /// Unpushed commits, newest first (only when an upstream exists).
    pub unpushed: Vec<Commit>,
    pub worktrees: Vec<Worktree>,
    pub stashes: u32,
}

/// Counts shown as repository badges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub changed: usize,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicts: usize,
}

impl RepoStatus {
    pub fn counts(&self) -> Counts {
        let mut c = Counts {
            changed: self.files.len() + self.files_omitted,
            ..Counts::default()
        };
        for f in &self.files {
            match f.kind {
                EntryKind::Untracked => c.untracked += 1,
                EntryKind::Conflict => c.conflicts += 1,
                EntryKind::Tracked => {
                    c.staged += usize::from(f.staged());
                    c.unstaged += usize::from(f.unstaged());
                }
            }
        }
        c
    }

    pub fn is_clean(&self) -> bool {
        self.files.is_empty() && self.files_omitted == 0
    }
}

/// Builds a `git -C dir` command with the environment every call shares.
pub(crate) fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null());
    cmd
}

/// Runs git and returns its output whatever the exit status.
pub(crate) fn run(dir: &Path, args: &[&str]) -> anyhow::Result<Output> {
    let out = git_command(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| anyhow::anyhow!("cannot run git: {e}"))?;
    Ok(out)
}

/// Runs git and returns stdout, failing with git's first stderr line.
pub(crate) fn run_ok(dir: &Path, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = run(dir, args)?;
    if !out.status.success() {
        anyhow::bail!("{}", stderr_summary(&out));
    }
    Ok(out.stdout)
}

/// First meaningful stderr line (git prefixes most with `fatal:` / `error:`).
pub(crate) fn stderr_summary(out: &Output) -> String {
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("git failed");
    line.trim_start_matches("fatal: ")
        .trim_start_matches("error: ")
        .to_string()
}

/// Scans one working tree: status, then unpushed commits and stashes, plus the
/// linked worktrees when `with_worktrees` (main repositories only).
pub fn scan(dir: &Path, with_worktrees: bool) -> anyhow::Result<RepoStatus> {
    if !dir.is_dir() {
        anyhow::bail!("folder not found");
    }
    let mut status = status::read(dir)?;
    if status.branch.upstream.is_some() && status.branch.ahead > 0 {
        status.unpushed = status::unpushed(dir).unwrap_or_default();
    }
    status.stashes = status::stash_count(dir);
    if with_worktrees {
        status.worktrees = worktree::list(dir).unwrap_or_default();
    }
    Ok(status)
}

#[cfg(test)]
pub(crate) mod test_repo;
#[cfg(test)]
mod tests;
