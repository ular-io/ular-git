# Repositories, Groups, and Scanning

> Status: Current
> Read when: Changing the repository pane, groups, filter, adding or removing
> repositories, worktree rows, scanning, or the status cache.
> Entry points: `src/ui/tree.rs`, `src/ui/scan.rs`, `src/store.rs`,
> `src/cache.rs`, `src/git/{status,worktree,discover}.rs`,
> `src/ui/overlays/{add_repo,group}.rs`, `src/ui/store_sync.rs`,
> `src/store/shared.rs`

## Rows

- Repositories without a group come first, flat, unindented, with no header.
- Then each group header (`▾` / `▸ name`, muted text on the group band) with its
  repositories indented beneath it. Empty groups stay visible (not while
  filtering). Inside each block repositories keep their order in
  `repos.json` (see Order below).
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
`…` not scanned yet, `✗` the last scan failed (the previous status stays
shown). A clean tree (none of the above) shows no badge. `↓N` reflects the
last fetch; ular-git does not fetch on its own.

## Filter

`/` (Repos pane) opens the filter line under the header. Matching is a case-insensitive
substring of the folder name or branch name (a worktree's branch match keeps
its repository visible). Folded groups open while filtering; groups without a
match disappear. When the selected row is filtered out the cursor moves to the
first match. `enter` keeps the filter, `esc` restores the previous one, `0`
(Repos pane) clears it. While a filter is applied the header shows `0` in
place of `/`.

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
  become ungrouped, at the end of the ungrouped block (the top of the list).
- On a group header (it has no files) `←` / `h` folds it, `→` / `l` unfolds
  it (each does nothing when the group already is), and `enter` toggles; the
  Repos pane shows no `→` arrow there. `space` does nothing. Not while a
  filter is applied. Fold state persists in `repos.json`.
- `enter` on a repository or worktree row opens Open With (see
  [architecture.md](./architecture.md) §Runtime); `→` / `l` move to Files.
- Palette only: Fold / Unfold All Groups.

## Order

- Groups show in `repos.json` `groups` order; repositories in `repos` order,
  inside their group (`build_rows` keeps the store order).
- `alt+↑` / `alt+↓` (Option on macOS; help only, the header has no room):
  on a group header moves the group one place among the groups
  (`RepoStore::move_group`); on a repository row moves it one place among
  the repositories of its group (`RepoStore::move_repo`). It stops at the
  first and last place and never crosses into another group (`ctrl+e` Move
  to Group does that). Nothing on a worktree row (git orders worktrees). The
  ungrouped block always comes first. Entry point: `App::move_row`.
- While a text filter or Pending Only is applied it does nothing and the
  footer says the order stays fixed: a hidden row could be the one passed.
- Palette: Move Group Up / Down, Move Repository Up / Down (same action).
  A terminal that does not send Option+arrow as a modified arrow never
  delivers the key; the palette commands still work.
- A new repository goes to the end of its group; a folder that yields
  several adds them by folder name (`discover::repos_for` order). Move to
  Group puts the repository at the end of the target group (nothing changes
  when it is already there).
- `repos.json` version 2 holds this order. A version 1 (or older) file was
  shown by folder name, so loading one sorts `repos` by folder name once; it
  is written as version 2 on the next change.

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

## Shared list and simultaneous instances

- Every list mutation runs off the input thread, in request order within an
  instance (`ui/store_sync.rs::change_store`). The writer holds an exclusive
  OS lock on the separate `repos.lock` file, loads the latest `repos.json`,
  applies only the requested operation, and saves via atomic replacement
  (`store/shared.rs::RepoStore::update`). `--add` uses the same transaction.
  Quitting waits for accepted list requests, including Add Repository discovery,
  to finish, displays a saving notice, and ignores further input. Reloads do not
  delay quitting. Lock acquisition times out after 5 s instead of blocking exit
  indefinitely. During quit, all queued requests share the remaining 5 s lock
  wait budget; requests fail without another wait once that budget expires.
- All instances share group names, membership, order, and fold state. Selection,
  pane focus, filter text, and Pending Only remain local. A later operation on
  the same item applies to the latest state; operations targeting a deleted
  repository or a deleted/renamed group fail instead of recreating it. Enter
  toggles the latest stored fold state, while left/right always queue the
  explicit desired state, so fast repeated keys retain their order. Creating
  a group and assigning its repository is one transaction.
- A native, non-recursive watcher watches the canonical configuration directory,
  not the replaced file (`store/shared.rs::StoreWatcher`). No periodic file
  metadata/content polling is used. Notifications are coalesced over a 150 ms
  window and processed by the event loop (idle wake-up is at most 1 s, plus
  backend notification latency). Only an actual list change rebuilds rows;
  newly listed repositories are scanned without rescanning unchanged entries.
- `ctrl+u` and palette Refresh All reload the list and rescan Git state.
  Terminal focus also reloads the list, even inside the existing 5 s Git scan
  throttle. The periodic Git scan does not reload the list.
- Reloads preserve selection by path and keep filters and dialog input. If a
  selected repository is inside a group folded by a peer (or moved into an
  already folded group), keep that row and a selected worktree's parent visible
  locally without changing shared fold state. Its diff remains open; navigating
  away removes the temporary rows. Explicit Fold All Groups in this instance
  selects the owning header and hides the inspected row; Fold/Unfold All are
  disabled while either filter is active. If the repository is removed from the list,
  the existing nearest-repository rule applies
  and its file/diff view is reset. Running Git writes are not cancelled;
  scan results for removed entries are discarded.
- Missing, malformed, or unreadable files keep the current view and report an
  error. A list already loaded from disk is not recreated after deletion by a
  subsequent UI edit. Repairing it allows subsequent reloads and writes,
  including after an initial malformed-file error. An initially absent list
  may be created by the first edit and refreshes quietly while still absent;
  `--add` may also create a missing list. If the initial file was invalid and
  never successfully loaded, removing it permits a fresh list without restart.
  Errors include the file path. The initial reload always runs after watcher
  registration to close the initial load/watch gap.
- The group-name dialog stays open during its save, ignores repeated input,
  closes on success, and retains its text with an inline error on failure.
  A background error alert appearing during the save remains visible; dismissing
  it returns to Normal after a successful group save.
- Watch failures are reported; Refresh All and focus reloads remain available.
  Locks protect cooperating versions of ular-git, not arbitrary editors or old
  executables. Restart all instances after upgrading. Native notifications may
  be unavailable on network filesystems; explicit refresh remains necessary.

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

- Shared list: `store/shared/tests.rs` (competing processes, invalid/missing
  files, native notifications across replacements), `ui/store_sync_tests.rs`
  (two instances, queued operations, stale dialogs, selection/filter preservation,
  recovery, fast fold keys, first-run reloads, peer folds, real dialog conflicts,
  quit during discovery, non-blocking lock wait), `runtime/tests.rs` (input
  suppression while quitting).
- Unit tests: `ui/tree.rs` (row shapes), `store.rs` (order, version 1
  migration), `ui/pane_keys_tests.rs` (`alt+↑↓`), `cache.rs`,
  `git/status.rs`, `git/worktree.rs`.
- Real git: `git/tests.rs` (scan, worktrees, discovery), `ui/tests.rs`
  (filter, groups, add / remove, worktree removal).
- Manual: [testing.md](./testing.md) §Manual checks.
