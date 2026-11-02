//! The repository list and groups (`~/.config/ular-git/repos.json`, app-owned).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 2: `repos` order is the display order inside each group (version 1 and
/// earlier showed repositories by folder name; loading one sorts it once).
const STORE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: PathBuf,
    /// Group name; `None` lists the repository under "Ungrouped".
    #[serde(default)]
    pub group: Option<String>,
}

impl RepoEntry {
    /// Folder name shown in the repository list.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoStore {
    #[serde(default)]
    pub version: u32,
    /// Display order of the groups.
    #[serde(default)]
    pub groups: Vec<Group>,
    /// Display order of the repositories inside each group.
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
    #[serde(default)]
    pub ungrouped_collapsed: bool,
}

impl Default for RepoStore {
    fn default() -> Self {
        RepoStore {
            version: STORE_VERSION,
            groups: Vec::new(),
            repos: Vec::new(),
            ungrouped_collapsed: false,
        }
    }
}

impl RepoStore {
    /// Missing file → empty store. A malformed file is an error so the caller
    /// can refuse to overwrite it.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(data) => {
                let mut store: RepoStore = serde_json::from_str(&data)
                    .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
                if store.version < 2 {
                    // Keep the by-name order those versions showed.
                    store.repos.sort_by_cached_key(|r| r.name().to_lowercase());
                }
                store.normalize();
                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RepoStore::default()),
            Err(e) => Err(anyhow::anyhow!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let data = serde_json::to_vec_pretty(self)?;
        crate::config::write_atomic(path, &data)?;
        Ok(())
    }

    /// Drops duplicate repositories and references to groups that do not exist
    /// (a hand-edited file stays usable).
    fn normalize(&mut self) {
        self.version = STORE_VERSION;
        let mut seen = std::collections::HashSet::new();
        self.groups
            .retain(|g| !g.name.trim().is_empty() && seen.insert(g.name.clone()));
        let mut paths = std::collections::HashSet::new();
        self.repos.retain(|r| paths.insert(r.path.clone()));
        let names: Vec<String> = self.groups.iter().map(|g| g.name.clone()).collect();
        for repo in &mut self.repos {
            if repo.group.as_ref().is_some_and(|g| !names.contains(g)) {
                repo.group = None;
            }
        }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.repos.iter().any(|r| r.path == path)
    }

    /// Adds a repository unless it is already listed; returns whether it was added.
    pub fn add_repo(&mut self, path: PathBuf, group: Option<String>) -> bool {
        if self.contains(&path) {
            return false;
        }
        self.repos.push(RepoEntry { path, group });
        true
    }

    pub fn remove_repo(&mut self, path: &Path) -> bool {
        let before = self.repos.len();
        self.repos.retain(|r| r.path != path);
        before != self.repos.len()
    }

    pub fn group_exists(&self, name: &str) -> bool {
        self.groups.iter().any(|g| g.name == name)
    }

    /// Validates a group name for creation or rename (`current` = the name
    /// being renamed, which may be kept).
    pub fn check_group_name(&self, name: &str, current: Option<&str>) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a group name".to_string());
        }
        if Some(name) != current && self.group_exists(name) {
            return Err(format!("Group \"{name}\" already exists"));
        }
        Ok(())
    }

    pub fn add_group(&mut self, name: &str) -> Result<(), String> {
        self.check_group_name(name, None)?;
        self.groups.push(Group {
            name: name.trim().to_string(),
            collapsed: false,
        });
        Ok(())
    }

    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<(), String> {
        self.check_group_name(new, Some(old))?;
        let new = new.trim().to_string();
        let Some(group) = self.groups.iter_mut().find(|g| g.name == old) else {
            return Err(format!("Group \"{old}\" no longer exists"));
        };
        group.name = new.clone();
        for repo in &mut self.repos {
            if repo.group.as_deref() == Some(old) {
                repo.group = Some(new.clone());
            }
        }
        Ok(())
    }

    /// Deletes a group; its repositories move to the end of "Ungrouped" in
    /// their order. Returns how many moved.
    pub fn delete_group(&mut self, name: &str) -> usize {
        self.groups.retain(|g| g.name != name);
        let (mut moved, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.repos)
            .into_iter()
            .partition(|r| r.group.as_deref() == Some(name));
        for repo in &mut moved {
            repo.group = None;
        }
        let count = moved.len();
        self.repos = kept;
        self.repos.extend(moved);
        count
    }

    /// Files a repository under `group`, at the end of it. Nothing changes
    /// when it is already there.
    pub fn set_group(&mut self, path: &Path, group: Option<String>) {
        let Some(i) = self.repos.iter().position(|r| r.path == path) else {
            return;
        };
        if self.repos[i].group == group {
            return;
        }
        let mut repo = self.repos.remove(i);
        repo.group = group;
        self.repos.push(repo);
    }

    /// Moves a repository one place up (`-1`) or down (`1`) among the
    /// repositories of its group.
    pub fn move_repo(&mut self, path: &Path, delta: isize) -> bool {
        let Some(i) = self.repos.iter().position(|r| r.path == path) else {
            return false;
        };
        let group = self.repos[i].group.clone();
        let same = |r: &RepoEntry| r.group == group;
        let j = if delta < 0 {
            self.repos[..i].iter().rposition(same)
        } else {
            self.repos[i + 1..].iter().position(same).map(|k| i + 1 + k)
        };
        match j {
            Some(j) => {
                self.repos.swap(i, j);
                true
            }
            None => false,
        }
    }

    /// Flips a group's collapsed flag (`None` = Ungrouped).
    pub fn toggle_collapsed(&mut self, group: Option<&str>) {
        match group {
            None => self.ungrouped_collapsed = !self.ungrouped_collapsed,
            Some(name) => {
                if let Some(g) = self.groups.iter_mut().find(|g| g.name == name) {
                    g.collapsed = !g.collapsed;
                }
            }
        }
    }

    pub fn is_collapsed(&self, group: Option<&str>) -> bool {
        match group {
            None => self.ungrouped_collapsed,
            Some(name) => self.groups.iter().any(|g| g.name == name && g.collapsed),
        }
    }

    /// Moves a group one place up (`-1`) or down (`1`) in display order.
    pub fn move_group(&mut self, name: &str, delta: isize) -> bool {
        let Some(i) = self.groups.iter().position(|g| g.name == name) else {
            return false;
        };
        let j = i as isize + delta;
        if j < 0 || j as usize >= self.groups.len() {
            return false;
        }
        self.groups.swap(i, j as usize);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_and_delete_keep_repositories_consistent() {
        let mut s = RepoStore::default();
        s.add_group("work").unwrap();
        assert!(s.add_group(" work ").is_err());
        assert!(s.add_repo("/a".into(), Some("work".into())));
        assert!(!s.add_repo("/a".into(), None));
        s.rename_group("work", "job").unwrap();
        assert_eq!(s.repos[0].group.as_deref(), Some("job"));
        assert_eq!(s.delete_group("job"), 1);
        assert_eq!(s.repos[0].group, None);
    }

    #[test]
    fn normalize_drops_dangling_groups_and_duplicates() {
        let json = r#"{"groups":[{"name":"a"},{"name":"a"},{"name":" "}],
            "repos":[{"path":"/x","group":"gone"},{"path":"/x"},{"path":"/y","group":"a"}]}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repos.json");
        std::fs::write(&path, json).unwrap();
        let s = RepoStore::load(&path).unwrap();
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.repos.len(), 2);
        assert_eq!(s.repos[0].group, None);
        assert_eq!(s.repos[1].group.as_deref(), Some("a"));
    }

    #[test]
    fn missing_file_is_empty_and_malformed_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repos.json");
        assert_eq!(RepoStore::load(&path).unwrap(), RepoStore::default());
        std::fs::write(&path, "{").unwrap();
        assert!(RepoStore::load(&path).is_err());
    }

    #[test]
    fn save_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/repos.json");
        let mut s = RepoStore::default();
        s.add_group("g").unwrap();
        s.add_repo("/r".into(), Some("g".into()));
        s.toggle_collapsed(Some("g"));
        s.save(&path).unwrap();
        assert_eq!(RepoStore::load(&path).unwrap(), s);
    }

    #[test]
    fn groups_move_within_bounds() {
        let mut s = RepoStore::default();
        s.add_group("a").unwrap();
        s.add_group("b").unwrap();
        assert!(!s.move_group("a", -1));
        assert!(s.move_group("a", 1));
        assert_eq!(s.groups[0].name, "b");
    }

    fn order(s: &RepoStore) -> Vec<String> {
        s.repos.iter().map(|r| r.name()).collect()
    }

    #[test]
    fn repositories_move_within_their_group() {
        let mut s = RepoStore::default();
        s.add_group("g").unwrap();
        s.add_repo("/a".into(), None);
        s.add_repo("/x".into(), Some("g".into()));
        s.add_repo("/b".into(), None);
        assert!(!s.move_repo(Path::new("/a"), -1));
        assert!(!s.move_repo(Path::new("/x"), 1));
        assert!(s.move_repo(Path::new("/b"), -1));
        assert_eq!(order(&s), ["b", "x", "a"]);
        assert!(!s.move_repo(Path::new("/a"), 1));
    }

    #[test]
    fn regrouped_repositories_go_to_the_end_of_their_group() {
        let mut s = RepoStore::default();
        s.add_group("g").unwrap();
        s.add_repo("/x".into(), Some("g".into()));
        s.add_repo("/a".into(), None);
        s.add_repo("/b".into(), None);
        s.set_group(Path::new("/x"), Some("g".into()));
        assert_eq!(order(&s), ["x", "a", "b"]);
        s.set_group(Path::new("/a"), Some("g".into()));
        assert_eq!(order(&s), ["x", "b", "a"]);
        assert_eq!(s.delete_group("g"), 2);
        assert_eq!(order(&s), ["b", "x", "a"]);
    }

    #[test]
    fn version_1_stores_load_in_name_order() {
        let json = r#"{"version":1,"repos":[{"path":"/z/Beta"},{"path":"/a/alpha"}]}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repos.json");
        std::fs::write(&path, json).unwrap();
        let mut s = RepoStore::load(&path).unwrap();
        assert_eq!(order(&s), ["alpha", "Beta"]);
        assert!(s.move_repo(Path::new("/z/Beta"), -1));
        s.save(&path).unwrap();
        assert_eq!(order(&RepoStore::load(&path).unwrap()), ["Beta", "alpha"]);
    }
}
