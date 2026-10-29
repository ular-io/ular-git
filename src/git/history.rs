//! Commit history with git's branch graph (`git log --graph`).
//!
//! git lays the graph out; each output line is kept as its graph prefix (the
//! raw `* | / \ _ -` characters, mapped to box drawing at render time) and,
//! on commit lines, the commit itself. Lines with only graph are connectors
//! between commits.

use super::{run, Commit};
use std::path::Path;

/// Commits read first; "more" doubles it.
pub const FIRST_PAGE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// git's graph characters for this line (trailing spaces trimmed).
    pub graph: String,
    /// The commit on this line; `None` for a connector line.
    pub commit: Option<Commit>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    pub rows: Vec<GraphRow>,
    /// The page was full, so older commits exist.
    pub more: bool,
}

/// Field separator in the format string; never appears in graph characters.
const SEP: char = '\x1f';

/// The newest `limit` commits reachable from HEAD, with the graph. A
/// repository without commits has an empty history.
pub fn load(dir: &Path, limit: usize) -> anyhow::Result<History> {
    let n = format!("-n{limit}");
    let out = run(
        dir,
        &[
            "log",
            "--graph",
            "--no-color",
            &n,
            "--format=%x1f%H%x1f%h%x1f%s",
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

pub fn parse(text: &str, limit: usize) -> History {
    let mut rows = Vec::new();
    let mut commits = 0;
    for line in text.lines() {
        match line.split_once(SEP) {
            Some((graph, rest)) => {
                let mut f = rest.split(SEP);
                let (Some(hash), Some(short)) = (f.next(), f.next()) else {
                    continue;
                };
                commits += 1;
                rows.push(GraphRow {
                    graph: graph.trim_end().to_string(),
                    commit: Some(Commit {
                        hash: hash.to_string(),
                        short: short.to_string(),
                        subject: f.next().unwrap_or("").to_string(),
                    }),
                });
            }
            None => rows.push(GraphRow {
                graph: line.trim_end().to_string(),
                commit: None,
            }),
        }
    }
    // Connector lines after the last commit lead nowhere on this page.
    while rows.last().is_some_and(|r| r.commit.is_none()) {
        rows.pop();
    }
    History {
        rows,
        more: commits >= limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_graph_from_commits_and_keeps_connectors() {
        let text = "* \x1faaa\x1fa1\x1ftop\n\
*   \x1fbbb\x1fb1\x1fMerge branch 'x'\n\
|\\  \n\
| * \x1fccc\x1fc1\x1fon x\n\
* | \x1fddd\x1fd1\x1fon main\n\
|/  \n\
* \x1feee\x1fe1\x1froot\n";
        let h = parse(text, 100);
        let graphs: Vec<&str> = h.rows.iter().map(|r| r.graph.as_str()).collect();
        assert_eq!(graphs, ["*", "*", "|\\", "| *", "* |", "|/", "*"]);
        assert_eq!(
            h.rows[1].commit.as_ref().unwrap().subject,
            "Merge branch 'x'"
        );
        assert!(h.rows[2].commit.is_none());
        assert!(!h.more);
    }

    #[test]
    fn a_full_page_means_more_and_trailing_connectors_are_dropped() {
        let text = "* \x1fa\x1fa\x1fs\n|\\\n";
        let h = parse(text, 1);
        assert!(h.more);
        assert_eq!(h.rows.len(), 1);
    }
}
