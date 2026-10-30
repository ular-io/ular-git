//! Classification of raw `git diff` / `git show` output into diff lines:
//! commit header, stat, file headers, hunks, and line numbers.

use super::{meta, DiffDoc, DiffLine, LineKind};

/// Classifies raw diff output. Inside a hunk the remaining old/new counts from
/// its header decide where content ends, so a removed line reading `-- x` is
/// not mistaken for a `---` file header. `show` marks the lines before the
/// first `diff --git` as the commit header: byline, subject, the rest of the
/// message, then the stat block from git's last `---` separator (a message
/// may itself contain a `---` line).
pub(super) fn push_parsed(doc: &mut DiffDoc, raw: &[String], show: bool) {
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
                    marks: Vec::new(),
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
                marks: Vec::new(),
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
                    marks: Vec::new(),
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
pub(super) fn header_path(text: &str) -> String {
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
        marks: Vec::new(),
    }
}

/// `@@ -a,b +c,d @@` → `(a, b, c, d)`; a missing count means 1.
pub(super) fn parse_hunk(text: &str) -> Option<(u32, u32, u32, u32)> {
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
