//! Diff text for one changed file or one unpushed commit, classified per line
//! with old/new line numbers so the diff pane can color and number it.

use super::{git_command, Commit, EntryKind, FileEntry};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;

/// What the diff pane shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    File(FileEntry),
    Commit(Commit),
    /// Every uncommitted change, as a stat (see [`super::stat`]).
    WorkTree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A file diff's path in the repository (`old → new` for a rename), the
    /// first line; the pane title names only the file.
    Path,
    /// A file diff's line counts (`+20 -3`), right below its path.
    Counts,
    /// Section title inserted by ular-git (`Staged changes`).
    Section,
    /// Date, author name, and email of a shown commit (one line).
    Byline,
    /// First line of a shown commit's message.
    Subject,
    /// Rest of a shown commit's message.
    Header,
    /// The `--stat` block of a shown commit.
    Stat,
    /// git's `---` separator between a shown commit's message and its stat,
    /// drawn as a full-width rule. Its text becomes the stat summary
    /// (`N files changed, …`) when there is one, shown centered on it.
    Rule,
    /// Per-file title of a shown commit (`── path ──`), in place of git's
    /// `diff --git` / `index` / `---` / `+++` lines.
    File,
    /// Informative file header lines git prints before a file's hunks: new or
    /// deleted file, mode change, rename, similarity, binary. (`diff --git`,
    /// `index`, `---`, and `+++` are dropped.)
    Meta,
    /// Lines skipped between two hunks of the same file (git's `@@` line
    /// after the first one). The first hunk's `@@` line is dropped: the line
    /// numbers say where it starts.
    Gap,
    Add,
    Del,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    pub old: Option<u32>,
    pub new: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffDoc {
    pub lines: Vec<DiffLine>,
    /// Output was cut at the line limit.
    pub truncated: bool,
    pub added: usize,
    pub removed: usize,
}

mod parse;
use parse::push_parsed;
#[cfg(test)]
use parse::{header_path, parse_hunk};

const BASE: &[&str] = &["--no-color", "--no-ext-diff"];

/// Loads the diff for `source` in the working tree at `dir`, reading at most
/// `max_lines` lines of git output in total.
pub fn load(dir: &Path, source: &DiffSource, max_lines: usize) -> anyhow::Result<DiffDoc> {
    let mut doc = DiffDoc::default();
    match source {
        DiffSource::WorkTree => return super::stat::load(dir, max_lines),
        DiffSource::Commit(c) => {
            let mut args = vec!["show"];
            args.extend(BASE);
            // One byline (date, then author), the raw message right below it;
            // the hash is the pane title, and the committer is left out.
            args.extend([
                "--stat",
                "--patch",
                "--date=format:%Y-%m-%d %H:%M",
                "--format=%ad  %an <%ae>%n%B",
                c.hash.as_str(),
            ]);
            let (raw, cut) = run_limited(dir, &args, max_lines)?;
            push_parsed(&mut doc, &raw, true);
            doc.truncated = cut;
        }
        DiffSource::File(f) => {
            let mut sections: Vec<(&str, Vec<&str>)> = Vec::new();
            let path = f.path.as_str();
            match f.kind {
                EntryKind::Untracked => {
                    let mut a = vec!["diff"];
                    a.extend(BASE);
                    a.extend(["--no-index", "--", "/dev/null", path]);
                    sections.push(("Untracked file", a));
                }
                EntryKind::Conflict => {
                    let mut a = vec!["diff"];
                    a.extend(BASE);
                    a.extend(["--", path]);
                    sections.push(("Merge conflict", a));
                }
                EntryKind::Tracked => {
                    if f.staged() {
                        let mut a = vec!["diff"];
                        a.extend(BASE);
                        a.extend(["--cached", "-M", "--"]);
                        if let Some(orig) = f.orig_path.as_deref() {
                            a.push(orig);
                        }
                        a.push(path);
                        sections.push(("Staged changes", a));
                    }
                    if f.unstaged() {
                        let mut a = vec!["diff"];
                        a.extend(BASE);
                        a.extend(["--", path]);
                        sections.push(("Unstaged changes", a));
                    }
                }
            }
            for (title, args) in sections {
                let budget = max_lines.saturating_sub(doc.lines.len());
                if budget == 0 {
                    doc.truncated = true;
                    break;
                }
                doc.lines.push(line_of(LineKind::Section, title));
                let (raw, cut) = run_limited(dir, &args, budget)?;
                if raw.iter().all(|l| l.trim().is_empty()) {
                    doc.lines.push(meta("(no textual changes)"));
                }
                push_parsed(&mut doc, &raw, false);
                doc.truncated |= cut;
            }
            let path = match f.orig_path.as_deref() {
                Some(orig) => format!("{orig} → {}", f.path),
                None => f.path.clone(),
            };
            let mut head = vec![line_of(LineKind::Path, &path)];
            let counts = counts_label(doc.added, doc.removed);
            if !counts.is_empty() {
                head.push(line_of(LineKind::Counts, &counts));
            }
            doc.lines.splice(0..0, head);
        }
    }
    Ok(doc)
}

/// `+N -N` with a zero side left out (`+42` for a new file); empty when no
/// line changed (binary, mode only).
pub fn counts_label(added: usize, removed: usize) -> String {
    let parts = [(added, '+'), (removed, '-')];
    parts
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, sign)| format!("{sign}{n}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn line_of(kind: LineKind, text: &str) -> DiffLine {
    DiffLine {
        kind,
        text: text.to_string(),
        old: None,
        new: None,
    }
}

fn meta(text: &str) -> DiffLine {
    line_of(LineKind::Meta, text)
}

/// Runs git reading at most `max` stdout lines; the child is killed once the
/// limit is hit so a huge untracked file never has to be read in full.
/// `git diff --no-index` exits 1 when files differ, so only stderr output with
/// an empty stdout counts as failure.
fn run_limited(dir: &Path, args: &[&str], max: usize) -> anyhow::Result<(Vec<String>, bool)> {
    let mut child = git_command(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("cannot run git: {e}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = Vec::new();
    let mut cut = false;
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if lines.len() >= max {
                    cut = true;
                    break;
                }
                lines.push(clean_line(&buf));
            }
        }
    }
    if cut {
        let _ = child.kill();
    }
    let mut err = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut err);
    }
    let status = child.wait()?;
    if !cut && lines.is_empty() && !status.success() {
        if let Some(line) = err.lines().map(str::trim).find(|l| !l.is_empty()) {
            anyhow::bail!("{}", line.trim_start_matches("fatal: "));
        }
    }
    Ok((lines, cut))
}

/// Decodes one output line: lossy UTF-8, tabs expanded to 4 columns, line
/// ending and other control characters dropped so they cannot move the
/// terminal cursor.
fn clean_line(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());
    let mut col = 0usize;
    for c in text.trim_end_matches(['\n', '\r']).chars() {
        if c == '\t' {
            let n = 4 - col % 4;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else if !c.is_control() {
            out.push(c);
            col += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
