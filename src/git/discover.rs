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
///
/// Several roots come back by folder name (case-insensitive), the order they
/// are appended to the repository list in.
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
        roots.sort_by_cached_key(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        });
        return Ok(roots);
    }
    main_repo(path)
        .map(|root| vec![root])
        .map_err(|_| anyhow::anyhow!("no git repository in {}", path.display()))
}

/// Main worktree root of the repository containing `dir`.
pub fn main_repo(dir: &Path) -> anyhow::Result<PathBuf> {
    roots(dir).map(|(main, _)| main)
}

/// `ular-git <dir>`: the repository to show and the working tree to select.
/// `dir` may be a working tree root, a folder inside one, or a linked
/// worktree (selected under the repository that owns it). A folder outside
/// any repository is an error that counts the repositories below it, since
/// `--add` is what lists those.
pub fn open_target(dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    if !dir.is_dir() {
        anyhow::bail!("not a folder: {}", dir.display());
    }
    if let Ok(found) = roots(dir) {
        return Ok(found);
    }
    let mut below = Vec::new();
    find_below(dir, 0, &mut below);
    match below.len() {
        0 => anyhow::bail!("not a git repository: {}", dir.display()),
        n => anyhow::bail!(
            "{} is not inside a git repository; it holds {n} below it. \
             To list them all: ular-git --add {}",
            dir.display(),
            dir.display()
        ),
    }
}

/// Main worktree root and the working tree root of `dir`, both canonical.
fn roots(dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
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
        _ => top.clone(),
    };
    if root.as_os_str().is_empty() {
        anyhow::bail!("not a git repository");
    }
    let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
    Ok((canonical(root), canonical(top)))
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

#[cfg(test)]
mod tests {
    use super::open_target;
    use crate::git::test_repo::TestRepo;

    #[test]
    fn open_target_resolves_roots_subfolders_and_linked_worktrees() {
        let repo = TestRepo::new();
        let root = repo.root.clone();
        assert_eq!(open_target(&root).unwrap(), (root.clone(), root.clone()));
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        assert_eq!(open_target(&sub).unwrap(), (root.clone(), root.clone()));
        let wt = root.parent().unwrap().join("repo-feat");
        repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
        assert_eq!(open_target(&wt.join("")).unwrap(), (root.clone(), wt));
    }

    #[test]
    fn open_target_rejects_folders_outside_a_repository() {
        let repo = TestRepo::new();
        let parent = repo.root.parent().unwrap();
        let err = open_target(parent).unwrap_err().to_string();
        assert!(err.contains("holds 1 below it"), "{err}");
        assert!(err.contains("ular-git --add"), "{err}");
        let empty = tempfile::tempdir().unwrap();
        let err = open_target(empty.path()).unwrap_err().to_string();
        assert!(err.starts_with("not a git repository"), "{err}");
        let err = open_target(&parent.join("missing"))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("not a folder"), "{err}");
    }
}
