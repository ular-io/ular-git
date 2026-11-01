//! End-to-end checks against the real `git` binary.

use super::diff::{self, DiffSource, LineKind};
use super::ops::{self, CommitScope};
use super::remote::{self, NewUpstream};
use super::test_repo::TestRepo;
use super::{discover, scan, EntryKind};

#[test]
fn scan_reports_staged_unstaged_and_untracked_files() {
    let repo = TestRepo::new();
    repo.write("a.txt", "one\n2\nthree\n");
    repo.write("b.txt", "b\n");
    repo.git(&["add", "b.txt"]);
    repo.write("dir/c.txt", "c\n");
    let s = scan(&repo.root, true).unwrap();
    assert_eq!(s.branch.head.as_deref(), Some("main"));
    let codes: Vec<(String, String)> = s.files.iter().map(|f| (f.path.clone(), f.code())).collect();
    assert_eq!(
        codes,
        [
            ("a.txt".to_string(), " M".to_string()),
            ("b.txt".to_string(), "A ".to_string()),
            ("dir/c.txt".to_string(), "??".to_string()),
        ]
    );
    assert!(s.worktrees.is_empty());
}

#[test]
fn file_diff_has_sections_and_line_numbers() {
    let repo = TestRepo::new();
    repo.write("a.txt", "one\n2\nthree\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "one\n2\n3\n");
    let s = scan(&repo.root, false).unwrap();
    let entry = s.files[0].clone();
    assert_eq!(entry.code(), "MM");
    let doc = diff::load(
        &repo.root,
        &DiffSource::File(entry),
        &diff::LoadOpts::lines(1000),
    )
    .unwrap();
    let sections: Vec<&str> = doc
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Section)
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(sections, ["Staged changes", "Unstaged changes"]);
    let added: Vec<(&str, Option<u32>)> = doc
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Add)
        .map(|l| (l.text.as_str(), l.new))
        .collect();
    assert_eq!(added, [("+2", Some(2)), ("+3", Some(3))]);
    // The first section leads the diff (the title names the file); the
    // counts cover both sections.
    assert_eq!(doc.lines[0].kind, LineKind::Section);
    assert_eq!((doc.added, doc.removed), (2, 2));
}

#[test]
fn a_staged_rename_and_a_new_file_count_their_lines() {
    let repo = TestRepo::new();
    repo.write("docs/old.md", "one\ntwo\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-q", "-m", "docs: add"]);
    repo.git(&["mv", "docs/old.md", "docs/new.md"]);
    repo.write("n.txt", "x\ny\n");
    let s = scan(&repo.root, false).unwrap();
    let file = |p: &str| s.files.iter().find(|f| f.path == p).unwrap().clone();
    let doc = diff::load(
        &repo.root,
        &DiffSource::File(file("docs/new.md")),
        &diff::LoadOpts::lines(1000),
    )
    .unwrap();
    // A pure rename changes no line; the section leads the diff.
    assert_eq!(doc.lines[0].kind, LineKind::Section);
    assert_eq!((doc.added, doc.removed), (0, 0));
    let doc = diff::load(
        &repo.root,
        &DiffSource::File(file("n.txt")),
        &diff::LoadOpts::lines(1000),
    )
    .unwrap();
    assert_eq!((doc.added, doc.removed), (2, 0));
    assert_eq!(diff::counts_label(2, 0), "+2");
    assert_eq!(diff::counts_label(0, 3), "-3");
}

#[test]
fn untracked_file_diff_shows_every_line_added_and_respects_the_limit() {
    let repo = TestRepo::new();
    let body: String = (0..50).map(|i| format!("line {i}\n")).collect();
    repo.write("new.txt", &body);
    let s = scan(&repo.root, false).unwrap();
    assert_eq!(s.files[0].kind, EntryKind::Untracked);
    let doc = diff::load(
        &repo.root,
        &DiffSource::File(s.files[0].clone()),
        &diff::LoadOpts::lines(1000),
    )
    .unwrap();
    assert_eq!(doc.added, 50);
    assert!(!doc.truncated);
    let cut = diff::load(
        &repo.root,
        &DiffSource::File(s.files[0].clone()),
        &diff::LoadOpts::lines(20),
    )
    .unwrap();
    assert!(cut.truncated);
    assert!(cut.lines.len() <= 21);
}

#[test]
fn commit_all_includes_untracked_files_and_staged_leaves_the_rest() {
    let repo = TestRepo::new();
    repo.write("a.txt", "changed\n");
    repo.write("b.txt", "b\n");
    repo.git(&["add", "b.txt"]);
    let summary = ops::commit(&repo.root, "add b", CommitScope::Staged).unwrap();
    assert!(summary.ends_with("add b"), "{summary}");
    let s = scan(&repo.root, false).unwrap();
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.files[0].path, "a.txt");
    repo.write("c.txt", "c\n");
    ops::commit(&repo.root, "rest", CommitScope::All).unwrap();
    assert!(scan(&repo.root, false).unwrap().is_clean());
}

#[test]
fn empty_commit_message_is_rejected_before_git_runs() {
    let repo = TestRepo::new();
    repo.write("a.txt", "x\n");
    assert!(ops::commit(&repo.root, "  ", CommitScope::All).is_err());
    assert_eq!(scan(&repo.root, false).unwrap().counts().staged, 0);
}

#[test]
fn worktrees_are_listed_resolved_to_the_main_repo_and_removable() {
    let repo = TestRepo::new();
    let wt = repo.root.parent().unwrap().join("repo-feat");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let s = scan(&repo.root, true).unwrap();
    assert_eq!(s.worktrees.len(), 1);
    assert_eq!(s.worktrees[0].branch.as_deref(), Some("feat"));
    assert_eq!(discover::main_repo(&wt).unwrap(), repo.root);
    assert_eq!(
        discover::repos_for(&wt).unwrap(),
        std::slice::from_ref(&repo.root)
    );

    std::fs::write(wt.join("dirty.txt"), "x").unwrap();
    assert!(ops::remove_worktree(&repo.root, &wt, false).is_err());
    ops::remove_worktree(&repo.root, &wt, true).unwrap();
    assert!(!wt.exists());
    assert!(scan(&repo.root, true).unwrap().worktrees.is_empty());
}

#[test]
fn a_parent_folder_resolves_to_the_repositories_below_it() {
    let repo = TestRepo::new();
    let parent = repo.root.parent().unwrap();
    assert_eq!(
        discover::repos_for(parent).unwrap(),
        std::slice::from_ref(&repo.root)
    );
    let sub = repo.root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(
        discover::repos_for(&sub).unwrap(),
        std::slice::from_ref(&repo.root)
    );
}

#[test]
fn unpushed_commits_are_listed_against_the_upstream() {
    let repo = TestRepo::new();
    let remote = repo.root.parent().unwrap().join("remote.git");
    super::test_repo::git(
        repo.root.parent().unwrap(),
        &["init", "-q", "--bare", remote.to_str().unwrap()],
    );
    repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    repo.write("a.txt", "next\n");
    repo.git(&["commit", "-qam", "second"]);
    let s = scan(&repo.root, false).unwrap();
    assert_eq!(s.branch.ahead, 1);
    assert_eq!(s.unpushed.len(), 1);
    assert_eq!(s.unpushed[0].subject, "second");
    let doc = diff::load(
        &repo.root,
        &DiffSource::Commit(s.unpushed[0].clone()),
        &diff::LoadOpts::lines(1000),
    )
    .unwrap();
    assert_eq!(doc.lines[0].kind, LineKind::Byline);
    let byline = &doc.lines[0].text;
    assert!(
        byline.ends_with("  t <t@t>") && byline.as_bytes()[4] == b'-',
        "{byline}"
    );
    assert_eq!(
        doc.lines[1].kind,
        LineKind::Subject,
        "message right below the byline"
    );
    assert!(!doc.lines.iter().any(|l| l.text.starts_with("Commit:")));
    assert!(doc
        .lines
        .iter()
        .any(|l| l.kind == LineKind::Subject && l.text == "second"));
    assert_eq!(doc.added, 1);
}

/// `repo` with a bare remote `origin` that has `main` as its upstream.
fn with_remote(repo: &TestRepo) -> std::path::PathBuf {
    let parent = repo.root.parent().unwrap();
    let remote = parent.join("remote.git");
    super::test_repo::git(
        parent,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            remote.to_str().unwrap(),
        ],
    );
    repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    remote
}

#[test]
fn push_sends_unpushed_commits_to_the_upstream() {
    let repo = TestRepo::new();
    with_remote(&repo);
    repo.write("a.txt", "next\n");
    repo.git(&["commit", "-qam", "second"]);
    assert_eq!(scan(&repo.root, false).unwrap().branch.ahead, 1);
    let summary = remote::push(&repo.root, None).unwrap();
    assert!(summary.contains("main → main"), "{summary}");
    assert_eq!(scan(&repo.root, false).unwrap().branch.ahead, 0);
}

#[test]
fn push_publishes_a_new_branch_and_sets_its_upstream() {
    let repo = TestRepo::new();
    with_remote(&repo);
    repo.git(&["switch", "-q", "-c", "feat/x"]);
    assert_eq!(
        remote::default_remote(&repo.root).as_deref(),
        Some("origin")
    );
    let new = NewUpstream {
        remote: "origin".into(),
        branch: "feat/x".into(),
    };
    remote::push(&repo.root, Some(&new)).unwrap();
    let s = scan(&repo.root, false).unwrap();
    assert_eq!(s.branch.upstream.as_deref(), Some("origin/feat/x"));
}

#[test]
fn pull_fast_forwards_and_refuses_a_diverged_branch() {
    let repo = TestRepo::new();
    let remote_path = with_remote(&repo);
    // A second clone pushes a commit the first does not have.
    let other = repo.root.parent().unwrap().join("other");
    super::test_repo::git(
        repo.root.parent().unwrap(),
        &[
            "clone",
            "-q",
            remote_path.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    std::fs::write(other.join("b.txt"), "b\n").unwrap();
    super::test_repo::git(&other, &["add", "b.txt"]);
    super::test_repo::git(&other, &["commit", "-qm", "from other"]);
    super::test_repo::git(&other, &["push", "-q"]);

    let summary = remote::pull(&repo.root).unwrap();
    assert!(summary.starts_with("fast-forward"), "{summary}");
    assert!(repo.root.join("b.txt").exists());
    assert_eq!(remote::pull(&repo.root).unwrap(), "already up to date");

    // Diverge: both sides commit.
    std::fs::write(other.join("c.txt"), "c\n").unwrap();
    super::test_repo::git(&other, &["add", "c.txt"]);
    super::test_repo::git(&other, &["commit", "-qm", "other again"]);
    super::test_repo::git(&other, &["push", "-q"]);
    repo.write("a.txt", "local\n");
    repo.git(&["commit", "-qam", "local"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert!(remote::pull(&repo.root).is_err());
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "nothing changes on refusal"
    );
}

#[test]
fn history_graph_follows_a_merged_branch() {
    let repo = TestRepo::new();
    repo.git(&["switch", "-q", "-c", "feat"]);
    repo.write("f.txt", "f\n");
    repo.git(&["add", "f.txt"]);
    repo.git(&["commit", "-qm", "on feat"]);
    repo.git(&["switch", "-q", "main"]);
    repo.write("m.txt", "m\n");
    repo.git(&["add", "m.txt"]);
    repo.git(&["commit", "-qm", "on main"]);
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge feat", "feat"]);
    let h = super::history::load(&repo.root, 100, false).unwrap();
    let subjects: Vec<&str> = h.rows.iter().map(|r| r.commit.subject.as_str()).collect();
    assert_eq!(subjects.len(), 4);
    assert_eq!(subjects[0], "merge feat");
    assert!(subjects.contains(&"on feat") && subjects.contains(&"init"));
    // One row per commit: the merge opens a lane, the branch point closes it.
    let graph: Vec<String> = h
        .rows
        .iter()
        .map(|r| r.cells.iter().map(|c| c.ch).collect())
        .collect();
    assert_eq!(graph[0], "●─╮", "{graph:?}");
    assert_eq!(graph[3], "●─╯", "{graph:?}");
    assert!(!h.more);
    assert!(super::history::load(&repo.root, 2, false).unwrap().more);
}

#[test]
fn an_unborn_branch_has_an_empty_history() {
    let dir = tempfile::tempdir().unwrap();
    super::test_repo::git(dir.path(), &["init", "-q", "-b", "main"]);
    let h = super::history::load(dir.path(), 100, false).unwrap();
    assert!(h.rows.is_empty() && !h.more);
}

/// `repo` with an `origin` that has `n` commits `repo` lacks (fetched).
fn behind_origin(repo: &TestRepo, n: usize) {
    with_remote(repo);
    let parent = repo.root.parent().unwrap();
    let other = parent.join("other");
    let remote = parent.join("remote.git");
    super::test_repo::git(
        parent,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    for i in 0..n {
        std::fs::write(other.join(format!("u{i}.txt")), "u\n").unwrap();
        super::test_repo::git(&other, &["add", "-A"]);
        super::test_repo::git(&other, &["commit", "-qm", &format!("upstream {i}")]);
    }
    super::test_repo::git(&other, &["push", "-q"]);
    repo.git(&["fetch", "-q"]);
}

#[test]
fn history_lists_commits_to_pull_above_head() {
    let repo = TestRepo::new();
    behind_origin(&repo, 2);
    let h = super::history::load(&repo.root, 100, false).unwrap();
    let rows: Vec<(&str, bool)> = h
        .rows
        .iter()
        .map(|r| (r.commit.subject.as_str(), r.incoming))
        .collect();
    assert_eq!(
        rows,
        [("upstream 1", true), ("upstream 0", true), ("init", false)]
    );
    assert_eq!(super::history::count(&repo.root), Some(3), "as listed");
}

#[test]
fn a_diverged_branch_shows_both_sides_meeting_at_their_base() {
    let repo = TestRepo::new();
    behind_origin(&repo, 1);
    repo.write("local.txt", "l\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "local"]);
    let h = super::history::load(&repo.root, 100, false).unwrap();
    let incoming: Vec<&str> = h
        .rows
        .iter()
        .filter(|r| r.incoming)
        .map(|r| r.commit.subject.as_str())
        .collect();
    assert_eq!(incoming, ["upstream 0"]);
    assert_eq!(
        super::history::count(&repo.root),
        Some(3),
        "both sides, once"
    );
    assert!(h
        .rows
        .iter()
        .any(|r| r.commit.subject == "local" && !r.incoming));
    // Two tips, one base: the base row closes the second lane.
    let base = h.rows.iter().find(|r| r.commit.subject == "init").unwrap();
    let cells: String = base.cells.iter().map(|c| c.ch).collect();
    assert_eq!(cells, "●─╯");
}

#[test]
fn discard_returns_each_kind_of_change_to_head() {
    let repo = TestRepo::new();
    repo.write("b.txt", "b\n");
    repo.git(&["add", "b.txt"]);
    repo.git(&["commit", "-qm", "b"]);
    repo.write("a.txt", "edited\n"); // unstaged
    repo.write("b.txt", "staged\n");
    repo.git(&["add", "b.txt"]);
    repo.write("b.txt", "staged then edited\n"); // MM
    repo.git(&["mv", "a.txt", "moved.txt"]); // rename (with the edit)
    repo.write("added.txt", "n\n");
    repo.git(&["add", "added.txt"]); // A
    repo.write("loose.txt", "u\n"); // ??
    let s = scan(&repo.root, false).unwrap();
    assert_eq!(s.files.len(), 4, "{:?}", s.files);
    for f in &s.files {
        ops::discard(&repo.root, f).unwrap();
    }
    assert!(scan(&repo.root, false).unwrap().is_clean());
    assert_eq!(
        std::fs::read_to_string(repo.root.join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.root.join("b.txt")).unwrap(),
        "b\n"
    );
    assert!(!repo.root.join("moved.txt").exists());
    assert!(!repo.root.join("added.txt").exists());
    assert!(!repo.root.join("loose.txt").exists());
}

#[test]
fn discard_refuses_a_conflict() {
    let repo = TestRepo::new();
    let conflict = super::FileEntry {
        path: "a.txt".into(),
        orig_path: None,
        x: 'U',
        y: 'U',
        kind: EntryKind::Conflict,
    };
    assert!(ops::discard(&repo.root, &conflict).is_err());
}
