//! Commit graph layout: one row per commit, lanes kept in place, branch and
//! merge joints drawn on the commit row (`●─╮`, `●─┤`, `●─╯`).
//!
//! Commits arrive newest first in topological order (`git log --topo-order`).
//! Each lane waits for one commit hash. A commit takes the leftmost lane
//! waiting for it; other lanes waiting for it end there (the branch started
//! at this commit). Its first parent continues its lane; every other parent
//! joins the lane already waiting for it or opens a new lane to the right.
//! A lane keeps its color for its whole length.

/// One character cell of a graph row and the color slot of the lane it
/// belongs to (`None` for blank cells).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub color: Option<usize>,
}

/// Where a joint on a commit row leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joint {
    /// A lane ends here: its branch started at this commit.
    End,
    /// A merge parent continues in a lane that already waits for it.
    Join,
    /// A merge parent opens a new lane.
    Open,
}

#[derive(Debug, Clone)]
struct Lane {
    waits_for: String,
    color: usize,
}

/// Lays out `commits` (`(hash, parents)`, newest first, topological order).
/// Returns one row of cells per commit; lanes sit on even columns, joints run
/// along odd ones.
pub fn layout(commits: &[(String, Vec<String>)]) -> Vec<Vec<Cell>> {
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color = 0;
    let mut rows = Vec::with_capacity(commits.len());
    for (hash, parents) in commits {
        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| l.as_ref().is_some_and(|l| &l.waits_for == hash))
            .map(|(i, _)| i)
            .collect();
        let (col, color) = match waiting.first() {
            Some(&i) => (i, lanes[i].as_ref().map_or(0, |l| l.color)),
            None => {
                let i = free_slot(&mut lanes, 0);
                next_color += 1;
                (i, next_color - 1)
            }
        };
        let before: Vec<Option<usize>> =
            lanes.iter().map(|l| l.as_ref().map(|l| l.color)).collect();

        let mut joints: Vec<(usize, Joint, usize)> = Vec::new();
        for &e in waiting.iter().skip(1) {
            let c = lanes[e].as_ref().map_or(color, |l| l.color);
            joints.push((e, Joint::End, c));
            lanes[e] = None;
        }
        lanes[col] = parents.first().map(|p| Lane {
            waits_for: p.clone(),
            color,
        });
        for p in parents.iter().skip(1) {
            match lanes
                .iter()
                .position(|l| l.as_ref().is_some_and(|l| &l.waits_for == p))
            {
                Some(j) => {
                    let c = lanes[j].as_ref().map_or(color, |l| l.color);
                    joints.push((j, Joint::Join, c));
                }
                None => {
                    let k = free_slot(&mut lanes, col + 1);
                    lanes[k] = Some(Lane {
                        waits_for: p.clone(),
                        color: next_color,
                    });
                    joints.push((k, Joint::Open, next_color));
                    next_color += 1;
                }
            }
        }
        let after: Vec<Option<usize>> = lanes.iter().map(|l| l.as_ref().map(|l| l.color)).collect();
        rows.push(draw_row(col, color, &joints, &before, &after));
        while lanes.last().is_some_and(|l| l.is_none()) {
            lanes.pop();
        }
    }
    rows
}

/// First free lane at or after `from`, growing the lane list when needed.
fn free_slot(lanes: &mut Vec<Option<Lane>>, from: usize) -> usize {
    if let Some(i) = (from..lanes.len()).find(|&i| lanes[i].is_none()) {
        return i;
    }
    while lanes.len() < from {
        lanes.push(None);
    }
    lanes.push(None);
    lanes.len() - 1
}

fn draw_row(
    col: usize,
    color: usize,
    joints: &[(usize, Joint, usize)],
    before: &[Option<usize>],
    after: &[Option<usize>],
) -> Vec<Cell> {
    let width = before.len().max(after.len()).max(col + 1);
    let lo = joints.iter().map(|j| j.0).chain([col]).min().unwrap_or(col);
    let hi = joints.iter().map(|j| j.0).chain([col]).max().unwrap_or(col);
    let lane_color = |i: usize| {
        after
            .get(i)
            .copied()
            .flatten()
            .or(before.get(i).copied().flatten())
    };
    // Color of the horizontal run at lane `i`: the joint it leads to.
    let run_color = |i: usize| -> Option<usize> {
        let toward = if i > col {
            joints.iter().filter(|j| j.0 >= i).min_by_key(|j| j.0)
        } else {
            joints.iter().filter(|j| j.0 <= i).max_by_key(|j| j.0)
        };
        toward.map(|j| j.2)
    };
    let mut cells = Vec::with_capacity(width * 2);
    for i in 0..width {
        let cell = if i == col {
            Cell {
                ch: '●',
                color: Some(color),
            }
        } else if let Some(&(_, joint, c)) = joints.iter().find(|j| j.0 == i) {
            let outer = i == hi || i == lo;
            let ch = match (joint, i > col, outer) {
                (Joint::End, true, true) => '╯',
                (Joint::Open, true, true) => '╮',
                (Joint::Join, true, true) => '┤',
                (Joint::End, false, true) => '╰',
                (Joint::Open, false, true) => '╭',
                (Joint::Join, false, true) => '├',
                (Joint::End, _, false) => '┴',
                (Joint::Open, _, false) => '┬',
                (Joint::Join, _, false) => '┼',
            };
            Cell { ch, color: Some(c) }
        } else if i > lo && i < hi {
            match lane_color(i) {
                Some(c) => Cell {
                    ch: '┼',
                    color: Some(c),
                },
                None => Cell {
                    ch: '─',
                    color: run_color(i),
                },
            }
        } else {
            match lane_color(i) {
                Some(c) => Cell {
                    ch: '│',
                    color: Some(c),
                },
                None => Cell {
                    ch: ' ',
                    color: None,
                },
            }
        };
        cells.push(cell);
        if i + 1 < width {
            cells.push(if i >= lo && i < hi {
                Cell {
                    ch: '─',
                    color: run_color(if i >= col { i + 1 } else { i }),
                }
            } else {
                Cell {
                    ch: ' ',
                    color: None,
                }
            });
        }
    }
    while cells.last().is_some_and(|c| c.ch == ' ') {
        cells.pop();
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commits(spec: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        spec.iter()
            .map(|(h, p)| (h.to_string(), p.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn text(rows: &[Vec<Cell>]) -> Vec<String> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.ch).collect())
            .collect()
    }

    /// Four branches overlapping and merged one after another (a real
    /// history from a scratch repository, commits named by subject).
    #[test]
    fn four_overlapping_branches_take_one_row_per_commit() {
        let spec: &[(&str, &[&str])] = &[
            ("Merge feat-c", &["main 3", "c3"]),
            ("c3", &["c2"]),
            ("c2", &["c1"]),
            ("c1", &["main 2"]),
            ("main 3", &["Merge feat-a"]),
            ("Merge feat-a", &["Merge feat-d", "a3"]),
            ("a3", &["a2"]),
            ("a2", &["a1"]),
            ("a1", &["main 1"]),
            ("Merge feat-d", &["Merge feat-b", "d1"]),
            ("d1", &["main 2"]),
            ("Merge feat-b", &["main 2", "b2"]),
            ("b2", &["b1"]),
            ("b1", &["main 1"]),
            ("main 2", &["main 1"]),
            ("main 1", &["init"]),
            ("init", &[]),
        ];
        let rows = layout(&commits(spec));
        assert_eq!(
            text(&rows),
            [
                "●─╮",
                "│ ●",
                "│ ●",
                "│ ●",
                "● │",
                "●─┼─╮",
                "│ │ ●",
                "│ │ ●",
                "│ │ ●",
                "●─┼─┼─╮",
                "│ │ │ ●",
                "●─┼─┼─┼─╮",
                "│ │ │ │ ●",
                "│ │ │ │ ●",
                "●─┴─┼─╯ │",
                "●───┴───╯",
                "●",
            ]
        );
    }

    #[test]
    fn merging_the_same_branch_twice_joins_its_lane() {
        let spec: &[(&str, &[&str])] = &[
            ("m2", &["m1", "f2"]),
            ("f2", &["f1"]),
            ("m1", &["base", "f1"]),
            ("f1", &["base"]),
            ("base", &[]),
        ];
        let rows = layout(&commits(spec));
        assert_eq!(text(&rows), ["●─╮", "│ ●", "●─┤", "│ ●", "●─╯"]);
    }

    #[test]
    fn a_lane_keeps_its_color_and_joints_take_the_lane_color() {
        let spec: &[(&str, &[&str])] = &[("m", &["a", "b"]), ("b", &["a"]), ("a", &[])];
        let rows = layout(&commits(spec));
        let main = rows[0][0].color;
        let side = rows[0][2].color;
        assert_ne!(main, side);
        assert_eq!(rows[1][2].color, side, "the branch keeps its color");
        assert_eq!(rows[2][2].color, side, "its end joint too");
        assert_eq!(rows[2][0].color, main);
    }

    #[test]
    fn octopus_merges_open_one_lane_per_extra_parent() {
        let spec: &[(&str, &[&str])] = &[
            ("o", &["a", "b", "c"]),
            ("c", &["a"]),
            ("b", &["a"]),
            ("a", &[]),
        ];
        let rows = layout(&commits(spec));
        assert_eq!(text(&rows), ["●─┬─╮", "│ │ ●", "│ ● │", "●─┴─╯"]);
    }

    #[test]
    fn a_page_cut_leaves_lanes_open() {
        // The parents of the last commits are not on the page.
        let spec: &[(&str, &[&str])] = &[("m", &["x", "y"]), ("y", &["z"])];
        let rows = layout(&commits(spec));
        assert_eq!(text(&rows), ["●─╮", "│ ●"]);
    }
}
