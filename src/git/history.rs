//! Commit history with a branch graph laid out by [`super::graph`]: git gives
//! the commits in topological order with their parents, ular-git places the
//! lanes (one row per commit, joints drawn on the commit row).

use super::graph::{self, Cell};
use super::{run, Commit};
use std::path::Path;

/// Commits read first; "more" doubles it.
pub const FIRST_PAGE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// Graph cells for this commit's row.
    pub cells: Vec<Cell>,
    pub commit: Commit,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    pub rows: Vec<GraphRow>,
    /// The page was full, so older commits exist.
    pub more: bool,
}

/// Field separator in the format string.
const SEP: char = '\x1f';

/// The newest `limit` commits reachable from HEAD, with the graph. A
/// repository without commits has an empty history.
pub fn load(dir: &Path, limit: usize) -> anyhow::Result<History> {
    let n = format!("-n{limit}");
    let out = run(
        dir,
        &[
            "log",
            "--topo-order",
            &n,
            "--format=%H%x1f%h%x1f%P%x1f%s",
            "HEAD",
        ],
    )?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        // An unborn branch has no history, which is not an error.
        if err.contains("does not have any commits") || err.contains("unknown revision") {
            return Ok(History::default());
        }
        anyhow::bail!("{}", super::stderr_summary(&out));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout), limit))
}

/// Parses `hash SEP short SEP parents SEP subject` lines and lays out the graph.
pub fn parse(text: &str, limit: usize) -> History {
    let mut commits: Vec<(Commit, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let mut f = line.split(SEP);
        let (Some(hash), Some(short), Some(parents)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        commits.push((
            Commit {
                hash: hash.to_string(),
                short: short.to_string(),
                subject: f.next().unwrap_or("").to_string(),
            },
            parents.split_whitespace().map(str::to_string).collect(),
        ));
    }
    let shape: Vec<(String, Vec<String>)> = commits
        .iter()
        .map(|(c, p)| (c.hash.clone(), p.clone()))
        .collect();
    let more = commits.len() >= limit;
    let rows = graph::layout(&shape)
        .into_iter()
        .zip(commits)
        .map(|(cells, (commit, _))| GraphRow { cells, commit })
        .collect();
    History { rows, more }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commits_with_parents_into_graph_rows() {
        let text = "m\x1fm1\x1fa b\x1fMerge b\n\
b\x1fb1\x1fa\x1fon b\n\
a\x1fa1\x1f\x1froot\n";
        let h = parse(text, 100);
        let rows: Vec<String> = h
            .rows
            .iter()
            .map(|r| r.cells.iter().map(|c| c.ch).collect())
            .collect();
        assert_eq!(rows, ["●─╮", "│ ●", "●─╯"]);
        assert_eq!(h.rows[0].commit.subject, "Merge b");
        assert!(!h.more);
        assert!(parse(text, 3).more);
    }
}
