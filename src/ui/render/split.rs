//! Side-by-side layout of a diff: old lines on the left, new lines on the
//! right. A context line shows on both sides; a run of removed lines pairs
//! row by row with the run of added lines right after it (the shorter side
//! is padded). Every other line (titles, meta, commit headers, gaps) keeps
//! the full width.

use crate::git::diff::{DiffLine, LineKind};

/// Diff pane inner width from which the side-by-side layout is used.
pub(crate) const SPLIT_MIN_W: usize = 120;

/// `old ` / `new ` number column on each side.
pub(super) const SIDE_GUTTER_W: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    /// A line drawn across the pane (index into the lines).
    Full(usize),
    /// The old side and the new side; either may be empty.
    Pair(Option<usize>, Option<usize>),
}

pub(super) fn pair(lines: &[DiffLine]) -> Vec<Item> {
    let run = |from: usize, kind: LineKind| {
        (from..lines.len())
            .take_while(|&j| lines[j].kind == kind)
            .collect::<Vec<_>>()
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        match lines[i].kind {
            LineKind::Context => {
                out.push(Item::Pair(Some(i), Some(i)));
                i += 1;
            }
            LineKind::Del | LineKind::Add => {
                let dels = run(i, LineKind::Del);
                let adds = run(i + dels.len(), LineKind::Add);
                for k in 0..dels.len().max(adds.len()) {
                    out.push(Item::Pair(dels.get(k).copied(), adds.get(k).copied()));
                }
                i += dels.len() + adds.len();
            }
            _ => {
                out.push(Item::Full(i));
                i += 1;
            }
        }
    }
    out
}

/// Widths of the left and right sides (a `│` sits between them).
pub(super) fn sides(full_w: usize) -> (usize, usize) {
    let left = full_w.saturating_sub(1) / 2;
    (left, full_w.saturating_sub(1 + left))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(kinds: &[LineKind]) -> Vec<DiffLine> {
        kinds
            .iter()
            .map(|&kind| DiffLine {
                kind,
                text: String::new(),
                old: None,
                new: None,
                marks: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn removed_runs_pair_with_the_added_runs_after_them() {
        use LineKind::*;
        let l = lines(&[Section, Context, Del, Del, Add, Context, Add, Gap, Del]);
        assert_eq!(
            pair(&l),
            [
                Item::Full(0),
                Item::Pair(Some(1), Some(1)),
                Item::Pair(Some(2), Some(4)),
                Item::Pair(Some(3), None),
                Item::Pair(Some(5), Some(5)),
                Item::Pair(None, Some(6)),
                Item::Full(7),
                Item::Pair(Some(8), None),
            ]
        );
        assert_eq!(sides(121), (60, 60));
        assert_eq!(sides(122), (60, 61));
    }
}
