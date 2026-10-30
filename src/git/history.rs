//! Commit history with a branch graph laid out by [`super::graph`]: git gives
//! the commits in topological order with their parents, ular-git places the
//! lanes (one row per commit, joints drawn on the commit row).
//!
//! With an upstream the history starts from both HEAD and the upstream, so
//! the commits a pull would bring in (as of the last fetch) are listed too,
//! on their own lane until they meet HEAD's history.

use super::graph::{self, Cell};
use super::{run, Commit};
use std::collections::HashSet;
use std::path::Path;

/// Commits read first; "more" doubles it.
pub const FIRST_PAGE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// Graph cells for this commit's row.
    pub cells: Vec<Cell>,
    pub commit: Commit,
    /// On the upstream but not reachable from HEAD: a pull brings it in.
    pub incoming: bool,
    /// Branches and tags pointing at the commit, in git's order.
    pub refs: Vec<RefLabel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// A local branch; `HEAD` alone when HEAD is detached.
    Branch,
    /// A remote-tracking branch (`origin/main`).
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLabel {
    pub kind: RefKind,
    /// Short name (`main`, `origin/main`, `v1.0`).
    pub name: String,
    /// The branch HEAD is on (or a detached HEAD).
    pub head: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    pub rows: Vec<GraphRow>,
    /// The page was full, so older commits exist.
    pub more: bool,
}

/// Field separator in the format string.
const SEP: char = '\x1f';

/// The newest `limit` commits reachable from HEAD (and from the upstream when
/// it has commits HEAD lacks), with the graph; rows a pull would bring in are
/// marked `incoming`. A repository without commits has an empty history.
pub fn load(dir: &Path, limit: usize) -> anyhow::Result<History> {
    let n = format!("-n{limit}");
    // Fails without an upstream (or on an unborn branch): HEAD only.
    let incoming: Option<HashSet<String>> = run(dir, &["rev-list", &n, "HEAD..@{upstream}"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect()
        });
    let mut args = vec![
        "log",
        "--topo-order",
        &n,
        // Full ref names tell a local `feat/x` from a remote `origin/x`.
        "--decorate=full",
        "--format=%H%x1f%h%x1f%P%x1f%D%x1f%s",
        "HEAD",
    ];
    if incoming.as_ref().is_some_and(|set| !set.is_empty()) {
        args.push("@{upstream}");
    }
    let out = run(dir, &args)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        // An unborn branch has no history, which is not an error.
        if err.contains("does not have any commits") || err.contains("unknown revision") {
            return Ok(History::default());
        }
        anyhow::bail!("{}", super::stderr_summary(&out));
    }
    let mut history = parse(&String::from_utf8_lossy(&out.stdout), limit);
    if let Some(set) = incoming {
        for row in &mut history.rows {
            row.incoming = set.contains(&row.commit.hash);
        }
    }
    Ok(history)
}

/// Parses `hash SEP short SEP parents SEP refs SEP subject` lines and lays
/// out the graph.
pub fn parse(text: &str, limit: usize) -> History {
    let mut commits: Vec<(Commit, Vec<String>, Vec<RefLabel>)> = Vec::new();
    for line in text.lines() {
        let mut f = line.split(SEP);
        let (Some(hash), Some(short), Some(parents), Some(refs)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        commits.push((
            Commit {
                hash: hash.to_string(),
                short: short.to_string(),
                subject: f.next().unwrap_or("").to_string(),
            },
            parents.split_whitespace().map(str::to_string).collect(),
            parse_refs(refs),
        ));
    }
    let shape: Vec<(String, Vec<String>)> = commits
        .iter()
        .map(|(c, p, _)| (c.hash.clone(), p.clone()))
        .collect();
    let more = commits.len() >= limit;
    let rows = graph::layout(&shape)
        .into_iter()
        .zip(commits)
        .map(|(cells, (commit, _, refs))| GraphRow {
            cells,
            commit,
            incoming: false,
            refs,
        })
        .collect();
    History { rows, more }
}

/// Reads `%D` with `--decorate=full` (`HEAD -> refs/heads/main,
/// refs/remotes/origin/main, tag: refs/tags/v1`). `origin/HEAD` and other
/// refs (stash, notes) are left out, and so is a remote branch when a local
/// branch of the same name sits on the same commit (`main` and
/// `origin/main`: the push / pull marks already tell them apart).
fn parse_refs(text: &str) -> Vec<RefLabel> {
    let mut refs = Vec::new();
    for item in text.split(", ").map(str::trim).filter(|s| !s.is_empty()) {
        let (head, name) = match item.strip_prefix("HEAD -> ") {
            Some(rest) => (true, rest),
            None => (false, item),
        };
        let label = |kind, name: &str| RefLabel {
            kind,
            name: name.to_string(),
            head,
        };
        if name == "HEAD" {
            refs.push(label(RefKind::Branch, "HEAD"));
        } else if let Some(b) = name.strip_prefix("refs/heads/") {
            refs.push(label(RefKind::Branch, b));
        } else if let Some(t) = name.strip_prefix("tag: refs/tags/") {
            refs.push(label(RefKind::Tag, t));
        } else if let Some(r) = name.strip_prefix("refs/remotes/") {
            if !r.ends_with("/HEAD") {
                refs.push(label(RefKind::Remote, r));
            }
        }
    }
    let local: Vec<String> = refs
        .iter()
        .filter(|r| r.kind == RefKind::Branch)
        .map(|r| r.name.clone())
        .collect();
    refs.retain(|r| {
        r.kind != RefKind::Remote
            || !r
                .name
                .split_once('/')
                .is_some_and(|(_, b)| local.iter().any(|l| l == b))
    });
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commits_with_parents_into_graph_rows() {
        let text = "m\x1fm1\x1fa b\x1fHEAD -> refs/heads/main\x1fMerge b\n\
b\x1fb1\x1fa\x1f\x1fon b\n\
a\x1fa1\x1f\x1ftag: refs/tags/v1\x1froot\n";
        let h = parse(text, 100);
        let rows: Vec<String> = h
            .rows
            .iter()
            .map(|r| r.cells.iter().map(|c| c.ch).collect())
            .collect();
        assert_eq!(rows, ["●─╮", "│ ●", "●─╯"]);
        assert_eq!(h.rows[0].commit.subject, "Merge b");
        assert_eq!(h.rows[0].refs[0].name, "main");
        assert!(h.rows[0].refs[0].head);
        assert!(h.rows[1].refs.is_empty());
        assert_eq!(h.rows[2].refs[0].kind, RefKind::Tag);
        assert!(!h.more);
        assert!(parse(text, 3).more);
    }

    #[test]
    fn refs_keep_branches_remotes_and_tags_and_drop_a_remote_twin() {
        let refs = parse_refs(
            "HEAD -> refs/heads/main, refs/remotes/origin/main, refs/remotes/origin/HEAD, \
             refs/remotes/origin/feat/x, refs/heads/feat/y, tag: refs/tags/v1.0, refs/stash",
        );
        let got: Vec<(RefKind, &str, bool)> = refs
            .iter()
            .map(|r| (r.kind, r.name.as_str(), r.head))
            .collect();
        assert_eq!(
            got,
            [
                (RefKind::Branch, "main", true),
                (RefKind::Remote, "origin/feat/x", false),
                (RefKind::Branch, "feat/y", false),
                (RefKind::Tag, "v1.0", false),
            ]
        );
        assert_eq!(parse_refs("HEAD")[0].name, "HEAD");
    }
}
