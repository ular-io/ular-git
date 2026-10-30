# Repositories, Groups, and Scanning

> Status: Current
> Read when: Changing the repository pane, groups, filter, adding or removing
> repositories, worktree rows, scanning, or the status cache.
> Entry points: `src/ui/tree.rs`, `src/ui/scan.rs`, `src/store.rs`,
> `src/cache.rs`, `src/git/{status,worktree,discover}.rs`,
> `src/ui/overlays/{add_repo,group}.rs`

## Rows

- Repositories without a group come first, flat, unindented, with no header.
- Then each group header (`▾` / `▸ name`, muted text on the group band) with its
  repositories indented beneath it. Empty groups stay visible (not while
  filtering). Repositories sort by folder name inside each block.
- An open group header shows nothing on the right; its rows carry their own
  badges. A folded group shows `Ⓜ N`, the changed files of its repositories and
  their worktrees, and nothing when there are none. No repository count.
- Each repository's linked worktrees follow it, indented with `├` / `└`,
  showing the folder name and branch. The main worktree is the repository row
  itself.
- Rows are identified by path (`RowKey`), so the cursor stays on the same item
  when rows are rebuilt (rescan, filter, fold, store change).

### Badges

Right-aligned, in this order: `push…` / `pull…` while one runs, `!N`
conflicts, `Ⓜ N` other changed files, `↑N` ahead, `↓N` behind, `Ⓢ N` stashes,
`✓` clean (none of the above), `…` not scanned yet, `✗` the last scan failed
(the previous status stays shown). `↓N` reflects the last fetch; ular-git does
not fetch on its own.

## Filter

`/` opens the filter line under the header. Matching is a case-insensitive
substring of the folder name or branch name (a worktree's branch match keeps
its repository visible). Folded groups open while filtering; groups without a
match disappear. When the selected row is filtered out the cursor moves to the
first match. `enter` keeps the filter, `esc` restores the previous one, `0`
clears it.

## Pending only

`.` on the repository pane toggles a view of the working trees with something
left to do (`build_rows(.., pending_only)` in `src/ui/tree.rs`;
`App::toggle_pending_only`):

- uncommitted changes (any changed, untracked, or conflicting file),
- commits to pull (behind the upstream, as of the last fetch),
- commits to push (ahead of the upstream; a branch without an upstream does
  not count),
- a stash. The stash is shared by a repository and its worktrees, so it
  counts for the repository row only.

A clean repository stays as the parent of a worktree that matches, and only
matching worktrees are listed. The working tree being looked at stays listed
after it stops matching (a commit and push made elsewhere, picked up by a
rescan): `rebuild_rows` passes it to `build_rows` as `keep`. It leaves on the
next rescan after the cursor moves away, or at once when the view or the
text filter changes (`rebuild_rows_for_filter` keeps nothing). Groups open and groups without a match
disappear, as with the text filter; both apply together. The pane title reads
`Repos[pending: N/M]` (`Repos[api · pending: N/M]` with a text filter) and
the header shows a Filter row. `0` clears both. The view lasts for the session
only. (`ctrl+.` is not used for anything: most terminals send it as a plain
`.`.)

## Cursor

When the selected repository or worktree leaves the list (removed, or no
longer matching after a filter change), the cursor goes to the nearest
repository row, below first, never to a group header (`App::place_cursor`).

## Groups

- `ctrl+n` New Group: the name must be non-empty and unique.
- `ctrl+e` on a group: Rename Group. On a repository or worktree: Move to Group
  (Ungrouped, each group, then `[NEW GROUP]`, which creates a group and moves
  the repository into it).
- `ctrl+d` on a group: Delete Group after a confirmation. Its repositories
  become ungrouped and move to the top of the list.
- `→` / `l` do nothing on a group header (it has no files), and the Repos
  pane shows no `→` arrow there.
- `space` / `enter` on a group header folds or unfolds it (not while a filter
  is applied). Fold state persists in `repos.json`.
- `enter` on a repository or worktree row opens Open With (see
  [architecture.md](./architecture.md) §Runtime); `→` / `l` move to Files.
- Palette only: Move Group Up / Down, Fold / Unfold All Groups.

## Adding and removing repositories

- `+` Add Repository: Folder (text) and Group (dropdown; defaults to the group
  of the selected row). Prefill: the launch folder's repository when it is not
  listed yet, otherwise the parent of the selected repository.
- A folder resolves (`git::discover::repos_for`) as:
  1. a working tree root → its main repository (a linked worktree resolves to
     the repository that owns it);
  2. otherwise every repository found below it, up to three levels (hidden
     folders, `node_modules`, `target`, `build`, `dist`, `vendor`, `Pods` are
     skipped);
  3. otherwise the repository that contains it.
- Already listed repositories are skipped and counted in the footer result.
- `ctrl+d` / `delete` on a repository: Remove Repository after a confirmation.
  Only the list changes; nothing on disk is touched.
- CLI: `ular-git --add <dir>` applies the same resolution, saves, and exits.

## Scanning

- One scan of a working tree (`git::scan`) runs `git status --porcelain=v2
  --branch -z --untracked-files=all --ignore-submodules=dirty`, the unpushed
  log (`@{upstream}..HEAD`, up to 50) when ahead of an upstream, the stash
  count, and for a repository `git worktree list --porcelain -z`. Each
  reported worktree is then scanned too.
- At most 2000 file entries are kept per working tree; the rest are counted.
- Scans run on a pool of `scan_jobs` (default 8) workers. One scan per path is
  in flight; a request while it runs is replayed when it finishes. A result for
  a repository or worktree that is no longer listed is dropped.
- When everything is rescanned:
  - at launch (after the cache is shown);
  - every `refresh_interval_secs` (default 60, `0` = off), only after the
    previous round has finished;
  - on `ctrl+u`, after a `!` command, and when the terminal regains focus (at
    most once per 5 s).
- Resting the cursor on a working tree for 300 ms rescans it ahead of the
  queue when its status came from the cache or is older than 3 s.
- Selecting a repository never waits on git: the file list comes from the last
  scan.

## Status cache

- `<OS cache>/ular-git/status.json` holds the last status of every listed
  repository and its worktrees, with the scan time.
- Written when the scan queue drains and on exit.
- Versioned (`cache::CACHE_VERSION`); a cache of another version is discarded.
  Bump the version whenever the meaning of a serialized field changes.

## Verification

- Graph layout: `git/graph.rs` unit tests pin a four-branch history, a branch
  merged twice, octopus merges, lane colors, and page cuts.

- Unit tests: `ui/tree.rs` (row shapes), `store.rs`, `cache.rs`,
  `git/status.rs`, `git/worktree.rs`.
- Real git: `git/tests.rs` (scan, worktrees, discovery), `ui/tests.rs`
  (filter, groups, add / remove, worktree removal).
- Manual: [testing.md](./testing.md) §Manual checks.
