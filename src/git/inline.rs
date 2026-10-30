//! Changes within a line: a run of removed lines pairs, in order, with the
//! run of added lines right after it (as the side-by-side layout does), and
//! each pair is compared word by word. The words that differ become `marks`
//! on both lines. Pairs less than half alike are left unmarked: marking most
//! of both lines says nothing a removed / added color does not.

use super::diff::{DiffLine, LineKind};

/// Lines longer than this (bytes) are not compared.
const LINE_MAX: usize = 1000;
/// Token count per line above which the comparison is skipped.
const TOKENS_MAX: usize = 300;

pub fn mark(lines: &mut [DiffLine]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Del {
            i += 1;
            continue;
        }
        let dels = (i..lines.len())
            .take_while(|&j| lines[j].kind == LineKind::Del)
            .count();
        let adds = (i + dels..lines.len())
            .take_while(|&j| lines[j].kind == LineKind::Add)
            .count();
        for k in 0..dels.min(adds) {
            let (d, a) = (i + k, i + dels + k);
            if let Some((dm, am)) = compare(&lines[d].text, &lines[a].text) {
                lines[d].marks = dm;
                lines[a].marks = am;
            }
        }
        i += dels + adds;
    }
}

type Marks = Vec<(usize, usize)>;

/// Marks for a removed and an added line (both with their `-` / `+`
/// prefix), or `None` when they are too long, too different, or equal.
fn compare(old: &str, new: &str) -> Option<(Marks, Marks)> {
    if old.len() > LINE_MAX || new.len() > LINE_MAX {
        return None;
    }
    let (a, b) = (tokens(old), tokens(new));
    if a.len() > TOKENS_MAX || b.len() > TOKENS_MAX {
        return None;
    }
    let keep = lcs(old, &a, new, &b);
    let same: usize = keep.iter().map(|&(i, _)| a[i].1 - a[i].0).sum();
    // Alike = twice the kept bytes over both lines' bytes (prefixes aside).
    let size = old.len().saturating_sub(1) + new.len().saturating_sub(1);
    if size == 0 || same * 4 < size || same * 2 == size {
        // Under half alike, or identical.
        return None;
    }
    let kept_a: Vec<usize> = keep.iter().map(|&(i, _)| i).collect();
    let kept_b: Vec<usize> = keep.iter().map(|&(_, j)| j).collect();
    Some((ranges(&a, &kept_a), ranges(&b, &kept_b)))
}

/// Byte ranges of words, runs of spaces, and single other characters, after
/// the one-byte `-` / `+` prefix.
fn tokens(text: &str) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    };
    let mut prev: Option<u8> = None;
    for (i, c) in text.char_indices().skip(1) {
        let k = class(c);
        let end = i + c.len_utf8();
        match out.last_mut() {
            Some(last) if prev == Some(k) && k != 2 => last.1 = end,
            _ => out.push((i, end)),
        }
        prev = Some(k);
    }
    out
}

/// Longest common subsequence of equal tokens: index pairs, in order.
fn lcs(sa: &str, a: &[(usize, usize)], sb: &str, b: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let eq = |i: usize, j: usize| sa[a[i].0..a[i].1] == sb[b[j].0..b[j].1];
    let (n, m) = (a.len(), b.len());
    let mut t = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if eq(i, j) {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if eq(i, j) {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Merged byte ranges of the tokens not kept.
fn ranges(tokens: &[(usize, usize)], kept: &[usize]) -> Marks {
    let mut out: Marks = Vec::new();
    for (i, &(s, e)) in tokens.iter().enumerate() {
        if kept.contains(&i) {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.1 == s => last.1 = e,
            _ => out.push((s, e)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: LineKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            text: text.to_string(),
            old: None,
            new: None,
            marks: Vec::new(),
        }
    }

    fn marked(l: &DiffLine) -> Vec<&str> {
        l.marks.iter().map(|&(s, e)| &l.text[s..e]).collect()
    }

    #[test]
    fn changed_words_are_marked_on_both_lines() {
        let mut l = vec![
            line(
                LineKind::Del,
                r#"-const url = "http://localhost:8080/api";"#,
            ),
            line(LineKind::Add, r#"+const url = "https://example.com/api";"#),
        ];
        mark(&mut l);
        assert_eq!(marked(&l[0]), ["http", "localhost:8080"]);
        assert_eq!(marked(&l[1]), ["https", "example.com"]);
    }

    #[test]
    fn two_separate_changes_stay_two_marks() {
        let mut l = vec![
            line(LineKind::Del, "-let a = foo(1, 2);"),
            line(LineKind::Add, "+let b = foo(1, 3);"),
        ];
        mark(&mut l);
        assert_eq!(marked(&l[0]), ["a", "2"]);
        assert_eq!(marked(&l[1]), ["b", "3"]);
    }

    #[test]
    fn unlike_lone_or_equal_lines_stay_unmarked() {
        let mut l = vec![
            line(LineKind::Del, "-fn alpha() {}"),
            line(LineKind::Add, "+let x = vec![1, 2, 3];"),
            line(LineKind::Context, " same"),
            line(LineKind::Add, "+only added"),
            line(LineKind::Del, "-same text"),
            line(LineKind::Add, "+same text"),
        ];
        mark(&mut l);
        assert!(l.iter().all(|l| l.marks.is_empty()), "{l:?}");
    }

    #[test]
    fn non_ascii_words_are_marked_on_char_boundaries() {
        let mut l = vec![
            line(LineKind::Del, "-제목: 이전 이름 변경"),
            line(LineKind::Add, "+제목: 새로운 이름 변경"),
        ];
        mark(&mut l);
        assert_eq!(marked(&l[0]), ["이전"]);
        assert_eq!(marked(&l[1]), ["새로운"]);
    }
}
