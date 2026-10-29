//! Throwaway repositories for tests that run the real `git` binary.

use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct TestRepo {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
}

/// Runs git in `dir` with a fixed identity, panicking on failure.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

impl TestRepo {
    /// A repository with one commit containing `a.txt`.
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).unwrap().join("repo");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        // Local identity so commits made through `crate::git::ops` succeed
        // without a global config.
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["config", "user.email", "t@t"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        // The developer's global hooks (commit message linters) must not
        // judge test commits.
        git(&root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        git(&root, &["add", "a.txt"]);
        git(&root, &["commit", "-q", "-m", "init"]);
        TestRepo { _dir: dir, root }
    }

    pub(crate) fn write(&self, rel: &str, text: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    pub(crate) fn git(&self, args: &[&str]) -> String {
        git(&self.root, args)
    }
}
