# Files, Diffs, and Git Operations

> Status: Current
> Read when: Changing the file pane, the diff pane, commit, push / pull, or
> worktree removal.
> Entry points: `src/ui/files.rs`, `src/ui/diff_view.rs`, `src/ui/sync.rs`,
> `src/git/{diff,ops,remote}.rs`, `src/ui/overlays/{commit,confirm}.rs`,
> `src/ui/render/{files,diff}.rs`

## Files pane

- Rows: the changed files of the selected working tree — conflicts first, then
  tracked changes, then untracked files, each by path — followed by unpushed
  commits as `↑ <first line of the message>`, the `↑` in the second
  status-code column (newest first).
- Status code: two letters, index then worktree (`M `, ` M`, `MM`, `A `,
  `D `, `R `, `??`, `UU`). Paths keep their tail when cut (`…/mod.rs`);
  renames show `old → new`.
- Title: `Files[N]`, plus `Commits[N]` when there are unpushed commits.
- Moving the cursor loads that item's diff immediately. The cursor stays on the
  same file (by path or commit hash) across rescans while it is listed.
- Empty states: `Select a repository` (group row), `Scanning…`, the scan
  error, `Working tree clean` (plus `N behind upstream`).

## Diff pane

| Item | Content |
| --- | --- |
| Tracked file | `Staged changes` section (`git diff --cached -M`) and / or `Unstaged changes` section (`git diff`) |
| Untracked file | The whole file as added (`git diff --no-index /dev/null <file>`) |
| Conflict | The combined diff |
| Unpushed commit | Title: the full hash. Body: one muted byline `<yyyy-mm-dd HH:MM>  <author> <<email>>` (the committer is not shown), the message right below it (first line bold), the stat, and the patch |

- Title for files: the path and `+N -N`.
- A gutter with old / new line numbers appears on code lines (added, removed,
  context) when the pane is at least 50 columns wide. Headers, meta lines,
  `@@` lines, and section titles start at the left edge.
- `.` toggles line wrapping (default: lines cut at the pane edge). Wrapped
  continuation lines keep a blank gutter.
- A commit's message (byline, subject, body) always wraps, whatever `.` says;
  its `--stat` block (from git's last `---` separator) follows `.` like code.
- At most `max_diff_lines` (default 20 000) lines are read; the child process
  is killed at the limit and a note ends the diff. Tabs expand to 4 columns;
  control characters are dropped.
- Loading is latest-wins: one load in flight, a newer request replaces the
  queued one, results of older requests are dropped. A rescan of the selected
  tree reloads the same diff and keeps the scroll position.

## Commit

`c` on a repository or worktree with uncommitted changes opens Commit:

- Include: `Staged only` (available only when something is staged; the default
  then) or `All changes (adds untracked files)` (`git add -A` first; the
  default when nothing is staged).
- Message: one line, required.
- Refused up front: nothing to commit, unresolved conflicts.
- git hooks run as usual; their failure message is shown in an error dialog.
- Afterwards the working tree is rescanned; the footer shows
  `<name>: committed <short hash> <subject>`.

## Push and pull

Both act on the selected repository or worktree and run in the background.
The row shows `push…` / `pull…` until they finish; one operation per working
tree runs at a time. The tree is rescanned afterwards.

- `P` Push opens a confirmation with Push focused:
  - With an upstream: `Push N commits on <branch> to <upstream>?` and up to 5
    commit subjects (`… and N more`). When the upstream has commits that are
    not local, the dialog says git will reject the push and to pull first.
    Nothing ahead → footer `Nothing to push`, no dialog.
  - Without an upstream: `Push it to <remote>/<branch> and set that as its
    upstream?` The remote is `remote.pushDefault`, else `origin`, else the only
    remote. With none of those the push is refused with a footer note.
  - Never forced.
- `p` Pull runs `git pull --ff-only --no-rebase` at once. A diverged branch or
  a local change that would be overwritten is refused by git and nothing
  changes. No upstream → footer note.
- Detached HEAD: both refused with a footer note.
- No credential prompt is ever shown: HTTPS relies on the credential helper
  (`GIT_TERMINAL_PROMPT=0`); SSH on keys already in the agent
  (`BatchMode=yes, ConnectTimeout=15`, unless `GIT_SSH_COMMAND` or
  `core.sshCommand` is set).
- Limits: 120 s per operation; HTTP transfers slower than 1 KB/s for 30 s
  abort.
- Success: footer `<name>: pushed <ref> → <ref> <old>..<new>` or
  `<name>: pulled fast-forward, <stat>` / `already up to date`. Failure: an
  error dialog with git's message.

## Worktree removal

`ctrl+d` / `delete` on a worktree row opens Remove Worktree with Cancel
focused:

- Clean worktree: `Remove` → `git worktree remove`.
- Uncommitted changes: the dialog says how many are lost; the button reads
  `Force Remove` → `git worktree remove --force --force` (also overrides a
  lock).
- A worktree whose folder is already gone is pruned (`git worktree prune`).
- The branch is never deleted. The main worktree cannot be removed this way.

## Not offered

Fetch on its own, force push, merge or rebase pulls, checkout, stage /
unstage, discard, and branch operations.

## Verification

- `git/tests.rs` covers every operation against temporary repositories and a
  local bare remote; `ui/tests.rs` covers the dialogs and keys end to end.
- Manual push / pull checks use a scratch repository with a bare remote only
  ([testing.md](./testing.md)).
