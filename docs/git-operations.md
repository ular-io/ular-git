# Files, Diffs, and Git Operations

> Status: Current
> Read when: Changing the file pane, the diff pane, commit, push / pull /
> fetch, or worktree removal.
> Entry points: `src/ui/files.rs`, `src/ui/commit_files.rs`,
> `src/ui/diff_view.rs`, `src/ui/sync.rs`, `src/ui/fetch.rs`,
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
     tree with changes, and the history reloads when that flips),
     `Uncommitted Changes` (`WORKTREE_TITLE`), and right-aligned badges as in
     the repository pane: `!N` conflicts, `Ⓜ N` other changes (untracked
     included), `↓N` commits to pull (zero badges left out). A clean tree has neither
     row and starts with the history. Selecting it shows
     `DiffSource::WorkTree` (`src/git/stat.rs`): every uncommitted change
     against HEAD, staged and unstaged together (`git diff HEAD --numstat
     -z -M`; the index alone on an unborn branch), and every untracked file
     with its line count, as a stat block under a rule carrying git's
     summary, then each file's diff under its title like a shown commit
     (`worktree_doc` in `src/git/diff.rs`: `git diff -M HEAD`, then
     `git diff --no-index /dev/null <file>` per untracked file, until
     `max_diff_lines`; the counts stay the stat's when the diffs are cut);
     the pane title reads `Uncommitted Changes +N -N`;
  4. the commit history from HEAD with its branch graph, one row per
     commit (no connector rows). Subjects use the file rows' text style; the
     graph lanes are faded. Between the graph and the subject, `muted`,
     rows show the author date (`%ad` with `--date=format:%Y-%m-%d %H:%M`;
     rows show `YYYY-MM-DD`) and / or the author name (`%an`, padded to the
     page's longest name, at most 16 cells with `…`). `.` on the Files pane
     (palette Cycle History Date / Author; `CommitMeta` in
     `src/ui/history_view.rs`, drawn by `meta_spans` in
     `src/ui/render/files.rs`) cycles date → date and author → author →
     subject only, saved in `view.json` (the last is stored as `none`). The
     choice holds at any width; the subject is cut instead. With subject
     only, the selected commit takes a second line (`detail_line` in
     `src/ui/render/history_row.rs`): the lanes running on below it
     (`graph::Row::below`, plain `│` per lane) and `YYYY-MM-DD HH:MM  author`
     (full name, cut only at the pane edge) in the selection text color
     faded 35 % toward the selection background, not bold. Rows below the
     selection keep their place as the cursor moves; the scrollbar counts
     the extra line.
     Lanes stay in place and keep their color;
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
  in `src/ui/render/history_row.rs`): `>` (Files pane, palette Cycle Branch / Tag
  Labels) cycles none → branches → branches and tags → tags → none. Labels
  are solid badges without icons after the subject (which keeps at least 12
  columns and is cut first), joined into one strip (a `│` in the background color splits two
  badges of the same color; different colors touch), running through the row's right
  padding cell to the border. Color tells
  the kind: local branch the branch color (`HEAD` when
  detached), remote branch the hunk color, tag the warning color; each is
  moved a quarter toward the text color and then faded a third toward the
  background, with the background color as text, in the same colors whether
  or not the pane has focus. A remote branch with
  a local twin on the same commit (`origin/main` next to `main`) is left out. A tag copied
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
  a group header is selected. The title stays the same inside a commit's or
  the uncommitted changes' files (`files::pane_title`).
- Commit count on the bottom border, right-aligned, soft-dim:
  `2 / 123 commits` on a history commit (its place in the list, 1 at the
  top), `123 commits` on any other row (a changed file, Uncommitted Changes,
  `… more`), `2 / 123` when the pane is narrow, nothing when even that does
  not fit (`commits_label` in `src/ui/render/files.rs`). The total covers the
  same commits as the list read in full, not only the loaded page
  (`history::count`: `git rev-list --count HEAD @{upstream}`, HEAD alone
  without an upstream). It is counted on its own thread whenever the
  history reloads (`request_count` in `src/ui/history_view.rs`), so a long
  history never holds up the list; it is absent until the first count
  arrives. Not shown in a commit's file view.
- Moving the cursor loads that item's diff immediately (a commit opens as
  `git show`). The cursor stays on the same file or commit across rescans and
  history reloads while it is listed.
- The cursor starts on the first item: the first changed file, or the
  newest commit of a clean tree.
- Empty states: `Select a repository` (group row), `Scanning…`, the scan
  error, `Loading history…` (below the working-tree row). Messages are
  centered. `Select a repository` sits on a `group_bg` fill reaching two rows
  above and below it and eight cells to each side.
- `enter` on a commit moves to the diff with the whole commit; `→` / `l`
  open the commit view instead.

### Commit view

`→` / `l` on a history commit replace the pane's rows with that commit's
files (`CommitView` in `src/ui/commit_files.rs`, drawn by
`src/ui/render/commit_files.rs`), read in the background by
`git::commit_files::load` (`git diff-tree -r -M --root
--diff-merges=first-parent --raw --numstat -z`: a merge lists what it
brought in against its first parent, a root commit its added files).

- Title: the repository's (or worktree's) folder name, as in the changes
  and history. Rows: the commit (`● subject`), a
  `Files` divider joined to the pane border, then one row per file: git's
  status letter (`A` success, `D` error, `R` / `C` branch, others warning
  color), the path (`old → new` for a rename, folder soft-dim, tail kept when
  cut), and `+N -N` right-aligned in the diff colors (`Bin` for binary).
  `Loading files…`, the error, or `No files changed` sit below the divider.
- The cursor starts on the commit row, which shows the whole commit as
  before (for a merge git's combined diff, not the first-parent one). A
  file shows its diff alone (`DiffSource::CommitFile`: `git show --format=
  -M --diff-merges=first-parent <hash> -- [old] <path>`); the diff title is
  the file name and its counts, like a changed file's.
- `→` / `l` move to Diff; `←` there comes back to the commit view. `enter`
  on a file opens Open With with the commit's tabs (Diff, Parent, the
  commit, Working Tree; see architecture.md §Runtime); on the commit row it
  moves to Diff.
  `←` / `h` in the view return to the changes and history with the cursor on
  the commit (or where it was if a history reload dropped it).
- Rescans and history reloads leave the view and its diff alone; selecting
  another working tree closes it.
- `App::selected_file` is `None` while a commit's view is open, so
  discarding a file and palette commands never act on a commit's file (it
  is a past version; the working tree's file of that path may differ).
  Open With reads the versions from git instead (`App::file_tabs`).

### Uncommitted changes view

`→` / `l` on the Uncommitted Changes row open the same view
(`ViewOf::WorkTree`) over the working tree, read by
`git::commit_files::load_worktree`: `git diff -M --raw --numstat -z HEAD`
(`--cached` on an unborn branch), then every untracked file as `?` with its
line count.

- Title: the folder name, as in a commit's view. Row 0:
  `○ Uncommitted Changes`, showing the summary stat
  (`DiffSource::WorkTree`). Status letters as in a commit's view, `?` in the
  untracked color, `U` in the error color. As in a commit's view, the
  cursor starts on row 0.
- A file shows its change against HEAD, staged and unstaged together
  (`DiffSource::WorkTreeFile`: `git diff -M HEAD -- [old] <path>`, an
  untracked file as added), unlike the changed-file rows' sections.
- Every rescan of the tree re-reads the list (`App::rescan_commit_files`
  from `refresh_files`); the cursor stays on its file while it is listed.
  The view closes once the tree is clean.
- `App::selected_file` is the working tree's entry for the file under the
  cursor, so discarding and palette commands act on it as on a changed-file
  row; `enter` opens Open With with the uncommitted tabs (Diff, HEAD,
  Working Tree). `enter` on row 0 moves to Diff.
- `←` / `h` return to the Uncommitted Changes row.

## Diff pane

| Item | Content |
| --- | --- |
| Tracked file | `Staged changes` section (`git diff --cached -M`) and / or `Unstaged changes` section (`git diff`) |
| Untracked file | The whole file as added (`git diff --no-index /dev/null <file>`) |
| Conflict | The combined diff |
| Unpushed commit | Title: the full hash. Body: one muted byline `<yyyy-mm-dd HH:MM>  <author> <<email>>` (the committer is not shown), the message right below it without its blank lines (first line bold), a rule, the stat, and the patch |

- Title for a single file (a changed file, one file of a commit or of the
  uncommitted changes): the file name and its line counts once loaded
  (`a.rs +20 -3`, a zero side left out, none when no line changed, as for a
  pure rename or a binary file; `file_title` in `src/ui/render/diff.rs`).
  The body starts with the diff itself (the first section title, or the
  first code line); the full path and a rename's `old → new` are on the
  file's row in the file pane. Commit titles keep the hash and `+N -N`.
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
  start at the left edge. Each number column is as wide as the largest number
  it shows in the whole diff (fixed while scrolling, so code never shifts),
  and a column with no numbers at all is left out: a new file shows only new
  numbers, a deleted file only old ones (`Gutter` in
  `src/ui/render/gutter.rs`; the side-by-side layout uses the same widths).
- Section titles (`Staged changes`, `Unstaged changes`, `Untracked file`,
  `Merge conflict`) are centered on a border-to-border line.
- Hunk headers (`@@ -14,6 +14,7 @@ context`) are not shown: the line numbers
  say where a hunk starts, and git's context guess is often wrong outside code.
  Between two hunks of the same file a `dim` cut line (`- - -`) marks the
  skipped lines: it starts where the code starts (the number columns left
  blank; the left side's when side by side) and runs to the right edge.
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
- `.` on the Diff pane toggles line wrapping (default: lines cut at the
  pane edge). Wrapped
  continuation lines keep a blank gutter.
- The line on top stays on top when the layout changes: `.`, `s`, or a new
  diff pane width (`1` / `2` / `3`, focus moving across panes, a terminal
  resize). The renderer records the top line and its layout each
  frame (`Anchor` in `src/ui/diff_view.rs`, `top_row` in
  `src/ui/render/diff.rs`) and starts the next frame at that line's first
  row when the layout differs. A scroll since that frame wins; near the end
  the position stops at the last full page.
- Side by side (`src/ui/render/split.rs`, drawn in `src/ui/render/diff.rs`):
  when the diff pane is at least 120 columns wide inside and line numbers
  show, old lines go left (old numbers) and new lines right (new numbers),
  split by a muted `│`. A context line shows on both sides; a run of removed
  lines pairs row by row with the run of added lines right after it, the
  shorter side padded. Every other line (section and file
  titles, gaps, commit byline, message, stat) keeps the full width. With `.`
  each side wraps on its own and the row takes the taller side. `s` (Diff
  pane, palette Toggle Side-by-Side Diff) switches between side by side and
  always unified, keeps the top line, and is saved in `view.json`
  (`split_diff`, default on).
- A commit's message (byline, subject, body) always wraps, whatever `.` says;
  its `--stat` block (from git's last `---` separator) follows `.` like code.
- At most `max_diff_lines` (default 20 000) lines are read; the child process
  is killed at the limit and a note ends the diff. Tabs expand to 4 columns;
  control characters are dropped.
- A file larger than `max_diff_file_mb` (default 10 MiB; 0 keeps git's 512
  MiB) counts as binary and its contents are not read: git's `Binary files
  … differ` line, `Bin` in a stat, no counts in the file lists. Every read
  that computes a diff passes it (`git::diff_command`,
  `-c core.bigFileThreshold=`): the diff pane, the uncommitted stat, and the
  commit and Uncommitted Changes file lists. Without it git diffs a huge
  text file in full even though only `max_diff_lines` are shown, because
  the stat comes first.
- Loading is latest-wins: one load in flight; a newer request cancels it
  (`git::cancel::Cancel` kills its git process, so moving through the history
  never waits for a slow commit it passed) and replaces the queued one;
  results of older requests are dropped. A rescan of the selected
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
tree runs at a time, and neither starts while the repository has a fetch
queued or running (footer `A fetch is still running here`). The tree is
rescanned afterwards.

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

## Fetch

Plain `git fetch` (`git::remote::fetch`): the remote of HEAD's upstream, else
`origin`, with the user's own fetch settings (`fetch.prune`, refspecs). A
repository without a remote is skipped without running `git fetch`. Same
network rules as push / pull: no credential prompt, 120 s limit, the
slow-HTTP abort.
Scheduling lives in `src/ui/fetch.rs` (`App::start_fetch`,
`App::refresh_and_fetch`).

- `ctrl+f` (every pane; palette Fetch) fetches the repository of the selected
  row. A worktree row fetches its repository: the remote-tracking refs are
  shared, so the repository and all its worktrees update. On a group header it
  fetches every repository of the group (the filter does not narrow it).
- `ctrl+u` (palette Refresh All) reloads the list, starts the rescan, then
  fetches every listed repository. The timed rescan, the focus rescan, and the
  rescan after a `!` command never fetch.
- At most 8 fetches run at once (`FETCH_JOBS`), each on its own thread, so
  scans never wait behind the network. The rest wait in a queue.
- A queued or running fetch shows `fetch…` on the repository row and on a
  folded group header. A repository with a push or pull running in it or one
  of its worktrees is not fetched (`ctrl+f`: footer `A push is still running
  here`; `ctrl+u` / a group: skipped).
- Each fetched repository is rescanned (its worktrees follow), which updates
  `↓N`.
- One repository (`ctrl+f` on a repository or worktree row): footer
  `<name>: fetched, N refs updated` / `fetched, nothing new` /
  `no remote to fetch from`; failure: an error dialog with git's message.
- Several repositories (`ctrl+u`, a group) form one round, reported when its
  last fetch finishes: `Fetched N repositories`, or
  `Fetch failed in K of N repositories: a, b, c and M more` (no dialog;
  `ctrl+f` on a named repository shows its error). Requests made while a round
  runs join it.

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

Force push, merge or rebase pulls, fetch with options (`--all`, `--prune`),
checkout, stage / unstage, discarding everything at once, and branch
operations.

## Verification

- `git/tests.rs` covers every operation against temporary repositories and a
  local bare remote; `ui/tests.rs`, `ui/sync_tests.rs`, and
  `ui/fetch_tests.rs` cover the dialogs and keys end to end.
- Manual push / pull / fetch checks use a scratch repository with a bare
  remote only
  ([testing.md](./testing.md)).
