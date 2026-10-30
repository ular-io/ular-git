//! Palette commands: the static table and the query match/rank.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    FocusRepos,
    FocusFiles,
    FocusDiff,
    AddRepository,
    RemoveRepository,
    NewGroup,
    RenameGroup,
    DeleteGroup,
    MoveToGroup,
    MoveGroupUp,
    MoveGroupDown,
    FoldAllGroups,
    UnfoldAllGroups,
    RemoveWorktree,
    Commit,
    DiscardFile,
    Push,
    Pull,
    OpenWith,
    FilterRepos,
    ToggleChangedOnly,
    TogglePendingOnly,
    ClearFilter,
    RefreshAll,
    ToggleWrap,
    TerminalCommand,
    ChangeTheme,
    EditConfig,
    OpenHelp,
    Quit,
}

pub struct CommandSpec {
    pub id: CommandId,
    /// Stable identifier stored in the history file.
    pub key: &'static str,
    pub label: &'static str,
    pub shortcut: Option<&'static str>,
    /// Lowercase search synonyms.
    pub aliases: &'static [&'static str],
    pub description: Option<&'static str>,
}

macro_rules! cmd {
    ($id:ident, $key:literal, $label:literal, $sc:expr, [$($a:literal),*], $desc:expr) => {
        CommandSpec {
            id: CommandId::$id,
            key: $key,
            label: $label,
            shortcut: $sc,
            aliases: &[$($a),*],
            description: $desc,
        }
    };
}

/// Registry in default display order.
pub const COMMANDS: &[CommandSpec] = &[
    cmd!(
        FocusRepos,
        "focus-repos",
        "Focus Repos Pane",
        Some("1"),
        ["go", "pane", "list"],
        None
    ),
    cmd!(
        FocusFiles,
        "focus-files",
        "Focus Files Pane",
        Some("2"),
        ["go", "pane", "changes"],
        None
    ),
    cmd!(
        FocusDiff,
        "focus-diff",
        "Focus Diff Pane",
        Some("3"),
        ["go", "pane", "patch"],
        None
    ),
    cmd!(
        AddRepository,
        "add-repository",
        "Add Repository",
        Some("+"),
        ["new", "create", "import", "scan", "folder"],
        Some("Add a repository, or every repository below a folder")
    ),
    cmd!(
        RemoveRepository,
        "remove-repository",
        "Remove Repository",
        Some("ctrl+d"),
        ["delete", "forget", "untrack"],
        Some("Drop the selected repository from the list (files stay on disk)")
    ),
    cmd!(
        NewGroup,
        "new-group",
        "New Group",
        Some("ctrl+n"),
        ["create", "add", "folder", "category"],
        None
    ),
    cmd!(
        RenameGroup,
        "rename-group",
        "Rename Group",
        Some("ctrl+e"),
        ["edit", "name", "title"],
        None
    ),
    cmd!(
        DeleteGroup,
        "delete-group",
        "Delete Group",
        Some("ctrl+d"),
        ["remove"],
        Some("Delete the selected group; its repositories become ungrouped")
    ),
    cmd!(
        MoveToGroup,
        "move-to-group",
        "Move to Group",
        Some("ctrl+e"),
        ["group", "assign", "edit", "change"],
        Some("File the selected repository under another group")
    ),
    cmd!(
        MoveGroupUp,
        "move-group-up",
        "Move Group Up",
        None,
        ["order", "reorder", "sort"],
        None
    ),
    cmd!(
        MoveGroupDown,
        "move-group-down",
        "Move Group Down",
        None,
        ["order", "reorder", "sort"],
        None
    ),
    cmd!(
        FoldAllGroups,
        "fold-all-groups",
        "Fold All Groups",
        None,
        ["collapse", "close", "hide"],
        None
    ),
    cmd!(
        UnfoldAllGroups,
        "unfold-all-groups",
        "Unfold All Groups",
        None,
        ["expand", "open", "show"],
        None
    ),
    cmd!(
        RemoveWorktree,
        "remove-worktree",
        "Remove Worktree",
        Some("ctrl+d"),
        ["delete", "worktree", "prune"],
        Some("git worktree remove: delete the selected worktree folder, keep its branch")
    ),
    cmd!(
        Commit,
        "commit",
        "Commit",
        Some("c"),
        ["save", "uncommitted", "changes"],
        Some("Commit the uncommitted changes of the selected repository")
    ),
    cmd!(
        DiscardFile,
        "discard-file",
        "Discard File Changes",
        None,
        ["revert", "restore", "reset", "undo", "throw", "checkout"],
        Some("Throw away the selected file's uncommitted changes (cannot be undone)")
    ),
    cmd!(
        Push,
        "push",
        "Push",
        Some("P"),
        ["upload", "publish", "remote", "upstream"],
        Some("Push the selected branch to its upstream (asks first)")
    ),
    cmd!(
        Pull,
        "pull",
        "Pull",
        Some("p"),
        ["update", "download", "fetch", "remote", "fast-forward"],
        Some("git pull --ff-only: take upstream commits when no merge is needed")
    ),
    cmd!(
        OpenWith,
        "open-with",
        "Open With…",
        Some("enter"),
        ["app", "launch", "editor", "ide", "finder", "vim", "external", "gui"],
        Some("Open the selected repository or file in an app listed in config.toml")
    ),
    cmd!(
        FilterRepos,
        "filter-repos",
        "Filter Repositories",
        Some("/"),
        ["search", "find"],
        None
    ),
    cmd!(
        ToggleChangedOnly,
        "toggle-changed-only",
        "Toggle Changed Only",
        Some("."),
        [
            "dirty",
            "modified",
            "changes",
            "uncommitted",
            "behind",
            "pull",
            "filter"
        ],
        Some("List only repositories and worktrees with uncommitted changes or commits to pull")
    ),
    cmd!(
        TogglePendingOnly,
        "toggle-pending-only",
        "Toggle Pending Only",
        Some("ctrl+."),
        ["unpushed", "ahead", "stash", "push", "sync", "dirty", "pending"],
        Some("List repositories with changes, commits to pull or push, or stashes")
    ),
    cmd!(
        ClearFilter,
        "clear-filter",
        "Clear Filter",
        Some("0"),
        ["reset", "all"],
        None
    ),
    cmd!(
        RefreshAll,
        "refresh-all",
        "Refresh All",
        Some("ctrl+u"),
        ["rescan", "reload", "update", "status"],
        Some("Rescan every repository")
    ),
    cmd!(
        ToggleWrap,
        "toggle-wrap",
        "Toggle Line Wrap",
        Some("."),
        ["wrap", "diff", "long"],
        None
    ),
    cmd!(
        TerminalCommand,
        "terminal-command",
        "Terminal Command",
        Some("!"),
        ["shell", "run", "exec"],
        Some("Run a shell command in the selected repository")
    ),
    cmd!(
        ChangeTheme,
        "change-theme",
        "Change Theme",
        None,
        ["color", "colour", "dark", "light", "palette"],
        None
    ),
    cmd!(
        EditConfig,
        "edit-config",
        "Edit Config",
        None,
        ["settings", "preferences", "options", "toml"],
        Some("Open ~/.config/ular-git/config.toml in your editor")
    ),
    cmd!(
        OpenHelp,
        "open-help",
        "Help",
        Some("?"),
        ["keys", "shortcuts", "keyboard"],
        None
    ),
    cmd!(Quit, "quit", "Quit", Some("q"), ["exit", "close"], None),
];

#[derive(Debug, Clone)]
pub struct QuickItem {
    /// Index in [`COMMANDS`].
    pub index: usize,
    pub enabled: bool,
}

impl QuickItem {
    pub fn spec(&self) -> &'static CommandSpec {
        &COMMANDS[self.index]
    }

    pub fn shortcut(&self) -> Option<&'static str> {
        self.spec().shortcut
    }

    pub fn label(&self) -> &'static str {
        self.spec().label
    }

    /// Footer text: the description, or why the command is unavailable.
    pub fn description(&self) -> &'static str {
        if self.enabled {
            self.spec().description.unwrap_or("")
        } else {
            "Not available for the current selection"
        }
    }
}

fn matches(spec: &CommandSpec, tokens: &[String]) -> bool {
    let label = spec.label.to_ascii_lowercase();
    tokens.iter().all(|t| {
        label.contains(t.as_str())
            || spec.aliases.iter().any(|a| a.contains(t.as_str()))
            || spec.shortcut.is_some_and(|s| s.contains(t.as_str()))
    })
}

/// Commands matching `query`, ranked enabled → recently used → registry order.
pub fn build_items<F: Fn(CommandId) -> bool>(
    query: &str,
    history: &[String],
    enabled: F,
) -> Vec<QuickItem> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let mut ranked: Vec<((bool, usize, usize), QuickItem)> = COMMANDS
        .iter()
        .enumerate()
        .filter(|(_, spec)| tokens.is_empty() || matches(spec, &tokens))
        .map(|(index, spec)| {
            let en = enabled(spec.id);
            let hist = history
                .iter()
                .position(|k| k == spec.key)
                .unwrap_or(usize::MAX);
            ((!en, hist, index), QuickItem { index, enabled: en })
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = COMMANDS.iter().map(|c| c.key).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }

    #[test]
    fn multi_word_and_alias_matching() {
        let items = build_items("rem work", &[], |_| true);
        assert_eq!(items[0].spec().id, CommandId::RemoveWorktree);
        let items = build_items("settings", &[], |_| true);
        assert_eq!(items[0].spec().id, CommandId::EditConfig);
    }

    #[test]
    fn disabled_sort_last_and_history_first() {
        let history = vec!["quit".to_string()];
        let items = build_items("", &history, |id| id != CommandId::FocusRepos);
        assert_eq!(items[0].spec().id, CommandId::Quit);
        assert_eq!(items.last().unwrap().spec().id, CommandId::FocusRepos);
    }
}
