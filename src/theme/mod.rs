//! Color themes: built-in palettes, custom theme files, and selection persistence.
//!
//! Every color in the UI is a semantic role on [`Theme`]; `render.rs` never uses
//! literal colors. Built-ins ship 40 palettes (20 dark, 20 light) — see `builtin_themes`.
//! Custom themes are user-edited TOML files in `~/.config/ular-git/themes/*.toml`
//! (user-edited file = TOML, app-owned state = JSON — same split as config.toml).
//! The selected theme key is app-owned state persisted to `~/.config/ular-git/theme.json`
//! by the theme dialog (Enter), so config.toml stays untouched by the app.

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use std::path::PathBuf;

/// Key of the theme used when nothing is persisted yet (first run).
pub const DEFAULT_THEME_KEY: &str = "nord";

/// A complete palette of semantic color roles used by the renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Stable identifier persisted in theme.json (kebab-case; file stem for customs).
    pub key: String,
    /// Display name shown in the theme dialog.
    pub name: String,
    /// Dark background palette (drives the dialog tag and pulse fade fallback).
    pub dark: bool,
    /// Loaded from a themes/*.toml file (shown as "Custom" in the dialog).
    pub custom: bool,

    /// App background. `Color::Reset` keeps the terminal's own background.
    pub bg: Color,
    /// Default text.
    pub fg: Color,
    /// Secondary text (labels, hints, footer descriptions).
    pub muted: Color,
    /// Faint text, separators, and unfocused input borders.
    pub dim: Color,
    /// Focus borders, panel titles, bullets, and the filter/status chips.
    pub accent: Color,
    /// Text placed on an accent-colored background.
    pub on_accent: Color,
    /// Focused table row highlight.
    pub selection_bg: Color,
    pub selection_fg: Color,
    /// Row highlight when the table is visible but not focused.
    pub selection_inactive_bg: Color,
    /// Subtle full-row fill for repository group header rows; selection takes precedence.
    pub group_bg: Color,
    /// Hotkey labels (`<enter>`, `<ctrl+n>`, ...).
    pub key_hint: Color,
    /// Branch names and ahead/behind counts.
    pub branch: Color,
    /// Removed diff lines.
    pub diff_del: Color,
    /// Focused dialog button.
    pub button_focus_bg: Color,
    pub button_focus_fg: Color,
    /// Unfocused dialog button.
    pub button_bg: Color,
    pub button_fg: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    /// Untracked file status.
    pub untracked: Color,
    /// Added diff lines.
    pub diff_add: Color,
    /// Diff hunk headers (`@@ … @@`).
    pub diff_hunk: Color,
}

impl Theme {
    /// Secondary-text style (previous `soft_dim_style`).
    pub fn soft_dim(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Hotkey label style.
    pub fn key_style(&self) -> Style {
        Style::default()
            .fg(self.key_hint)
            .add_modifier(Modifier::BOLD)
    }

    /// Base fill style painted under the whole frame (and re-painted under modals
    /// after `Clear`, which resets cells to the terminal default).
    pub fn base_style(&self) -> Style {
        Style::default().bg(self.bg).fg(self.fg)
    }

    /// RGB the background fades toward for pulse animations. `Color::Reset`
    /// backgrounds fall back on the dark flag (terminal default assumption).
    pub fn bg_rgb(&self) -> (u8, u8, u8) {
        match self.bg {
            Color::Reset => {
                if self.dark {
                    (0, 0, 0)
                } else {
                    (255, 255, 255)
                }
            }
            c => color_rgb(c),
        }
    }
}

/// Approximate RGB for named ANSI colors (used by pulse fading only).
pub fn color_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::DarkGray => (100, 100, 100),
        Color::Gray => (170, 170, 170),
        Color::White => (235, 235, 235),
        Color::Cyan => (0, 190, 190),
        Color::Red => (220, 70, 70),
        Color::Green => (80, 200, 120),
        Color::Yellow => (220, 200, 90),
        Color::Blue => (70, 110, 220),
        Color::Magenta => (200, 90, 200),
        _ => (170, 170, 170),
    }
}

/// Black or white, whichever contrasts more against the given background color
/// (used for text on severity-colored fills, which vary per theme).
pub fn contrast_fg(bg: Color) -> Color {
    let (r, g, b) = color_rgb(bg);
    let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if luma > 140.0 {
        Color::Black
    } else {
        Color::White
    }
}

mod builtin;

pub use builtin::builtin_themes;
use builtin::rgb;

/// Parses a color value from a theme file: `#RRGGBB` / `RRGGBB` hex, ANSI names,
/// or `default`/`reset` for the terminal's own color.
pub fn parse_color(value: &str) -> Option<Color> {
    let v = value.trim().to_ascii_lowercase();
    match v.as_str() {
        "default" | "reset" | "none" => return Some(Color::Reset),
        "black" => return Some(Color::Black),
        "red" => return Some(Color::Red),
        "green" => return Some(Color::Green),
        "yellow" => return Some(Color::Yellow),
        "blue" => return Some(Color::Blue),
        "magenta" => return Some(Color::Magenta),
        "cyan" => return Some(Color::Cyan),
        "gray" | "grey" => return Some(Color::Gray),
        "darkgray" | "darkgrey" => return Some(Color::DarkGray),
        "white" => return Some(Color::White),
        "lightred" => return Some(Color::LightRed),
        "lightgreen" => return Some(Color::LightGreen),
        "lightyellow" => return Some(Color::LightYellow),
        "lightblue" => return Some(Color::LightBlue),
        "lightmagenta" => return Some(Color::LightMagenta),
        "lightcyan" => return Some(Color::LightCyan),
        _ => {}
    }
    let hex = v.strip_prefix('#').unwrap_or(&v);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(rgb(n))
}

/// Custom theme file schema (`~/.config/ular-git/themes/<key>.toml`). Every color is
/// optional; missing roles inherit from `base` (a built-in key, default "nord").
#[derive(Debug, Default, Deserialize)]
struct ThemeFile {
    name: Option<String>,
    dark: Option<bool>,
    base: Option<String>,
    #[serde(default)]
    colors: std::collections::HashMap<String, String>,
}

/// Builds a custom theme from file content. `key` is the file stem.
/// Unknown role names and unparsable colors are ignored (inherited from base).
fn theme_from_file(key: &str, data: &str) -> Option<Theme> {
    let file: ThemeFile = toml::from_str(data).ok()?;
    let base_key = file.base.as_deref().unwrap_or(DEFAULT_THEME_KEY);
    let mut theme = builtin_themes()
        .into_iter()
        .find(|t| t.key == base_key)
        .unwrap_or_else(|| builtin_themes().remove(0));
    theme.key = key.to_string();
    theme.name = file.name.unwrap_or_else(|| key.to_string());
    theme.custom = true;
    if let Some(dark) = file.dark {
        theme.dark = dark;
    }
    for (role, value) in &file.colors {
        let Some(color) = parse_color(value) else {
            continue;
        };
        let slot = match role.as_str() {
            "bg" => &mut theme.bg,
            "fg" => &mut theme.fg,
            "muted" => &mut theme.muted,
            "dim" => &mut theme.dim,
            "accent" => &mut theme.accent,
            "on_accent" => &mut theme.on_accent,
            "selection_bg" => &mut theme.selection_bg,
            "selection_fg" => &mut theme.selection_fg,
            "selection_inactive_bg" => &mut theme.selection_inactive_bg,
            "group_bg" => &mut theme.group_bg,
            "key_hint" => &mut theme.key_hint,
            "branch" => &mut theme.branch,
            "diff_del" => &mut theme.diff_del,
            "button_focus_bg" => &mut theme.button_focus_bg,
            "button_focus_fg" => &mut theme.button_focus_fg,
            "button_bg" => &mut theme.button_bg,
            "button_fg" => &mut theme.button_fg,
            "success" => &mut theme.success,
            "warning" => &mut theme.warning,
            "error" => &mut theme.error,
            "untracked" => &mut theme.untracked,
            "diff_add" => &mut theme.diff_add,
            "diff_hunk" => &mut theme.diff_hunk,
            _ => continue,
        };
        *slot = color;
    }
    Some(theme)
}

/// Custom theme directory: `~/.config/ular-git/themes`.
pub fn themes_dir() -> PathBuf {
    crate::config::config_base_dir().join("themes")
}

/// Loads custom themes from a directory, sorted by display name.
fn load_custom_themes_from(dir: &std::path::Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut themes: Vec<Theme> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                return None;
            }
            let key = path.file_stem()?.to_str()?.to_string();
            let data = std::fs::read_to_string(&path).ok()?;
            theme_from_file(&key, &data)
        })
        .collect();
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    themes
}

/// Full theme list for the dialog: built-ins (dark then light) followed by
/// custom themes. Unit tests skip disk access for determinism.
pub fn all_themes() -> Vec<Theme> {
    let mut themes = builtin_themes();
    if !cfg!(test) {
        themes.extend(load_custom_themes_from(&themes_dir()));
    }
    themes.sort_by(|a, b| {
        let a_is_cvd = a.name.starts_with("(CVD) ");
        let b_is_cvd = b.name.starts_with("(CVD) ");
        match (a_is_cvd, b_is_cvd) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    themes
}

/// Resolves a theme by key, falling back to the default built-in.
pub fn resolve(key: &str) -> Theme {
    all_themes()
        .into_iter()
        .find(|t| t.key == key)
        .unwrap_or_else(default_theme)
}

pub fn default_theme() -> Theme {
    builtin_themes()
        .into_iter()
        .find(|t| t.key == DEFAULT_THEME_KEY)
        .expect("default theme exists")
}

/// Selection persistence: `~/.config/ular-git/theme.json` (`{"name": "<key>"}`).
fn pref_path() -> PathBuf {
    crate::config::config_base_dir().join("theme.json")
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct ThemePref {
    name: String,
}

/// Loads the persisted selection and resolves it (default theme on first run,
/// unknown key, or in unit tests).
pub fn current() -> Theme {
    if cfg!(test) {
        return default_theme();
    }
    let key = std::fs::read_to_string(pref_path())
        .ok()
        .and_then(|data| serde_json::from_str::<ThemePref>(&data).ok())
        .map(|p| p.name);
    match key {
        Some(key) => resolve(&key),
        None => default_theme(),
    }
}

/// Persists the selected theme key (best-effort; no-op in unit tests).
pub fn save_selected(key: &str) {
    if cfg!(test) {
        return;
    }
    let path = pref_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(data) = serde_json::to_string_pretty(&ThemePref {
        name: key.to_string(),
    }) {
        let _ = std::fs::write(path, data);
    }
}

#[cfg(test)]
mod tests;
