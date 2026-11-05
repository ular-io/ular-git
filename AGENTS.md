# Repo Agent Notes

ular-git is a Rust terminal UI (ratatui/crossterm) that shows many local git
repositories at once: status badges, linked worktrees, changed files, and
diffs. It reads git through the `git` CLI. Its only writes are commit, push,
`pull --ff-only`, discarding one file's changes, and `git worktree remove`.
Open With (`enter`) starts user-configured external apps in a working tree;
those apps may change it, and ular-git rescans on return. The executable is
`ular-git`.

## Routing: read before you change

Read only the document for the area being changed. Use
[architecture.md](./docs/architecture.md) when a change crosses areas or the
owning module is unclear. Use [backlog.md](./docs/backlog.md) only for feature
planning; backlog entries are not implementation authorization.

| If you change… | Read first |
| --- | --- |
| Repository list, groups, filter, Add / Remove Repository, worktree rows, scanning, status cache (`src/ui/tree.rs`, `src/ui/scan.rs`, `src/store.rs`, `src/cache.rs`, `src/git/{status,worktree,discover}.rs`) | [repositories.md](./docs/repositories.md) |
| Files pane, diff pane, commit, push / pull, worktree removal (`src/ui/files.rs`, `src/ui/commit_files.rs`, `src/ui/diff_view.rs`, `src/ui/sync.rs`, `src/git/{diff,ops,remote}.rs`, `src/ui/overlays/{commit,confirm}.rs`) | [git-operations.md](./docs/git-operations.md) |
| TUI layout, pane widths, focus, badges, header / footer, dialogs, keys, help, palette (`src/ui/render/`, `src/ui/keys.rs`, `src/ui/input.rs`, `src/ui/overlays/`, `src/ui/quick/`) | [ui-style-guide.md](./docs/ui-style-guide.md) |
| Terminal lifecycle, paste, text input (`src/runtime.rs`, `src/runtime/terminal.rs`, `src/ui/components/input.rs`) | [architecture.md](./docs/architecture.md) §Runtime + [testing.md](./docs/testing.md) |
| Open With apps and file versions (`src/ui/overlays/open_with.rs`, `src/ui/file_versions.rs`, `src/scratch.rs`, `src/git/blob.rs`, `src/launch.rs`, `[[apps]]` / `[[file_apps]]` / `[[diff_apps]]` in `src/config.rs`) | [architecture.md](./docs/architecture.md) §Runtime |
| Release process | [releasing.md](./docs/releasing.md) |

For past reasoning, prefer `git log -- <path>`, `git blame`, and `git show`.
Completed plans and chronological change logs are intentionally not kept as
active documentation.

## Critical rules

### Security, privacy, language

- **Public repository.** Never hard-code or commit personal local settings
  (absolute paths, local folder names, repository lists), secrets (API keys,
  tokens), or other sensitive personal information in code or documentation.
  Examples use neutral paths such as `~/code`.
- **English only.** All committed source, comments, documentation, and commit
  messages must be written in English. Comments give an agent context; do not
  restate what the code already makes obvious. The one exception is test input
  that exercises non-ASCII handling (Korean text for double-width and
  grapheme tests). Any file containing other Korean may be added to
  `.gitignore` only after the user approves.

### Source file size

- A source file (`*.rs`, counted with `wc -l`, inline `#[cfg(test)]` modules
  included) is at most 500 lines. `src/theme/builtin.rs` (a palette data table)
  is exempt.
- The limit is enforced **incrementally, only on files a change already
  edits**. New files: at most 500 lines. An edited file that would go over is
  split in the same change by responsibility (tests by topic), not by line
  count.
- Do the split as its own commit before the behavior change: moves only, no
  logic edits, `scripts/check.sh` passing.
- Find over-limit files with
  `find src -name '*.rs' -exec wc -l {} + | awk '$1 > 500 && $2 != "total"'`.

### Layers

- `git/` knows git and nothing about the UI. Every call goes through
  `git::git_command` (optional locks off, no prompts, no pager); network calls
  go through `git::remote::run_network`.
- `ui/` key handlers never run git. Background work returns a `Patch`
  applied in `App::poll_background`.
- `ui/components/` must not know repositories or git.
- Header and help read the same key hints (`src/ui/keys.rs`); change them there.
- Colors only through `Theme` roles; no literal colors in rendering.

### Verification

Every code change must pass `scripts/check.sh` (`cargo fmt --all -- --check`,
`cargo test -q`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo build --release`) and then be installed with `cargo install --path .`
so the next `ular-git` launch runs it. A running instance keeps the old binary
until it is restarted; say so when reporting.
[testing.md](./docs/testing.md) is the authoritative matrix; the essentials:

- **Layout, dialogs, keys**: a real-terminal look at 160 / 100 / 56 columns
  with a sandbox `HOME` (never the real `~/.config/ular-git`).
- **Push / pull**: automated tests use a local bare remote. In manual checks,
  never press `P` / `p` on a real repository; use a scratch repository with a
  bare remote.
- **Edit Config** launches `$EDITOR`; do not trigger it in automated runs when
  that is a GUI editor. **Open With** runs real apps (the default `open .`
  opens Finder); use a sandbox `config.toml` with harmless apps.

When behavior changes, update the owning document in the same change.

## Current hazards

- `git status --untracked-files=all` walks untracked folders. A repository
  with a huge untracked tree scans slowly; the stored file list is capped at
  2000 entries, but the walk itself is not.
- Push / pull never prompt for credentials. A remote whose credentials are not
  in the credential helper or `ssh-agent` fails with git's message.
- Circled-letter badges (`Ⓜ`, `Ⓢ`) are East Asian Ambiguous width; many fonts
  draw them two cells wide, so they are always followed by a space.
