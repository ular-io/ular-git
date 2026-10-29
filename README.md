# ular-git

Many git repositories at a glance, in the terminal: which ones have
uncommitted changes or unpushed commits, which worktrees exist, which files
changed, and what the diff is.

```
┏━━━━━━━ Repos[12] ━━━━━ → ━┓┌──── Files[3] ─────┐┌──────── app.rs +4 -1 ──────────┐
┃ dotfiles               ✓ ┃│  M src/app.rs     ││ ── Unstaged changes ────────── │
┃ ▾ work                   ┃│ A  src/new.rs     ││ @@ -10,6 +10,9 @@ fn run()     │
┃   api             Ⓜ 3 ↑1 ┃│ ?? notes.txt      ││   10   10      let c = load(); │
┃   └ api-fix fix/x    Ⓜ 1 ┃│                   ││        11 +    c.check()?;     │
┃   web                  ✓ ┃│                   ││   11   12      start(c)        │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━┛└───────────────────┘└────────────────────────────────┘
```

## Key Features

- **Repository list with status badges**: `Ⓜ N` changed files, `!N` merge
  conflicts, `↑N` / `↓N` ahead of / behind the upstream, `Ⓢ N` stashes, `✓`
  clean.
- **Worktrees** nest under the repository they belong to, with their branch.
- **Groups**: file repositories under groups, fold them, filter by name.
- **Arrow-key flow**: Repos → Files → Diff; the diff follows the file cursor.
  Unpushed commits are listed after the changed files and open as `git show`.
- **Fast with many repositories**: the last status is cached so launch is
  instant, and scans run in parallel in the background without blocking keys.
- **Few, explicit writes**: commit (`c`), push (`P`, asks first), pull (`p`,
  fast-forward only), and removing a worktree (`ctrl+d`). Nothing else writes
  to your repositories.
- Quick command palette (`:`), shell command in a repository (`!`), 40 themes,
  help (`?`).

## Build / Installation

Homebrew distribution (`ular-io/tap`) starts with the first release. Until
then, build from source (the Rust toolchain is pinned in
`rust-toolchain.toml`):

```bash
git clone https://github.com/ular-io/ular-git.git
cd ular-git
cargo install --path .     # installs the `ular-git` executable
```

A short alias is handy:

```bash
alias ugit='ular-git'
```

## Usage

```bash
ular-git --add ~/code   # add every repository below a folder (or one repository)
ular-git                # start the TUI
```

`+` in the TUI does the same as `--add`: give it a repository, a folder inside
one, a linked worktree (resolved to its repository), or a parent folder
(every repository up to three levels below it is added).

### Shortcuts

| Key | Action |
| --- | --- |
| `↑↓` `j k`, `pgup pgdn`, `home end` `g G` | Move in lists; scroll the diff |
| `←→` `h l`, `enter` | Move between panes |
| `1` `2` `3` | Focus Repos / Files / Diff |
| `/`, `0` | Filter repositories, clear the filter |
| `+` | Add repository |
| `ctrl+n` | New group |
| `ctrl+e` | Rename a group, or move a repository to a group |
| `ctrl+d` / `delete` | Remove from the list · delete a group · remove a worktree (by row) |
| `space` | Fold / unfold a group |
| `c` | Commit (staged only, or all changes including untracked files) |
| `P`, `p` | Push (asks first), pull (fast-forward only) |
| `.` | Toggle diff line wrap |
| `:`, `!` | Quick command palette, shell command in the selected repository |
| `ctrl+u` | Rescan every repository |
| `?` | Help for the focused pane |
| `q` / `ctrl+c` (twice) | Quit |

### Configuration

`:` → **Edit Config** opens `~/.config/ular-git/config.toml` (created with
every option commented out):

| Key | Default | Meaning |
| --- | --- | --- |
| `refresh_interval_secs` | `60` | Rescan every repository this often (`0` = off) |
| `scan_jobs` | `8` | Parallel git processes during a scan (1–32) |
| `max_diff_lines` | `20000` | Diff lines read for one file |
| `editor` | `$VISUAL`, `$EDITOR`, `vi` | Editor for Edit Config |

Custom themes go in `~/.config/ular-git/themes/*.toml`; `:` → **Change Theme**
previews them live.

### Push and pull

Both run in the background with no terminal to type a password into. HTTPS
remotes use your git credential helper (for example the macOS keychain); SSH
remotes use keys already loaded in `ssh-agent`. When credentials are missing
the error is shown; run `git push` once in a terminal (`!`) to log in.

## License

MIT
