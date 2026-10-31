//! Line-number gutter of the diff pane, sized to the document: each number
//! column is as wide as the largest number it shows anywhere in the diff (so
//! the width holds while scrolling), and a column with no numbers at all (a
//! new file has no old numbers, a deleted file no new ones) is left out.

use crate::git::diff::{DiffLine, LineKind};

/// Digit widths of the old and new number columns; 0 means the column is
/// left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Gutter {
    old: usize,
    new: usize,
}

/// Lines that carry old/new numbers (and the gutter).
pub(super) fn is_code(kind: LineKind) -> bool {
    matches!(kind, LineKind::Add | LineKind::Del | LineKind::Context)
}

fn digits(n: u32) -> usize {
    n.checked_ilog10().map_or(1, |d| d as usize + 1)
}

impl Gutter {
    pub(super) fn of(lines: &[DiffLine]) -> Self {
        let width = |pick: fn(&DiffLine) -> Option<u32>| {
            lines
                .iter()
                .filter(|l| is_code(l.kind))
                .filter_map(pick)
                .max()
                .map_or(0, digits)
        };
        Gutter {
            old: width(|l| l.old),
            new: width(|l| l.new),
        }
    }

    /// No code line carries a number.
    pub(super) fn is_empty(&self) -> bool {
        self.old == 0 && self.new == 0
    }

    /// Unified layout: each shown column and one space after it.
    pub(super) fn width(&self) -> usize {
        self.side(true) + self.side(false)
    }

    /// One column and its space (`old` picks the old column); 0 when the
    /// column is left out. Each side of the side-by-side layout uses its own.
    pub(super) fn side(&self, old: bool) -> usize {
        match if old { self.old } else { self.new } {
            0 => 0,
            w => w + 1,
        }
    }

    /// One column's number, right-aligned, and its space; blank when `n` is
    /// `None` or `blank` (a wrapped continuation).
    pub(super) fn side_text(&self, old: bool, n: Option<u32>, blank: bool) -> String {
        let w = if old { self.old } else { self.new };
        match n.filter(|_| !blank && w > 0) {
            Some(n) => format!("{n:>w$} "),
            None => " ".repeat(self.side(old)),
        }
    }

    /// Unified layout: both shown columns of `line`.
    pub(super) fn text(&self, line: &DiffLine, blank: bool) -> String {
        self.side_text(true, line.old, blank) + &self.side_text(false, line.new, blank)
    }

    /// One side of the side-by-side layout (`old` is the left side): its
    /// column, after one space on the right side so its numbers do not touch
    /// the `│` divider.
    pub(super) fn split_side(&self, old: bool) -> usize {
        self.side(old) + usize::from(!old)
    }

    /// One side's number text, `split_side` wide.
    pub(super) fn split_text(&self, old: bool, n: Option<u32>, blank: bool) -> String {
        let lead = if old { "" } else { " " };
        format!("{lead}{}", self.side_text(old, n, blank))
    }

    /// The hunk gap marker, right-aligned in the first shown column.
    pub(super) fn gap(&self) -> String {
        let w = if self.old > 0 { self.old } else { self.new };
        format!("{:>w$}", "⋯", w = w.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            text: String::new(),
            old,
            new,
            marks: Vec::new(),
        }
    }

    #[test]
    fn columns_fit_the_largest_number_and_drop_when_empty() {
        use LineKind::*;
        let edited = [
            line(Context, Some(9), Some(9)),
            line(Del, Some(10), None),
            line(Add, None, Some(120)),
        ];
        let g = Gutter::of(&edited);
        assert_eq!(g.width(), 3 + 4);
        assert_eq!(g.text(&edited[1], false), "10     ");
        assert_eq!(g.text(&edited[2], false), "   120 ");
        assert_eq!(g.text(&edited[2], true), "       ");
        assert_eq!(g.gap(), " ⋯");
        assert_eq!(g.split_text(true, Some(9), false), " 9 ");
        assert_eq!(g.split_text(false, Some(9), false), "   9 ");
        assert_eq!(g.split_side(false), 5);

        // A new file: no old numbers, so only the new column shows.
        let new_file = [line(Add, None, Some(1)), line(Add, None, Some(18))];
        let g = Gutter::of(&new_file);
        assert_eq!((g.side(true), g.side(false)), (0, 3));
        assert_eq!(g.text(&new_file[0], false), " 1 ");
        assert_eq!(g.gap(), " ⋯");

        // Numbers past four digits keep the code aligned.
        let long = [line(Context, Some(12345), Some(12346))];
        assert_eq!(Gutter::of(&long).text(&long[0], false), "12345 12346 ");

        // Only code lines count; no numbers means no gutter.
        assert!(Gutter::of(&[line(Header, Some(99), None)]).is_empty());
    }
}
