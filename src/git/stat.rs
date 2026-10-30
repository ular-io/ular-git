//! Working-tree summary shown when the history's top row is selected: every
//! uncommitted change against HEAD (staged and unstaged together) and every
//! untracked file, as a stat block under a summary rule, like a commit's.

use super::diff::{DiffDoc, DiffLine, LineKind};
use super::run;
use std::io::Read;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

/// Widest path column; longer paths keep their tail.
const PATH_MAX: usize = 40;
/// Widest `+++--` bar.
const BAR_MAX: usize = 20;
/// Bytes read from an untracked file to count its lines.
const READ_MAX: u64 = 1 << 20;

/// One stat row: path and its added / removed lines (`None` for binary).
struct Entry {
    path: String,
    lines: Option<(usize, usize)>,
}

pub fn load(dir: &Path, max_lines: usize) -> anyhow::Result<DiffDoc> {
    let mut entries = tracked(dir)?;
    for path in untracked(dir)? {
        let lines = count_lines(&dir.join(&path)).map(|n| (n, 0));
        entries.push(Entry { path, lines });
    }
    let mut doc = DiffDoc::default();
    if entries.is_empty() {
        doc.lines.push(line(LineKind::Meta, "Working tree clean"));
        return Ok(doc);
    }
    doc.truncated = entries.len() + 1 > max_lines;
    entries.truncate(max_lines.saturating_sub(1));
    for e in &entries {
        if let Some((a, d)) = e.lines {
            doc.added += a;
            doc.removed += d;
        }
    }
    doc.lines.push(line(
        LineKind::Rule,
        &summary(entries.len(), doc.added, doc.removed),
    ));
    let path_w = entries
        .iter()
        .map(|e| e.path.width())
        .max()
        .unwrap_or(0)
        .min(PATH_MAX);
    let biggest = entries
        .iter()
        .filter_map(|e| e.lines.map(|(a, d)| a + d))
        .max()
        .unwrap_or(0);
    let count_w = biggest.to_string().len();
    for e in &entries {
        let path = tail(&e.path, path_w);
        let pad = " ".repeat(path_w.saturating_sub(path.width()));
        let text = match e.lines {
            Some((a, d)) => {
                let (pa, pd) = bar(a, d, biggest);
                format!(
                    " {path}{pad} | {:>count_w$} {}{}",
                    a + d,
                    "+".repeat(pa),
                    "-".repeat(pd)
                )
            }
            None => format!(" {path}{pad} | Bin"),
        };
        doc.lines.push(line(LineKind::Stat, &text));
    }
    Ok(doc)
}

fn line(kind: LineKind, text: &str) -> DiffLine {
    DiffLine {
        kind,
        text: text.to_string(),
        old: None,
        new: None,
        marks: Vec::new(),
    }
}

/// git's wording: `2 files changed, 5 insertions(+), 1 deletion(-)`.
fn summary(files: usize, added: usize, removed: usize) -> String {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut s = plural(files, "file changed", "files changed");
    if added > 0 {
        s.push_str(&format!(
            ", {}",
            plural(added, "insertion(+)", "insertions(+)")
        ));
    }
    if removed > 0 {
        s.push_str(&format!(
            ", {}",
            plural(removed, "deletion(-)", "deletions(-)")
        ));
    }
    s
}

/// `+` and `-` cells for one file, scaled so the biggest change fills the bar.
fn bar(added: usize, removed: usize, biggest: usize) -> (usize, usize) {
    let total = added + removed;
    if biggest <= BAR_MAX || total == 0 {
        return (added, removed);
    }
    let cells = (total * BAR_MAX).div_ceil(biggest).max(1);
    let plus = (added * cells).div_ceil(total).min(cells);
    (plus, cells - plus)
}

/// `…` and the end of `path` within `max` columns.
fn tail(path: &str, max: usize) -> String {
    if path.width() <= max {
        return path.to_string();
    }
    let mut out: Vec<char> = Vec::new();
    let mut w = 1;
    for c in path.chars().rev() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > max {
            break;
        }
        w += cw;
        out.push(c);
    }
    std::iter::once('…').chain(out.into_iter().rev()).collect()
}

/// `git diff HEAD --numstat -z` (staged and unstaged together); against the
/// index alone on an unborn branch.
fn tracked(dir: &Path) -> anyhow::Result<Vec<Entry>> {
    let mut out = run(
        dir,
        &["diff", "--no-ext-diff", "--numstat", "-z", "-M", "HEAD"],
    )?;
    if !out.status.success() {
        out = run(
            dir,
            &["diff", "--no-ext-diff", "--numstat", "-z", "--cached"],
        )?;
        if !out.status.success() {
            anyhow::bail!("{}", super::stderr_summary(&out));
        }
    }
    Ok(parse_numstat(&String::from_utf8_lossy(&out.stdout)))
}

/// `added TAB removed TAB path NUL`, or for a rename
/// `added TAB removed TAB NUL old NUL new NUL`; `-` counts mean binary.
fn parse_numstat(text: &str) -> Vec<Entry> {
    let mut fields = text.split('\0');
    let mut entries = Vec::new();
    while let Some(head) = fields.next() {
        let mut parts = head.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
                break;
            };
            format!("{old} → {new}")
        } else {
            path.to_string()
        };
        let lines = a.parse().ok().zip(d.parse().ok());
        entries.push(Entry { path, lines });
    }
    entries
}

fn untracked(dir: &Path) -> anyhow::Result<Vec<String>> {
    let out = run(dir, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if !out.status.success() {
        anyhow::bail!("{}", super::stderr_summary(&out));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect())
}

/// Lines of a text file (the first MiB); `None` for binary or unreadable.
fn count_lines(path: &Path) -> Option<usize> {
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(READ_MAX)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.contains(&0) {
        return None;
    }
    let n = buf.iter().filter(|&&b| b == b'\n').count();
    Some(n + usize::from(buf.last().is_some_and(|&b| b != b'\n')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_reads_counts_renames_and_binaries() {
        let e = parse_numstat("3\t1\tsrc/a.rs\0-\t-\timg.png\x000\t0\t\0old.md\0new.md\0");
        let got: Vec<(&str, Option<(usize, usize)>)> =
            e.iter().map(|e| (e.path.as_str(), e.lines)).collect();
        assert_eq!(
            got,
            [
                ("src/a.rs", Some((3, 1))),
                ("img.png", None),
                ("old.md → new.md", Some((0, 0))),
            ]
        );
    }

    #[test]
    fn summary_and_bar_follow_git() {
        assert_eq!(summary(1, 2, 0), "1 file changed, 2 insertions(+)");
        assert_eq!(
            summary(3, 1, 4),
            "3 files changed, 1 insertion(+), 4 deletions(-)"
        );
        assert_eq!(bar(3, 1, 10), (3, 1));
        assert_eq!(bar(100, 100, 200), (10, 10));
        assert_eq!(tail("a/b/c/long.rs", 8), "…long.rs");
    }
}
