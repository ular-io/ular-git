# ular-git

Many git repositories at a glance, in the terminal: which ones have
uncommitted changes or unpushed commits, which worktrees exist, which files
changed, and what the diff is.

```
┏━ ← ━━━━━━━━ api ━━━━━━━ → ━┓┌───────── src/app.rs +4 -1 ─────────────┐
┃  M src/app.rs              ┃├────────── Unstaged changes ────────────┤
┃ A  src/new.rs              ┃│   10   10      let c = load();         │
┃ ?? notes.txt               ┃│        11 +    c.check()?;             │
┃  ↑ fix: retry on timeout   ┃│   11   12      start(c)                │
┃                            ┃│    ⋯                                   │
┃                            ┃│   40   41  fn stop() {                 │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└────────────────────────────────────────┘
```

## Key Features

- **Repository list with status badges**: `Ⓜ N` changed files, `!N` merge
  conflicts, `↑N` / `↓N` ahead of / behind the upstream, `Ⓢ N` stashes; a
  clean repository shows none.
- **Worktrees** nest under the repository they belong to, with their branch.
- **Groups**: file repositories under groups, fold them, filter by name.
- **Arrow-key flow**: Repos → Files → Diff; the diff follows the file cursor.
- **History with the branch graph** below the changed files (unpushed
  commits marked `↑`); any commit opens as `git show`.
- **Fast with many repositories**: the last status is cached so launch is
  instant, and scans run in parallel in the background without blocking keys.
- **Few, explicit writes**: commit (`c`), push (`P`, asks first), pull (`p`,
  fast-forward only), discarding one file's changes (palette, asks first), and
  removing a worktree (`ctrl+d`). Nothing else writes to your repositories.
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
| `←→` `h l` | Move between panes (`enter` too, on a history commit) |
| `1` `2` `3` | Show 1 pane (the focused one), 2 (it and the one to its right), or all 3, as the terminal width allows (remembered) |
| `\` | Cycle the pane count 3 → 2 → 1 → 3 (2 ↔ 1 when only two panes fit) |
| `→` (history commit or Uncommitted Changes) | List its files; each shows its own diff, `←` goes back |
| `enter` (repository, worktree, or file) | Open With: pick an app from `config.toml` and open it there; on a file, `←→` pick a diff, the file before / after, or the working tree's |
| `/`, `0` | Filter repositories, clear the filter |
| `.` (Repos pane) | Show only repositories with uncommitted changes, commits to pull or push, or a stash (toggle) |
| `+` | Add repository |
| `ctrl+n` | New group |
| `ctrl+e` | Rename a group, or move a repository to a group |
| `ctrl+d` / `delete` | Remove from the list · delete a group · remove a worktree (by row) |
| `←` `→` / `enter` (group header) | Fold / unfold a group (`enter` toggles) |
| `c` | Commit (staged only, or all changes including untracked files) |
| `P`, `p` | Push (asks first), pull (fast-forward only) |
| `.` (Files / Diff pane) | Toggle diff line wrap |
| `>` | History labels: none → branches → branches and tags → tags (remembered) |
| `s` | Side-by-side diff when the diff pane is 120+ columns wide, or always unified (remembered) |
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
| `max_diff_file_mb` | `10` | Larger files show as binary in diffs, unread (`0` = git's 512 MiB) |
| `editor` | `$VISUAL`, `$EDITOR`, `vi` | Editor for Edit Config |
| `[[apps]]` | Open Folder (`open .`) | Open With apps for a repository or worktree |
| `[[file_apps]]` | vim (`vim {path}`) | Open With apps for a file (before, after, or in the working tree) |
| `[[diff_apps]]` | vimdiff (`vimdiff {old} {new}`) | Open With apps for a file's Diff tab |

Each app has a `name`, a `command` run in the working tree (`{path}` becomes
the quoted folder or file path, `{old}` and `{new}` the two files of a diff),
and `gui = true` for apps that open their own window. Versions read from git
are opened as read-only temporary copies, removed when ular-git exits. Terminal apps (the default) take over the terminal until they exit,
then ular-git comes back and rescans; GUI apps start without leaving ular-git.

```toml
[[apps]]
name = "gitui"
command = "gitui"

[[apps]]
name = "VS Code"
command = "code {path}"
gui = true

[[file_apps]]
name = "vim"
command = "vim {path}"

[[diff_apps]]
name = "VS Code"
command = "code --diff {old} {new}"
gui = true
```

Custom themes go in `~/.config/ular-git/themes/*.toml`; `:` → **Change Theme**
previews them live.

### Push and pull

Both run in the background with no terminal to type a password into. HTTPS
remotes use your git credential helper (for example the macOS keychain); SSH
remotes use keys already loaded in `ssh-agent`. When credentials are missing
the error is shown; run `git push` once in a terminal (`!`) to log in.

## License

MIT
