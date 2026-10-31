//! One file as git stores it at a revision (`git cat-file blob <rev>:<path>`),
//! for Open With's past versions.

use super::run_ok;
use std::path::Path;

/// The raw contents of `path` (relative to the working tree root `dir`) at
/// `rev`.
pub fn read(dir: &Path, rev: &str, path: &str) -> anyhow::Result<Vec<u8>> {
    run_ok(dir, &["cat-file", "blob", &format!("{rev}:{path}")])
}

#[cfg(test)]
mod tests {
    use crate::git::test_repo::TestRepo;

    #[test]
    fn reads_a_file_at_a_revision_and_fails_where_it_is_missing() {
        let a = TestRepo::new();
        a.write("a.txt", "new\n");
        a.git(&["commit", "-qam", "edit"]);
        assert_eq!(
            super::read(&a.root, "HEAD^", "a.txt").unwrap(),
            b"one\ntwo\nthree\n"
        );
        assert_eq!(super::read(&a.root, "HEAD", "a.txt").unwrap(), b"new\n");
        assert!(super::read(&a.root, "HEAD", "missing.txt").is_err());
    }
}
