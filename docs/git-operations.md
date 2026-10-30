# Files, Diffs, and Git Operations

> Status: Current
> Read when: Changing the file pane, the diff pane, commit, push / pull, or
> worktree removal.
> Entry points: `src/ui/files.rs`, `src/ui/diff_view.rs`, `src/ui/sync.rs`,
> `src/git/{diff,ops,remote}.rs`, `src/ui/overlays/{commit,confirm}.rs`,
> `src/ui/render/{files,diff}.rs`

## Files pane

- Rows, top to bottom:
  1. the changed files of the selected working tree — conflicts first, then
     tracked changes, then untracked files, each by path;
  2. only while the tree has changes, a muted `History` divider joined to
     the pane border, then
  3. the working-tree row (`FileItem::WorkTree`): a `○` node on the lane
     leading to HEAD (`History::worktree`, laid out as a hash-less commit
     whose parent is HEAD; `history::load(.., worktree)` adds it only for a
     tree with changes, and the history reloads when that flips) and
     `N changes · M modified · U untracked · C conflicted` (zero parts left
     out), plus `· N behind` with commits to pull. A clean tree has neither
     row and starts with the history. Selecting it shows
     `DiffSource::WorkTree` (`src/git/stat.rs`): every uncommitted change
     against HEAD, staged and unstaged together (`git diff HEAD --numstat
     -z -M`; the index alone on an unborn branch), and every untracked file
     with its line count, as a stat block under a rule carrying git's
     summary; the pane title reads `Working Tree +N -N`;
  4. the commit history from HEAD with its branch graph, faded, one row per
     commit (no connector rows). Lanes stay in place and keep their color;
     joints sit on the commit row: `─╮` a merge opens a lane, `─┤` a merge
     joins a lane already open (the same branch merged again, or a lane that
     ends on this row and is reused at once by the merge's new lane), `─╯` a lane
     ends (its branch started here), `┬ ┴` the same with the line going on,
     `┼` a line crossing a lane, `╭ ╰ ├` for lanes to the left. Not faded,
     in the branch color: `↑` commits not on the upstream yet (push sends
     them) and `↓` upstream commits HEAD lacks (pull brings them in; as of the
     last fetch). With an upstream ahead the history starts from both HEAD
     and the upstream, so a diverged branch shows both sides meeting at their
     base. The graph uses at most 12 columns;
  5. `… more` when older commits exist.
- Ref labels (`RefLabels` in `src/ui/history_view.rs`, drawn by `ref_spans`
  in `src/ui/render/files.rs`): `>` (any pane, palette Cycle Branch / Tag
  Labels) cycles none → branches → branches and tags → tags → none. Labels
  are solid badges without icons after the subject (which keeps at least 12
  columns and is cut first), joined into one strip with a `│` in the
  background color between two badges, running through the row's right
  padding cell to the border. Color tells
  the kind: local branch the branch color (`HEAD` when
  detached), remote branch the hunk color, tag the warning color; each is
  moved a quarter toward the text color, with the background color as text,
  and a dimmed pane fades them a third. A remote branch with a local twin on
  the same commit (`origin/main` next to `main`) is left out. A tag copied
  under `refs/remotes/<remote>/tags/` (a fetch refspec
  `+refs/tags/*:refs/remotes/origin/tags/*`) counts as that tag: left out next
  to the local tag, shown as the tag without it. Names are cut at 24 columns;
  labels that do not fit are dropped. They keep their colors on the selection
  bar. Read from `git log --decorate=full` `%D`. The last choice is saved in
  `~/.config/ular-git/view.json`.
- History is read for the selected working tree only, in the background, when
  it is selected or its HEAD or ahead / behind counts change (commit, pull,
  checkout, a fetch elsewhere): the newest 100
  commits first. Selecting `… more` re-reads twice as many (the graph needs one
  pass from HEAD) and keeps the cursor on the first newly read commit.
- Status code: two letters, index then worktree (`M `, ` M`, `MM`, `A `,
  `D `, `R `, `??`, `UU`). Paths keep their tail when cut (`…/mod.rs`);
  renames show `old → new`.
- Title: the selected repository's (or worktree's) folder name; `Files` while
  a group header is selected.
- Moving the cursor loads that item's diff immediately (a commit opens as
  `git show`). The cursor stays on the same file or commit across rescans and
  history reloads while it is listed.
- The cursor starts on the first item: the first changed file, or the
  newest commit of a clean tree.
- Empty states: `Select a repository` (group row), `Scanning…`, the scan
  error, `Loading history…` (below the working-tree row).

## Diff pane

| Item | Content |
| --- | --- |
| Tracked file | `Staged changes` section (`git diff --cached -M`) and / or `Unstaged changes` section (`git diff`) |
| Untracked file | The whole file as added (`git diff --no-index /dev/null <file>`) |
| Conflict | The combined diff |
| Unpushed commit | Title: the full hash. Body: one muted byline `<yyyy-mm-dd HH:MM>  <author> <<email>>` (the committer is not shown), the message right below it without its blank lines (first line bold), a rule, the stat, and the patch |

- Title for files: the file name only. The body starts with the full path in
  the repository (`old → new` for a rename), muted and always wrapped after a
  `/`, then the line counts (`+20 -3`, each in its diff color, a zero side
  left out; no line when nothing changed, as for a pure rename or a binary
  file). No rule separates them from the first section title. Commit titles
  keep the hash and `+N -N`.
- git's per-file header is condensed. `diff --git`, `index`, `---`, and `+++`
  lines are dropped. In a commit, each file starts with a full-width title
  line with the path centered (`├──── <path> ────┤`, `old → new` for a
  rename); a single-file diff already names the file in the pane title.
  git's `---` between a commit's message and its stat is drawn as the same
  border-to-border rule, carrying the stat summary (`N files changed, …`)
  centered; the summary line is removed from the stat. The blank line git
  prints between the stat and the first file is dropped (the spacing rule
  below adds its own). Lines that carry information stay,
  muted: new / deleted file mode, mode change, similarity, binary.
- A gutter with old / new line numbers appears on code lines (added, removed,
  context) when the pane is at least 50 columns wide. Headers and meta lines
  start at the left edge.
- Section titles (`Staged changes`, `Unstaged changes`, `Untracked file`,
  `Merge conflict`) are centered on a border-to-border line.
- Hunk headers (`@@ -14,6 +14,7 @@ context`) are not shown: the line numbers
  say where a hunk starts, and git's context guess is often wrong outside code.
  Between two hunks of the same file a muted `⋯` line marks the skipped lines.
- Every rule and title (section, file, the stat rule) has one blank line
  above and below it (`space_dividers` in `src/git/diff.rs`, for file,
  commit, and working-tree diffs): none above the first line, none doubled,
  none at the end.
- Changes within a line (`src/git/inline.rs`, computed when the diff loads):
  a run of removed lines pairs, in order, with the run of added lines right
  after it, and each pair is compared word by word (words, runs of spaces,
  single other characters; longest common subsequence). The differing words
  become `DiffLine::marks` on both lines and are drawn reversed in the line's
  color, in the unified and the side-by-side layouts, across wrapped pieces.
  Pairs less than half alike, equal lines, lines over 1000 bytes, and lines
  over 300 words are left unmarked.
- `.` toggles line wrapping (default: lines cut at the pane edge). Wrapped
  continuation lines keep a blank gutter.
- Side by side (`src/ui/render/split.rs`, drawn in `src/ui/render/diff.rs`):
  when the diff pane is at least 120 columns wide inside and line numbers
  show, old lines go left (old numbers) and new lines right (new numbers),
  split by a muted `│`. A context line shows on both sides; a run of removed
  lines pairs row by row with the run of added lines right after it, the
  shorter side padded. Every other line (path, counts, section and file
  titles, gaps, commit byline, message, stat) keeps the full width. With `.`
  each side wraps on its own and the row takes the taller side. `s` (any
  pane, palette Toggle Side-by-Side Diff) switches between side by side and
  always unified, scrolls to the top, and is saved in `view.json`
  (`split_diff`, default on).
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

## Discard file changes

Palette only (`:` → **Discard File Changes**, no shortcut), on the changed file
under the file cursor. A confirmation (Cancel focused, error color) names the
file and what is lost (`+N -N lines` when its diff is loaded). It cannot be
undone; nothing is stashed.

| File | Effect |
| --- | --- |
| Modified, staged and / or unstaged, deleted | `git restore --source=HEAD --staged --worktree` — back to the last commit |
| Renamed | Both paths restored: the old name comes back, the new one goes |
| Newly added | Removed from the index and the folder |
| Untracked | Deleted (`git clean -f`) |
| Conflict | Refused (resolve it first) |

The command is unavailable on a history commit, `… more`, or a conflict. The
working tree is rescanned afterwards.

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
unstage, discarding everything at once, and branch operations.

## Verification

- `git/tests.rs` covers every operation against temporary repositories and a
  local bare remote; `ui/tests.rs` covers the dialogs and keys end to end.
- Manual push / pull checks use a scratch repository with a bare remote only
  ([testing.md](./testing.md)).
