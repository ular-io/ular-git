# Architecture Map

> Status: Current
> Read when: A change crosses feature boundaries or its owning module is unclear.
> Entry points: `src/lib.rs`, `src/runtime.rs` (+ `src/runtime/terminal.rs`), `src/ui/mod.rs`

## Execution modes

| Command | Path |
| --- | --- |
| `ular-git` | `runtime::run` → `App::new` (load list + status cache, start scan pool) → `TerminalSession::enter` → `run_loop` |
| `ular-git --add <dir>` | `runtime::add_from_cli` → `git::discover::repos_for` → `RepoStore::save`, then exit (no TUI) |

## Module map

| Module | Owns |
| --- | --- |
| `git/mod.rs` | `git_command` (shared env: `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, no pager), `scan`, status types |
| `git/status.rs` | `status --porcelain=v2 --branch -z` parsing, unpushed log, stash count |
| `git/worktree.rs` | `worktree list --porcelain -z` parsing |
| `git/history.rs` | `git log --graph` history (graph prefix + commit per line) |
| `git/diff.rs` | Diff loading (file sections, untracked, conflict, `git show`), line classification and numbering, line limit |
| `git/ops.rs` | Commit, worktree remove / prune |
| `git/remote.rs` | Push, pull `--ff-only`, default remote; network env and timeout |
| `git/discover.rs` | Folder → repository roots |
| `store.rs` | `repos.json`: groups and repositories |
| `cache.rs` | Status cache file |
| `config.rs` | `config.toml`, app paths, atomic writes |
| `theme/` | Theme roles, 40 built-in palettes, custom themes, selection |
| `ui/mod.rs` | `App`, `UiMode` (who gets keys), `Pane` (focus), row rebuild and cursor placement |
| `ui/background.rs` | Scan pool, one-off threads, `Patch` channel |
| `ui/scan.rs` | Scan scheduling, focus rescan, auto rescan, cache load / save |
| `ui/tree.rs` | Repository pane rows (pure) |
| `ui/files.rs`, `ui/diff_view.rs`, `ui/history_view.rs` | File-pane items (files, history commits, `… more`) and selection; latest-wins diff and history loaders |
| `ui/sync.rs` | Push / pull from the UI |
| `ui/input.rs`, `ui/keys.rs` | Key routing; header / help hints |
| `ui/render/` | Frame, pane layout, header, panes |
| `ui/overlays/` | Dialogs: add repository, groups, confirm, commit, help, theme, message |
| `ui/quick/` | `:` palette and `!` terminal command |
| `ui/components/` | Repository-agnostic primitives (text input, list movement, modal, scrollbar, text width) |
| `runtime.rs`, `runtime/terminal.rs` | CLI, event loop, shell handover, terminal modes |

## Data flow

```
key ──► App::on_key ──► state change / bg.submit(job) / bg.spawn(job)
                                   │
                   worker thread: git … ──► Patch ──► channel
                                                        │
run_loop: poll_background (apply every Patch) ─► tick (timers) ─► draw once
```

- Scans: `request_scan(path)` → pool job `git::scan` → `apply_scan` updates
  `App::statuses`, rescans the worktrees a repository reports, rebuilds rows,
  refreshes the file pane when the scanned tree is selected.
- Diffs: `select_file` → `load_diff` (one in flight; newer requests replace
  the queued one; results of older generations are dropped).
- Writes (commit, push, pull, worktree removal): a one-off thread, then a
  rescan of the affected tree.

## Runtime

- The loop waits for the first event, drains the queue, then draws once. While
  jobs are pending it wakes every 100 ms (spinner, results); otherwise it
  sleeps until the next deadline (`App::next_deadline`: notice expiry, focus
  rescan, auto rescan), at most 1 s.
- `TerminalSession` (RAII) enables raw mode, the alternate screen, bracketed
  paste, focus reporting, and keyboard enhancement, and restores them on every
  exit path, including the panic hook. Focus gained triggers a rescan.
- `!` and Edit Config hand the terminal to `$SHELL -c <command>`; on return the
  config is reloaded and every repository rescanned.

## Persistence ownership

| File | Owner | Writer |
| --- | --- | --- |
| `~/.config/ular-git/repos.json` | app | `App::save_store` after each list / group change. A malformed file is never overwritten; that session does not save |
| `~/.config/ular-git/config.toml` | user | Seeded once by `ensure_config_template`; never rewritten by the app |
| `~/.config/ular-git/theme.json` | app | Theme dialog |
| `~/.config/ular-git/themes/*.toml` | user | — |
| `~/.config/ular-git/{quick,terminal}_history.json` | app | Palette / `!` |
| `<OS cache>/ular-git/status.json` | app | `save_status_cache` when the scan queue drains and on exit (versioned; another version is discarded) |

## Design decisions

- **git CLI, not libgit2.** It honors the user's git config, fsmonitor, and
  hooks exactly as their shell does and needs no C build. Optional locks are
  off, so a scan never takes `index.lock` from an editor.
- **Background scans with a persisted cache.** Measured with 28 repositories
  (macOS, git 2.54): one `git status --untracked-files=all` takes 30 ms median,
  161 ms worst; a full scan takes 4.2 s sequentially and 1.4 s with 8 in
  parallel. Reading on the UI thread would freeze the screen, so scans run on a
  pool and the last result is cached for an instant launch.
- **No file watching.** Many repositories × large trees would need many
  watches, and `.git` internals churn. Periodic rescans, the focus rescan, and
  terminal focus events keep data fresh without that cost.
- **Prompts are impossible by construction** for background git: stdin is
  null, `GIT_TERMINAL_PROMPT=0`, SSH runs with `BatchMode=yes`.

## Cross-cutting change map

| Change | Touches |
| --- | --- |
| New badge or status field | `git/status.rs` (parse) → `git::RepoStatus` → `cache.rs` `CACHE_VERSION` if the meaning of a field changes → `ui/render/repos.rs::badges` → [repositories.md](./repositories.md) |
| New git write | `git/ops.rs` or `git/remote.rs` → a `ui/` caller using `bg.spawn` → palette entry (`ui/quick/registry.rs`, `quick/input.rs`) → key hints (`ui/keys.rs`) → [git-operations.md](./git-operations.md) |
| New dialog | `UiMode` variant + `Option<…>` on `App` → `ui/input.rs::on_key` → `ui/render/mod.rs::draw` → `components/modal.rs::backdrop_dimmed` → [ui-style-guide.md](./ui-style-guide.md) |
| New key | `ui/input.rs` → `ui/keys.rs` (header and help) → palette entry → README shortcut table |
