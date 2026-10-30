# Testing Guide

> Status: Current
> Read when: Verifying any code change.
> Entry point: `scripts/check.sh`

## Routine

```bash
scripts/check.sh        # fmt --check → test → clippy -D warnings → build --release
cargo install --path .  # replace the installed `ular-git`
```

Both run after every change. `check.sh` fails fast and names the failing
stage.

## Verification matrix by change area

| Change | Automated | Manual |
| --- | --- | --- |
| Status / worktree parsing | `git/status.rs`, `git/worktree.rs` unit tests, `git/tests.rs` | — |
| Diff loading or classification | `git/diff.rs` unit tests, `git/tests.rs` | Look at a commit, an untracked file, and a staged + unstaged file |
| Commit, worktree removal | `git/tests.rs`, `ui/tests.rs` | Scratch repository only |
| Push / pull | `git/tests.rs`, `ui/tests.rs` (local bare remote) | Scratch repository with a bare remote only — never a real repository |
| Repository rows, groups, filter | `ui/tree.rs`, `ui/tests.rs` | Real terminal |
| Layout, header, panes, dialogs, help | `ui/render/tests.rs` | Real terminal at 160 / 100 / 56 columns |
| Keys | `ui/tests.rs` | Help shows only keys that work in the pane |
| Terminal lifecycle | `runtime/terminal/tests.rs` | Quit, `!` command round trip, focus regain rescan |

## Unit test policy

- Tests that run git use `git::test_repo::TestRepo`: a temporary repository
  with a fixed identity and global hooks disabled (`core.hooksPath=/dev/null`),
  so the developer's commit-message hooks never judge test commits.
- Never point a test at a real repository or the real `~/.config/ular-git`.
  `App::new` loads nothing from disk under `cfg!(test)`; build test apps with
  `ui::test_support::scanned_app`.
- UI tests drive `App` with keys and wait for background jobs with
  `App::wait_background`; rendering tests read a `TestBackend` buffer
  (`test_support::screen`).

## Manual checks

Use a sandbox `HOME` so the real list, cache, and theme are untouched:

```bash
mkdir -p /tmp/ular-git-home
HOME=/tmp/ular-git-home ./target/release/ular-git --add ~/code
tmux new-session -d -s ular-git -x 160 -y 40 \
  "HOME=/tmp/ular-git-home $PWD/target/release/ular-git"
tmux capture-pane -t ular-git -p          # text
tmux capture-pane -t ular-git -p -e       # with colors
tmux resize-window -t ular-git -x 100 -y 30
tmux kill-session -t ular-git
```

- A sandbox `HOME` still reads the real repositories it was given. Before
  pressing `c`, `P`, `p`, or `ctrl+d`, make sure the selected row is a scratch
  repository.
- For push / pull, create a scratch repository and a bare remote:

  ```bash
  git init -q --bare /tmp/ular-git-remote.git
  git -C <scratch> remote add origin /tmp/ular-git-remote.git
  git -C <scratch> push -q -u origin main
  ```

- Edit Config launches `$EDITOR`; with a GUI editor it opens a window. Do not
  trigger it in unattended runs.
- Open With runs real apps; the default folder app (`open .`) opens a Finder
  window. In manual checks give the sandbox `config.toml` harmless `[[apps]]`
  (`true {path}` with `gui = true`, a failing `open -a NoSuchApp`, a terminal
  app such as `gitui`) and quit `vim` with `:q!`.

## When to update tests and docs

- A new git write gets a `git/tests.rs` test against real git and a
  `ui/tests.rs` test through the keys.
- A layout rule gets a `ui/render/tests.rs` test that reads the buffer.
- Behavior changes update the owning document in the same change (see the
  routing table in `AGENTS.md`).
