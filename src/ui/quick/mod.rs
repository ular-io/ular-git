//! Quick Command window: the `:` palette (every command, searchable) and `!`
//! terminal mode (a shell command run in the selected repository).
//!
//! - Palette matching: whitespace-separated words must all appear in the
//!   label, an alias, or the shortcut (case-insensitive). Enabled commands
//!   sort first, then by recent use, then registry order.
//! - Typing `:` or `!` into an empty input switches the mode.
//! - Both histories persist under `~/.config/ular-git/`.

pub(crate) mod input;
pub(crate) mod registry;
pub(crate) mod render;
pub(crate) mod state;

pub use state::{load_history, load_terminal_history, QuickMode, QuickState};

pub(crate) use render::draw_quick_command;

/// Rows visible in the list; more scroll with the cursor.
const VIEWPORT: usize = 10;
