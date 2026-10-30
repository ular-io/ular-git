//! User configuration (`~/.config/ular-git/config.toml`) and the app's file paths.
//!
//! The user-edited file is TOML so the seeded template can carry comments;
//! app-owned state (repository list, theme choice, histories) is JSON, and the
//! status cache lives in the OS cache directory.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// App name used for the config and cache directories.
pub const APP_NAME: &str = "ular-git";

/// Effective settings: built-in defaults overlaid with config.toml.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Seconds between automatic rescans of every repository (0 disables).
    pub refresh_interval_secs: u64,
    /// Parallel `git` processes used by a scan.
    pub scan_jobs: usize,
    /// Upper bound on diff lines loaded for one file (the rest is cut with a note).
    pub max_diff_lines: usize,
    /// Editor command for Edit Config. None = `$VISUAL` / `$EDITOR` / `vi`.
    pub editor: Option<String>,
    /// Apps listed by Open With on a repository or worktree row; never empty.
    pub launchers: Vec<Launcher>,
    /// Apps listed by Open With on a changed file; never empty.
    pub file_launchers: Vec<Launcher>,
}

/// An app Open With runs in the selected working tree.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Launcher {
    pub name: String,
    /// Shell command run in the working tree; `{path}` becomes its quoted path.
    pub command: String,
    /// Started detached, the screen kept (GUI apps). Otherwise the terminal is
    /// handed over until the command exits (terminal apps).
    pub gui: bool,
}

/// Open With entry used when config.toml lists no `[[apps]]`: the folder in
/// the system file manager (macOS `open`).
fn default_launchers() -> Vec<Launcher> {
    vec![Launcher {
        name: "Open Folder".into(),
        command: "open .".into(),
        gui: true,
    }]
}

/// Open With entry used when config.toml lists no `[[file_apps]]`.
fn default_file_launchers() -> Vec<Launcher> {
    vec![Launcher {
        name: "vim".into(),
        command: "vim {path}".into(),
        gui: false,
    }]
}

impl Default for Config {
    fn default() -> Self {
        Config {
            refresh_interval_secs: 60,
            scan_jobs: 8,
            max_diff_lines: 20_000,
            editor: None,
            launchers: default_launchers(),
            file_launchers: default_file_launchers(),
        }
    }
}

/// config.toml overlay; every key is optional and unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    refresh_interval_secs: Option<u64>,
    scan_jobs: Option<usize>,
    max_diff_lines: Option<usize>,
    editor: Option<String>,
    apps: Option<Vec<FileLauncher>>,
    file_apps: Option<Vec<FileLauncher>>,
}

/// One `[[apps]]` / `[[file_apps]]` table; `name` defaults to the command.
#[derive(Debug, Deserialize)]
struct FileLauncher {
    name: Option<String>,
    command: Option<String>,
    #[serde(default)]
    gui: bool,
}

impl Config {
    /// Loads config.toml over the defaults. A missing or malformed file keeps
    /// the defaults (unit tests never read the user's file).
    pub fn load() -> Self {
        let mut cfg = Config::default();
        if cfg!(test) {
            return cfg;
        }
        if let Some(fc) = std::fs::read_to_string(config_file_path())
            .ok()
            .and_then(|data| toml::from_str::<FileConfig>(&data).ok())
        {
            cfg.apply(fc);
        }
        cfg
    }

    fn apply(&mut self, fc: FileConfig) {
        if let Some(v) = fc.refresh_interval_secs {
            self.refresh_interval_secs = v;
        }
        if let Some(v) = fc.scan_jobs {
            self.scan_jobs = v.clamp(1, 32);
        }
        if let Some(v) = fc.max_diff_lines {
            self.max_diff_lines = v.max(100);
        }
        self.editor = fc.editor.filter(|e| !e.trim().is_empty());
        self.launchers = launchers(fc.apps).unwrap_or_else(default_launchers);
        self.file_launchers = launchers(fc.file_apps).unwrap_or_else(default_file_launchers);
    }

    /// Editor command line: config `editor`, then `$VISUAL`, `$EDITOR`, `vi`.
    pub fn editor_command(&self) -> String {
        resolve_editor(
            self.editor.as_deref(),
            std::env::var("VISUAL").ok().as_deref(),
            std::env::var("EDITOR").ok().as_deref(),
        )
    }
}

/// Usable entries of an app list (a command is required); `None` when there
/// are none, so the default list applies.
fn launchers(apps: Option<Vec<FileLauncher>>) -> Option<Vec<Launcher>> {
    let list: Vec<Launcher> = apps
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| {
            let command = a.command?.trim().to_string();
            if command.is_empty() {
                return None;
            }
            let name = a
                .name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| command.clone());
            Some(Launcher {
                name,
                command,
                gui: a.gui,
            })
        })
        .collect();
    (!list.is_empty()).then_some(list)
}

fn resolve_editor(configured: Option<&str>, visual: Option<&str>, editor: Option<&str>) -> String {
    [configured, visual, editor]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|v| !v.is_empty())
        .unwrap_or("vi")
        .to_string()
}

/// `~/.config/ular-git` (XDG-style on every platform, like s7s and k9s).
pub fn config_base_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config"))
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

pub fn config_file_path() -> PathBuf {
    config_base_dir().join("config.toml")
}

/// App-owned repository list and groups.
pub fn repos_path() -> PathBuf {
    config_base_dir().join("repos.json")
}

/// Last scan results shown instantly on the next launch.
pub fn status_cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_NAME)
        .join("status.json")
}

const CONFIG_TEMPLATE: &str = "\
# ular-git configuration. Every key is optional; remove the leading `#` to override.

# Seconds between automatic rescans of every repository (0 disables).
# refresh_interval_secs = 60

# Parallel git processes used by a scan.
# scan_jobs = 8

# Upper bound on diff lines loaded for one file.
# max_diff_lines = 20000

# Editor for Edit Config. Defaults to $VISUAL, then $EDITOR, then vi.
# editor = \"code --wait\"

# Apps offered by Open With: `enter` on a repository or worktree row lists
# [[apps]], `enter` on a changed file lists [[file_apps]]. Commands run in the
# working tree; {path} is replaced with the quoted folder or file path.
# Terminal apps take over this terminal until they exit; `gui = true` apps are
# started without leaving ular-git. Without entries, folders open with
# Open Folder (`open .`) and files with vim (`vim {path}`).
# [[apps]]
# name = \"gitui\"
# command = \"gitui\"
#
# [[apps]]
# name = \"VS Code\"
# command = \"code {path}\"
# gui = true
#
# [[file_apps]]
# name = \"Zed\"
# command = \"zed {path}\"
# gui = true
";

/// Seeds a commented config.toml when none exists (or it is blank), so Edit
/// Config always opens a file that documents every option.
pub fn ensure_config_template() {
    let path = config_file_path();
    let blank = std::fs::read_to_string(&path)
        .map(|s| s.trim().is_empty())
        .unwrap_or(true);
    if !blank {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, CONFIG_TEMPLATE);
}

/// `~`-prefixed display form of a path under the home directory.
pub fn collapse_home(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.to_string_lossy());
        }
    }
    path.to_string_lossy().into_owned()
}

/// Expands a leading `~` / `~/` to the home directory.
pub fn expand_home(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

/// Writes `data` to `path` through a sibling temp file and rename, so a crash
/// mid-write never leaves a truncated file behind.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_applies_partial_keys_and_clamps() {
        let mut cfg = Config::default();
        cfg.apply(toml::from_str("scan_jobs = 99\neditor = \"  \"").unwrap());
        assert_eq!(cfg.scan_jobs, 32);
        assert_eq!(cfg.refresh_interval_secs, 60);
        assert_eq!(cfg.editor, None);
    }

    #[test]
    fn template_is_fully_commented_and_parses_to_defaults() {
        let fc: FileConfig = toml::from_str(CONFIG_TEMPLATE).unwrap();
        let mut cfg = Config::default();
        cfg.apply(fc);
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn apps_replace_the_default_and_skip_entries_without_a_command() {
        let mut cfg = Config::default();
        cfg.apply(
            toml::from_str(
                "[[apps]]\nname = \"gitui\"\ncommand = \"gitui\"\n\
                 [[apps]]\ncommand = \"code {path}\"\ngui = true\n\
                 [[apps]]\nname = \"broken\"\ncommand = \" \"\n",
            )
            .unwrap(),
        );
        assert_eq!(
            cfg.launchers,
            [
                Launcher {
                    name: "gitui".into(),
                    command: "gitui".into(),
                    gui: false,
                },
                Launcher {
                    name: "code {path}".into(),
                    command: "code {path}".into(),
                    gui: true,
                },
            ]
        );
        assert_eq!(cfg.file_launchers, default_file_launchers());
        let mut cfg = Config::default();
        cfg.apply(toml::from_str("apps = []").unwrap());
        assert_eq!(cfg.launchers, default_launchers());
    }

    #[test]
    fn editor_precedence_falls_back_to_vi() {
        assert_eq!(resolve_editor(Some("hx"), Some("code"), None), "hx");
        assert_eq!(resolve_editor(Some(" "), None, Some("nano")), "nano");
        assert_eq!(resolve_editor(None, None, None), "vi");
    }

    #[test]
    fn expand_home_handles_tilde_forms() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/a/b"), home.join("a/b"));
        assert_eq!(expand_home("/tmp/x"), PathBuf::from("/tmp/x"));
        assert_eq!(collapse_home(&home.join("x")), "~/x");
    }
}
