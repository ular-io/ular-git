# UI Style Guide

> Status: Current
> Read when: Changing TUI layout, pane widths, focus, badges, header, footer,
> dialogs, keys, help, or the palette.
> Entry points: `src/ui/render/`, `src/ui/keys.rs`, `src/ui/input.rs`,
> `src/ui/overlays/`, `src/ui/quick/`, `src/ui/components/modal.rs`,
> `src/theme/`

The conventions follow the rust-tui style shared with ular-s7s (k9s-like
header, thick focused frames, footer for results only). This file records how
ular-git applies them.

## Screen

| Area | Content |
| --- | --- |
| Header (5 rows, no border) | Context column (Repos totals; the selected Repo — `repo › worktree` for a worktree — with its Branch and Path; Filter while one is applied), the focused pane's two shortcut columns, the common column (`:` `!` `ctrl+u` `?` `q`), the logo (`UlarGit` in figlet `small`, 29 × 4; shown from about 162 columns with Repos focus, 160 with Files focus) |
| Filter line (3 rows, only while typing `/`) | The repository filter input |
| Panes | Repos · Files · Diff |
| Footer (1 row) | The latest result with a text tag (`[INFO]` `[OK]` `[WARN]` `[ERROR]`) for 6 s, then nothing. Never action hints or pane names |

## Width

- `1` / `2` / `3` choose how many panes are shown side by side
  (`App::pane_count`, default 3, saved in `view.json`). The width caps the
  count: 3 from 160 columns, 2 from 64, 1 below. A key the width cannot
  show does nothing, so below 64 columns none applies. `\` moves to the
  next count the width can show, starting from the count shown now
  (3 → 2 → 1 → 3, or 2 ↔ 1).
- 1: the focused pane alone. 2: the focused pane and the one to its right;
  Diff has none, so Diff focus shows Files + Diff. 3: all three (Repos
  22 % 30–44, Files 32 % 40–80, Diff the rest).
- The count stays while the focus moves; moving left or right always
  reveals the pane the focus moves to.
- Repos 42 % (28–44) beside Files; Files 40 % (30–80) beside Diff.
- Repos shrinks below those widths to fit its longest name (never below the
  minimum): every registered repository, group header, and known worktree
  folder, shown or not, plus 7 cells kept for badges (`Ⓜ 12 ↑3`). Filtering,
  folding, and rescanning never move the panes; only adding or removing a
  repository, group, or worktree does. Longer names, branches, and badges
  wider than 7 cells are cut (`repos::fit_width` in
  `src/ui/render/repos.rs`). The space it frees goes to Diff (three panes) or
  Files (two).
- The file pane is titled with the selected repository's (or worktree's)
  folder name, also inside a commit's or the uncommitted changes' files. Its bottom border carries the commit count, right-aligned
  and soft-dim, one border cell before the corner (`pane_footer`):
  `━ 2 / 123 commits ━┛`. The bottom border holds nothing else, so it never
  meets the `←` / `→` arrows on the top border.
- Header: when space runs out the logo goes first, then shortcut columns from
  the right. Below 40×10 only `Terminal too small for ular-git` is drawn.

## Focus and tone

- Focused pane: `Thick` border, `accent` + bold title, `←` / `→` on the top
  border only for the directions it can move.
- Another pane focused: `Plain` border, soft-dim frame and title. The
  content is **never dimmed**: text, diff colors, status letters, badges, and
  the history graph look the same focused or not, because the user moves
  through repository or file rows while reading the panes beside them. Focus
  shows only in the frame (border, title, arrows) and the selection bar.
- A dialog, help, the palette, or the filter line owns input: no pane is
  dimmed.
- The history graph is lane color faded once toward the background (`↑` /
  `↓` nodes: full branch color), and history ref labels use their faded
  colors, in every pane state.
- The selected history commit's second line (subject-only rows) sits on
  the selection bar; its date and author use the selection text color faded
  toward the selection background, so it reads as secondary in every theme
  and pane state.
- Row content in the repository and file panes is never bold: status codes,
  badges, `↑` / `↓` nodes, ref labels, and the running `push…` / `pull…`
  label use color alone. Bold marks focus only (the focused pane's selection
  bar, title, and border).
- Selected row: the bar runs from the left padding cell to the cell before the
  right border (where the scrollbar sits). Focused → `selection_bg` +
  `selection_fg` + bold; another pane focused → `fg` text, not bold, on
  `muted` moved 40% toward the background (not reversed: reversed text is
  pale on a mid-tone bar); neutral → `selection_inactive_bg` +
  `selection_fg` + bold. Text keeps one cell of margin on the right.

## Repository rows

- Group headers are muted (`muted` text, no bold) on `group_bg`, so they label
  the rows below without outweighing them. The `group_bg` band stays when the
  repository pane loses focus, and a folded group's totals keep the row badge
  colors (`Ⓜ N` `warning`, `↑N` / `↓N` `branch`).
- Badge colors: conflicts `error` bold, changed `warning`, ahead / behind
  `branch`, stashes soft-dim, failed scan `error`, running push / pull
  `accent` bold. A clean tree has no badge.
- **Circled letters (`Ⓜ`, `Ⓢ`) are always followed by a space.** They are East
  Asian Ambiguous: counted as one cell but drawn two cells wide by many fonts,
  so a digit right after them is covered.

## Diff colors

`diff_add` added, `diff_del` removed, unchanged (context) lines in `muted`
moved 25% toward the background,
soft-dim meta and the commit byline, bold commit subject, `accent` bold
section and file titles.
Title lines run from border to border and join it (`├ ┤` on a plain frame,
`┠ ┨` on the focused thick frame, in the border's style). Every label is
centered: `├─── Unstaged changes ───┤`, `├─── src/app.rs ───┤`,
`├─── 1 file changed, … ───┤`. Hunk headers are replaced by a `dim` cut line
(`- - -`) between hunks, from where the code starts to the right edge.
The line-number gutter (`dim`) is on code lines only; its columns are sized
to the diff's largest numbers, and a column with no numbers is left out.

## Theme roles specific to ular-git

The theme set is shared with ular-s7s; these roles were renamed for git:
`branch`, `diff_add`, `diff_del`, `diff_hunk`, `untracked`, `group_bg`.

## Dialogs

- `modal_block` + `render_modal`; thick frame, centered title, one-cell side
  margin, the screen behind faded (except theme select and help).
- Buttons: action left, Cancel right, one blank row above. Destructive actions
  (Remove Worktree, Remove Repository, Delete Group) focus Cancel; Push focuses
  Push.
- `enter` in a text field moves on; it never submits. `←` / `→` pick a button
  or a radio option and stop at the ends; `tab` cycles.
- A notice line under the fields is always reserved for validation errors.
- No key hints inside dialogs; help documents them.
- Confirmation text keeps deliberate blank lines.

## Keys, header, help

- `src/ui/keys.rs` is the single source for the header columns and help.
- Common keys (`:` `!` `ctrl+u` `?` `q` in the common column, plus the pane
  count `1` / `2` / `3` / `\` in help only) work in every pane. Every other
  key works only in a pane whose header lists it: `/` `0` `+` on Repos, `>`
  on Files (not a commit's file list), `s` on Diff; `c` `P` `p` are listed on
  all three. `.` differs per pane: Pending Only on Repos, history date /
  author on Files (not a commit's file list), line wrap on Diff. While a filter is applied, Repos shows `0` Clear Filter in place
  of `/` (help keeps `/`). The palette runs any command whatever the focus.
  Labels follow the selected row (`ctrl+d` reads `Remove Worktree` on a
  worktree, `Delete Group` on a group); keys that do nothing there are left
  out.
- At most five rows per header column. Movement keys (`↑↓`, `pgup` / `pgdn`,
  `g` / `G`, arrows) and the Repos reorder keys (`alt+↑↓`) are left to help.
- Help is a centered box: the focused pane's keys, then Anywhere and Dialogs;
  two columns when wide, one when narrow, scrolling with a scrollbar when it
  does not fit. It takes every key; `esc`, `?`, or `q` closes it.
- The front-most layer takes every key; only `ctrl+c` passes through (quit
  needs two presses).

## Palette

`:` lists every command with its shortcut. Disabled commands sort last and the
footer line says why. Recently used commands sort first. Commands take no
arguments; a value comes from a dialog. Typing `!` into an empty palette
switches to the terminal command, and `:` switches back.

## Verification

Layout or dialog changes need the release build and a real-terminal look
([testing.md](./testing.md) §Manual checks). `ui/render/tests.rs` pins the
width rules, the logo, the selection bar, the gutter, badge spacing, and help
fitting 80×24.
