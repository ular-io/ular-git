//! Key hints shown in the header and in help, from one source so the two
//! never disagree. Labels follow the selected row (`ctrl+d` reads "Remove
//! Worktree" on a worktree and "Delete Group" on a group).

use super::tree::Row;
use super::{App, Pane};

pub type Hint = (&'static str, String);

fn h(key: &'static str, label: &str) -> Hint {
    (key, label.to_string())
}

/// Keys that work in every pane (third header column).
pub fn common() -> Vec<Hint> {
    vec![
        h(":", "Quick Command"),
        h("!", "Terminal Command"),
        h("ctrl+u", "Refresh All"),
        h("?", "Help"),
        h("q", "Quit"),
    ]
}

impl App {
    /// The focused pane's two header columns (at most five rows each).
    pub fn pane_hints(&self) -> [Vec<Hint>; 2] {
        match self.focus {
            Pane::Repos => {
                let (enter, edit, remove) = match self.current_row() {
                    Some(Row::Group {
                        collapsed, name, ..
                    }) => (
                        if *collapsed {
                            "Unfold Group"
                        } else {
                            "Fold Group"
                        },
                        if name.is_some() { "Rename Group" } else { "" },
                        if name.is_some() { "Delete Group" } else { "" },
                    ),
                    Some(Row::Worktree { .. }) => {
                        ("Open With…", "Move to Group", "Remove Worktree")
                    }
                    _ => ("Open With…", "Move to Group", "Remove Repository"),
                };
                let mut first = vec![
                    h("/", "Filter"),
                    h("+", "Add Repository"),
                    h("ctrl+n", "New Group"),
                ];
                if !edit.is_empty() {
                    first.push(h("ctrl+e", edit));
                }
                if !remove.is_empty() {
                    first.push(h("ctrl+d", remove));
                }
                // `1..3` and `0` stay out (footer and help show them).
                // `.` cycles the views; the label names the next one.
                let changed = match self.change_view {
                    super::tree::ChangeView::All => "Changed Only",
                    super::tree::ChangeView::Changed => "Pending Only",
                    super::tree::ChangeView::Pending => "Show All",
                };
                let second = vec![
                    h("enter", enter),
                    h("c", "Commit"),
                    h("P", "Push"),
                    h("p", "Pull"),
                    h(".", changed),
                ];
                [first, second]
            }
            Pane::Files => [
                vec![
                    h(
                        "enter",
                        if self.selected_file().is_some() {
                            "Open With…"
                        } else {
                            "Open Diff"
                        },
                    ),
                    h("c", "Commit"),
                    h(".", "Wrap Lines"),
                ],
                vec![
                    h("P", "Push"),
                    h("p", "Pull"),
                    h("/", "Filter Repos"),
                    h("+", "Add Repository"),
                ],
            ],
            Pane::Diff => [
                vec![h(".", "Wrap Lines"), h("c", "Commit")],
                vec![
                    h("P", "Push"),
                    h("p", "Pull"),
                    h("/", "Filter Repos"),
                    h("+", "Add Repository"),
                ],
            ],
        }
    }

    /// Movement keys of the focused pane, left out of the header for space.
    pub fn pane_help_only(&self) -> Vec<Hint> {
        match self.focus {
            Pane::Repos => vec![
                h("↑↓ / j k", "Move"),
                h("pgup / pgdn", "Move a page"),
                h("home / g", "First row"),
                h("end / G", "Last row"),
                h("space", "Fold / unfold group"),
                h(
                    ".",
                    "Changed (to commit or pull) → Pending (+ push, stash) → All",
                ),
                h("→ / l", "Files pane"),
                h("delete", "Same as ctrl+d"),
            ],
            Pane::Files => vec![
                h("↑↓ / j k", "Select file (diff follows)"),
                h("home / g", "First file"),
                h("end / G", "Last file"),
                h("pgup / pgdn", "Scroll the diff"),
                h("← / h", "Repos pane"),
                h("→ / l", "Diff pane"),
            ],
            Pane::Diff => vec![
                h("↑↓ / j k", "Scroll a line"),
                h("pgup / pgdn", "Scroll a page"),
                h("home / g", "Top"),
                h("end / G", "Bottom"),
                h("← / h", "Files pane"),
            ],
        }
    }
}

/// Keys used inside dialogs (help only — dialogs carry no key hints).
pub fn dialog() -> Vec<Hint> {
    vec![
        h("tab / shift+tab", "Next / previous control"),
        h("↑↓", "Move between rows"),
        h("← →", "Pick a button or option"),
        h("enter", "Next row, or press the button"),
        h("space", "Open a dropdown, toggle an option"),
        h("esc", "Close the dropdown, then the dialog"),
    ]
}

/// Keys available everywhere on the screen.
pub fn anywhere() -> Vec<Hint> {
    vec![
        h("1 / 2 / 3", "Focus Repos / Files / Diff"),
        h("/", "Filter repositories"),
        h("0", "Clear the filter and the changed-only view"),
        h("+", "Add repository"),
        h("c", "Commit the selected repository"),
        h("P", "Push (asks first)"),
        h("p", "Pull, fast-forward only"),
        h(":", "Quick command (every command)"),
        h("!", "Run a shell command in the repository"),
        h("ctrl+u", "Rescan every repository"),
        h("?", "Open / close help"),
        h("q / ctrl+c", "Quit (press twice)"),
    ]
}
