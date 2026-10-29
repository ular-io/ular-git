//! The repository list and groups (`~/.config/ular-git/repos.json`, app-owned).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const STORE_VERSION: u32 = 1;

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

    /// Deletes a group; its repositories move to "Ungrouped". Returns how many moved.
    pub fn delete_group(&mut self, name: &str) -> usize {
        self.groups.retain(|g| g.name != name);
        let mut moved = 0;
        for repo in &mut self.repos {
            if repo.group.as_deref() == Some(name) {
                repo.group = None;
                moved += 1;
            }
        }
        moved
    }

    pub fn set_group(&mut self, path: &Path, group: Option<String>) {
        if let Some(repo) = self.repos.iter_mut().find(|r| r.path == path) {
            repo.group = group;
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
}
