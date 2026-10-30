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

/// Classifies raw diff output. Inside a hunk the remaining old/new counts from
/// its header decide where content ends, so a removed line reading `-- x` is
/// not mistaken for a `---` file header. `show` marks the lines before the
/// first `diff --git` as the commit header: byline, subject, the rest of the
/// message, then the stat block from git's last `---` separator (a message
/// may itself contain a `---` line).
fn push_parsed(doc: &mut DiffDoc, raw: &[String], show: bool) {
    let header_end = raw
        .iter()
        .position(|l| l.starts_with("diff "))
        .unwrap_or(raw.len());
    let stat_start = raw[..header_end]
        .iter()
        .rposition(|l| l == "---")
        .unwrap_or(header_end);
    let mut in_header = show;
    let mut rename_from: Option<String> = None;
    let mut file_title: Option<usize> = None;
    let mut header_seen = 0usize;
    let mut subject_done = false;
    let mut old_left = 0u32;
    let mut new_left = 0u32;
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut combined = false;
    for (index, text) in raw.iter().enumerate() {
        if text.starts_with("diff ") {
            in_header = false;
            old_left = 0;
            new_left = 0;
            combined = false;
            // A single-file diff already names its file in the pane title.
            if show {
                // The file title is the separator: no blank line before it.
                while doc
                    .lines
                    .last()
                    .is_some_and(|l| l.kind == LineKind::Stat && l.text.trim().is_empty())
                {
                    doc.lines.pop();
                }
                file_title = Some(doc.lines.len());
                doc.lines.push(DiffLine {
                    kind: LineKind::File,
                    text: header_path(text),
                    old: None,
                    new: None,
                });
            }
            continue;
        }
        if in_header {
            // The message is shown compact: no blank lines, so the rule sits
            // right under its last line.
            if index < stat_start && text.trim().is_empty() {
                continue;
            }
            let kind = if index == stat_start {
                LineKind::Rule
            } else if index > stat_start {
                LineKind::Stat
            } else if header_seen == 0 {
                LineKind::Byline
            } else if !subject_done && !text.trim().is_empty() {
                subject_done = true;
                LineKind::Subject
            } else {
                LineKind::Header
            };
            header_seen += 1;
            doc.lines.push(DiffLine {
                kind,
                text: text.clone(),
                old: None,
                new: None,
            });
            continue;
        }
        if text.starts_with("@@") {
            combined = text.starts_with("@@@");
            if let Some((o, ol, n, nl)) = parse_hunk(text) {
                (old_no, old_left, new_no, new_left) = (o, ol, n, nl);
            }
            let after_hunk = doc.lines.last().is_some_and(|l| {
                matches!(l.kind, LineKind::Add | LineKind::Del | LineKind::Context)
            });
            if after_hunk {
                doc.lines.push(DiffLine {
                    kind: LineKind::Gap,
                    text: "⋯".to_string(),
                    old: None,
                    new: None,
                });
            }
            continue;
        }
        let in_hunk = combined || old_left > 0 || new_left > 0;
        let line = if in_hunk {
            content_line(
                text,
                combined,
                &mut old_no,
                &mut new_no,
                &mut old_left,
                &mut new_left,
            )
        } else {
            if is_redundant_header(text) {
                continue;
            }
            // A rename goes into the commit's file title (`old → new`).
            if show {
                if let Some(from) = text.strip_prefix("rename from ") {
                    rename_from = Some(from.to_string());
                    continue;
                }
                if let Some(to) = text.strip_prefix("rename to ") {
                    if let (Some(from), Some(t)) = (rename_from.take(), file_title) {
                        doc.lines[t].text = format!("{from} → {to}");
                    }
                    continue;
                }
            }
            meta(text)
        };
        match line.kind {
            LineKind::Add => doc.added += 1,
            LineKind::Del => doc.removed += 1,
            _ => {}
        }
        doc.lines.push(line);
    }
    if show {
        fold_stat_summary(doc);
    }
}

/// Moves the stat's summary line (`N files changed, …`) into the rule above
/// the stat, where it is drawn as the rule's right-aligned label.
fn fold_stat_summary(doc: &mut DiffDoc) {
    let Some(rule) = doc.lines.iter().position(|l| l.kind == LineKind::Rule) else {
        return;
    };
    let stat_end = doc.lines[rule + 1..]
        .iter()
        .position(|l| l.kind != LineKind::Stat)
        .map_or(doc.lines.len(), |n| rule + 1 + n);
    let summary = (rule + 1..stat_end).rev().find(|&i| {
        let t = doc.lines[i].text.trim();
        t.starts_with(|c: char| c.is_ascii_digit()) && t.contains(" changed")
    });
    if let Some(i) = summary {
        let line = doc.lines.remove(i);
        doc.lines[rule].text = line.text.trim().to_string();
    }
}

/// git file header lines that add nothing on screen: the blob ids and the
/// `---` / `+++` path lines (the path is in the title).
fn is_redundant_header(text: &str) -> bool {
    text.starts_with("index ") || text.starts_with("--- ") || text.starts_with("+++ ")
}

/// File path of a `diff --git a/<p> b/<p>` or `diff --cc <p>` line. For
/// `a/<x> b/<y>` with different halves (a rename), the new path; the rename
/// lines that follow refine the title.
fn header_path(text: &str) -> String {
    if let Some(p) = text
        .strip_prefix("diff --cc ")
        .or(text.strip_prefix("diff --combined "))
    {
        return p.to_string();
    }
    let rest = text.strip_prefix("diff --git ").unwrap_or(text);
    // Equal halves: `a/<p> b/<p>`, even when <p> contains " b/".
    let n = rest.len();
    if n % 2 == 1 {
        let half = (n - 1) / 2;
        if rest.is_char_boundary(half) && rest.as_bytes()[half] == b' ' {
            let (a, b) = (&rest[..half], &rest[half + 1..]);
            if let (Some(a), Some(b)) = (a.strip_prefix("a/"), b.strip_prefix("b/")) {
                if a == b {
                    return a.to_string();
                }
            }
        }
    }
    match rest.rfind(" b/") {
        Some(i) => rest[i + 3..].to_string(),
        None => rest.to_string(),
    }
}

fn content_line(
    text: &str,
    combined: bool,
    old_no: &mut u32,
    new_no: &mut u32,
    old_left: &mut u32,
    new_left: &mut u32,
) -> DiffLine {
    let prefix: String = text.chars().take(if combined { 2 } else { 1 }).collect();
    let (kind, old, new) = if prefix.starts_with('\\') {
        (LineKind::Meta, None, None)
    } else if prefix.contains('+') {
        *new_left = new_left.saturating_sub(1);
        *new_no += 1;
        (LineKind::Add, None, Some(*new_no - 1))
    } else if prefix.contains('-') {
        *old_left = old_left.saturating_sub(1);
        *old_no += 1;
        (LineKind::Del, Some(*old_no - 1), None)
    } else {
        *old_left = old_left.saturating_sub(1);
        *new_left = new_left.saturating_sub(1);
        *old_no += 1;
        *new_no += 1;
        (LineKind::Context, Some(*old_no - 1), Some(*new_no - 1))
    };
    let (old, new) = if combined { (None, None) } else { (old, new) };
    DiffLine {
        kind,
        text: text.to_string(),
        old,
        new,
    }
}

/// `@@ -a,b +c,d @@` → `(a, b, c, d)`; a missing count means 1.
fn parse_hunk(text: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = text.split_whitespace().skip(1);
    let range = |s: &str| -> Option<(u32, u32)> {
        let s = &s[1..];
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let old = parts.next().filter(|p| p.starts_with('-'))?;
    let new = parts.next().filter(|p| p.starts_with('+'))?;
    let (o, ol) = range(old)?;
    let (n, nl) = range(new)?;
    Some((o, ol, n, nl))
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
