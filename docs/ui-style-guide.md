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
| Panes | 1 Repos · 2 Files · 3 Diff |
| Footer (1 row) | The latest result with a text tag (`[INFO]` `[OK]` `[WARN]` `[ERROR]`) for 6 s, then `<1> Repos <2> Files <3> Diff` with the focused pane highlighted. Never action hints |

## Width

- ≥ 160 columns: all three panes, whatever has focus (Repos 22 % 30–44,
  Files 28 % 40–60, Diff the rest).
- 64–159 columns: Repos focus shows Repos + Files; Files focus shows
  Files + Diff, so the diff follows while files are browsed; Diff focus
  shows the diff alone at full width for reading. Below 64 columns only the
  focused pane is shown.
- Moving left or right always reveals the pane the focus moves to.
- Repos 42 % (28–44) beside Files; Files 40 % (30–56) beside Diff.
- The file pane is titled with the selected repository's (or worktree's)
  folder name.
- Header: when space runs out the logo goes first, then shortcut columns from
  the right. Below 40×10 only `Terminal too small for ular-git` is drawn.

## Focus and tone

- Focused pane: `Thick` border, `accent` + bold title, `←` / `→` on the top
  border only for the directions it can move.
- Another pane focused: `Plain` border, soft-dim frame, title, and text. The
  diff keeps its colors faded halfway toward the background so `+` / `-` stay
  readable.
- A dialog, help, the palette, or the filter line owns input: no pane is
  dimmed.
- Faded colors fade further when their pane loses focus, never back to
  `muted`: the history graph is lane color faded once when focused and twice
  when not (`↑` / `↓` nodes: full branch color, then faded once). `muted` can
  be stronger than a faded color, so swapping to it would make a dimmed pane
  look bolder.
- Selected row: the bar runs from the left padding cell to the cell before the
  right border (where the scrollbar sits). Focused → `selection_bg` +
  `selection_fg` + bold; dimmed pane → `selection_inactive_bg` with the plain
  text color (`fg`) at normal weight, no reverse; neutral → `selection_inactive_bg` +
  `selection_fg` + bold. Text keeps one cell of margin on the right.
  (Deliberately not the rust-tui weak reverse — muted text reversed over the
  inactive color — which put pale text on a mid-tone bar on light themes.)

## Repository rows

- Group headers are muted (`muted` text, no bold) on `group_bg`, so they label
  the rows below without outweighing them.
- Badge colors: conflicts `error` bold, changed `warning`, ahead / behind
  `branch`, stashes soft-dim, clean `success`, failed scan `error`, running
  push / pull `accent` bold.
- **Circled letters (`Ⓜ`, `Ⓢ`) are always followed by a space.** They are East
  Asian Ambiguous: counted as one cell but drawn two cells wide by many fonts,
  so a digit right after them is covered.

## Diff colors

`diff_add` added, `diff_del` removed, soft-dim meta and
the commit byline, bold commit subject, `accent` bold section and file titles.
Title lines run from border to border and join it (`├ ┤` on a plain frame,
`┠ ┨` on the focused thick frame, in the border's style). Every label is
centered: `├─── Unstaged changes ───┤`, `├─── src/app.rs ───┤`,
`├─── 1 file changed, … ───┤`. Hunk headers are replaced by a `dim` `⋯` gap
line between hunks (in the old-number column when the gutter is shown). The
line-number gutter (`dim`) is on code lines only.

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
  Labels follow the selected row (`ctrl+d` reads `Remove Worktree` on a
  worktree, `Delete Group` on a group); keys that do nothing there are left
  out.
- At most five rows per header column. Movement keys (`↑↓`, `pgup` / `pgdn`,
  `g` / `G`, arrows) are left to help.
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
