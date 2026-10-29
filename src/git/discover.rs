//! Turning a folder the user typed into repository roots.

use super::run_ok;
use std::path::{Path, PathBuf};

/// Folder depth searched below a non-repository folder.
pub const MAX_DEPTH: usize = 3;

/// Folders never searched: dependency and build output trees hold no
/// repositories the user wants listed and can be enormous.
const SKIP_DIRS: &[&str] = &["node_modules", "target", "build", "dist", "vendor", "Pods"];

/// Repository roots for `path`:
/// 1. `path` is a working tree root → its main repository (a linked worktree
///    resolves to the repository that owns it);
/// 2. otherwise repositories found below `path` (up to [`MAX_DEPTH`]);
/// 3. otherwise the repository `path` sits inside.
pub fn repos_for(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !path.is_dir() {
        anyhow::bail!("not a folder: {}", path.display());
    }
    if path.join(".git").exists() {
        return Ok(vec![main_repo(path)?]);
    }
    let mut found = Vec::new();
    find_below(path, 0, &mut found);
    if !found.is_empty() {
        let mut roots: Vec<PathBuf> = found.iter().filter_map(|p| main_repo(p).ok()).collect();
        roots.sort();
        roots.dedup();
        return Ok(roots);
    }
    main_repo(path)
        .map(|root| vec![root])
        .map_err(|_| anyhow::anyhow!("no git repository in {}", path.display()))
}

/// Main worktree root of the repository containing `dir`.
pub fn main_repo(dir: &Path) -> anyhow::Result<PathBuf> {
    let out = run_ok(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ],
    )?;
    let text = String::from_utf8_lossy(&out);
    let mut lines = text.lines();
    let top = PathBuf::from(lines.next().unwrap_or_default());
    let common = PathBuf::from(lines.next().unwrap_or_default());
    // A linked worktree shares `<main>/.git`; resolve to `<main>`.
    let root = match (common.file_name(), common.parent()) {
        (Some(name), Some(parent)) if name == ".git" && parent != top => parent.to_path_buf(),
        _ => top,
    };
    if root.as_os_str().is_empty() {
        anyhow::bail!("not a git repository");
    }
    Ok(std::fs::canonicalize(&root).unwrap_or(root))
}

fn find_below(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !name.starts_with('.') && !SKIP_DIRS.contains(&name)
        })
        .collect();
    children.sort();
    for child in children {
        // `.git` as a file is a linked worktree or submodule: it belongs to
        // another repository, which `main_repo` resolves to.
        if child.join(".git").exists() {
            found.push(child);
        } else {
            find_below(&child, depth + 1, found);
        }
    }
}
