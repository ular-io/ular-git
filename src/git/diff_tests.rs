//! Diff parsing tests.

use super::*;

fn lines(raw: &str) -> Vec<String> {
    raw.lines().map(str::to_string).collect()
}

#[test]
fn numbers_and_classifies_a_hunk() {
    let mut doc = DiffDoc::default();
    let raw = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n\
@@ -10,3 +10,3 @@ fn x\n ctx\n--- removed dashes\n+added\n tail";
    push_parsed(&mut doc, &lines(raw), false);
    let kinds: Vec<_> = doc.lines.iter().map(|l| l.kind).collect();
    use LineKind::*;
    // diff --git / index / --- / +++ are dropped from a single-file diff.
    // The first hunk's `@@` line is dropped too.
    assert_eq!(kinds, [Context, Del, Add, Context]);
    assert_eq!((doc.lines[0].old, doc.lines[0].new), (Some(10), Some(10)));
    assert_eq!((doc.lines[1].old, doc.lines[1].new), (Some(11), None));
    assert_eq!((doc.lines[2].old, doc.lines[2].new), (None, Some(11)));
    assert_eq!((doc.lines[3].old, doc.lines[3].new), (Some(12), Some(12)));
    assert_eq!((doc.added, doc.removed), (1, 1));
}

#[test]
fn show_output_marks_the_commit_header() {
    let mut doc = DiffDoc::default();
    let raw =
            "2026-01-02 03:04  a <a@x>\nsubject\n\nbody\n---\nmore body\n\n---\n x | 2 +-\ndiff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b";
    push_parsed(&mut doc, &lines(raw), true);
    let kinds: Vec<(LineKind, &str)> = doc.lines[..8]
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    use LineKind::*;
    // Blank message lines are dropped; a `---` inside the message stays
    // message, and git's last one becomes the rule.
    assert_eq!(
        kinds,
        [
            (Byline, "2026-01-02 03:04  a <a@x>"),
            (Subject, "subject"),
            (Header, "body"),
            (Header, "---"),
            (Header, "more body"),
            (Rule, "---"),
            (Stat, " x | 2 +-"),
            (File, "x"),
        ]
    );
    assert_eq!(doc.lines[8].kind, LineKind::Del);
}

#[test]
fn the_stat_summary_moves_onto_the_rule() {
    let mut doc = DiffDoc::default();
    let raw = "d  a <a@x>\nsubject\n---\n x | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n\ndiff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b";
    push_parsed(&mut doc, &lines(raw), true);
    let kinds: Vec<(LineKind, &str)> = doc.lines[2..5]
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    use LineKind::*;
    assert_eq!(
        kinds,
        [
            (Rule, "1 file changed, 1 insertion(+), 1 deletion(-)"),
            (Stat, " x | 2 +-"),
            (File, "x"),
        ]
    );
}

#[test]
fn combined_conflict_diff_uses_two_prefix_columns() {
    let mut doc = DiffDoc::default();
    let raw = "diff --cc x\n@@@ -1,1 -1,1 +1,3 @@@\n++<<<<<<< HEAD\n +ours\n- theirs";
    push_parsed(&mut doc, &lines(raw), false);
    use LineKind::*;
    let kinds: Vec<_> = doc.lines.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, [Add, Add, Del]);
    assert!(doc.lines.iter().all(|l| l.old.is_none() && l.new.is_none()));
}

#[test]
fn clean_line_expands_tabs_and_strips_controls() {
    assert_eq!(clean_line(b"a\tb\x1b[31m\r\n"), "a   b[31m");
    assert_eq!(clean_line(b"\t\tx"), "        x");
}

#[test]
fn hunk_header_defaults_missing_counts_to_one() {
    assert_eq!(parse_hunk("@@ -3 +4,2 @@"), Some((3, 1, 4, 2)));
    assert_eq!(parse_hunk("@@ -0,0 +1 @@"), Some((0, 0, 1, 1)));
}

#[test]
fn commit_file_titles_replace_git_headers_and_keep_informative_lines() {
    let mut doc = DiffDoc::default();
    let raw = "d  a <a@x>\nsubject\n---\n x | 1 +\n\
diff --git a/old name.rs b/new name.rs\nsimilarity index 90%\nrename from old name.rs\nrename to new name.rs\nindex 1..2 100644\n--- a/old name.rs\n+++ b/new name.rs\n@@ -1 +1 @@\n-a\n+b\n\
diff --git a/img.png b/img.png\nnew file mode 100644\nindex 0..3\nBinary files /dev/null and b/img.png differ";
    push_parsed(&mut doc, &lines(raw), true);
    let tail: Vec<(LineKind, &str)> = doc.lines[4..]
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    use LineKind::*;
    assert_eq!(
        tail,
        [
            (File, "old name.rs → new name.rs"),
            (Meta, "similarity index 90%"),
            (Del, "-a"),
            (Add, "+b"),
            (File, "img.png"),
            (Meta, "new file mode 100644"),
            (Meta, "Binary files /dev/null and b/img.png differ"),
        ]
    );
}

#[test]
fn header_paths_handle_spaces_and_combined_diffs() {
    assert_eq!(header_path("diff --git a/a b/c.rs b/a b/c.rs"), "a b/c.rs");
    assert_eq!(header_path("diff --cc src/x.rs"), "src/x.rs");
    assert_eq!(header_path("diff --git a/x b/y"), "y");
}

#[test]
fn a_gap_marks_skipped_lines_between_hunks_only() {
    let mut doc = DiffDoc::default();
    let raw = "@@ -1,2 +1,2 @@ fn a\n x\n-y\n+z\n@@ -40,1 +40,2 @@ fn b\n w\n+v";
    push_parsed(&mut doc, &lines(raw), false);
    let kinds: Vec<_> = doc.lines.iter().map(|l| l.kind).collect();
    use LineKind::*;
    assert_eq!(kinds, [Context, Del, Add, Gap, Context, Add]);
    assert_eq!(doc.lines[4].old, Some(40));
    assert!(!doc.lines.iter().any(|l| l.text.contains("@@")));
}

#[test]
fn the_worktree_shows_its_stat_then_every_files_diff() {
    let a = crate::git::test_repo::TestRepo::new();
    a.write("a.txt", "one\nTWO\nthree\n");
    a.git(&["add", "a.txt"]);
    a.write("a.txt", "one\nTWO\nthree\nfour\n");
    a.write("new.txt", "n\n");
    let doc = load(&a.root, &DiffSource::WorkTree, &LoadOpts::lines(1000)).unwrap();
    let rows: Vec<(LineKind, &str)> = doc
        .lines
        .iter()
        .filter(|l| !l.text.is_empty())
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    use LineKind::*;
    assert_eq!(
        rows[0],
        (Rule, "2 files changed, 3 insertions(+), 1 deletion(-)")
    );
    assert_eq!(rows[1].0, Stat);
    let titles: Vec<&str> = rows.iter().filter(|r| r.0 == File).map(|r| r.1).collect();
    assert_eq!(titles, ["a.txt", "new.txt"], "tracked, then untracked");
    // Staged and unstaged together, against HEAD.
    assert!(rows.contains(&(Add, "+TWO")) && rows.contains(&(Add, "+four")));
    assert!(rows.contains(&(Add, "+n")));
    assert_eq!((doc.added, doc.removed), (3, 1), "the stat's counts");
    // The line limit cuts the diffs, not the counts.
    let cut = load(&a.root, &DiffSource::WorkTree, &LoadOpts::lines(6)).unwrap();
    assert!(cut.truncated);
    assert_eq!((cut.added, cut.removed), (3, 1));
}

#[test]
fn a_file_over_the_size_limit_shows_as_binary_unread() {
    let a = crate::git::test_repo::TestRepo::new();
    let big: String = (0..500).map(|i| format!("line {i}\n")).collect();
    a.write("big.txt", &big);
    a.write("small.txt", "s\n");
    a.git(&["add", "."]);
    a.git(&["commit", "-qm", "big"]);
    a.write("big.txt", &format!("{big}more\n"));
    a.write("new.txt", &big);
    let head = crate::git::Commit {
        hash: "HEAD".into(),
        short: "HEAD".into(),
        subject: "big".into(),
    };
    let opts = LoadOpts {
        big_file: 1000,
        ..LoadOpts::lines(1000)
    };
    let adds = |doc: &DiffDoc, text: &str| doc.lines.iter().any(|l| l.text == text);

    let commit = load(&a.root, &DiffSource::Commit(head.clone()), &opts).unwrap();
    assert!(!adds(&commit, "+line 1"), "the big file is not diffed");
    assert!(adds(&commit, "+s"), "a small file still is");
    let full = load(&a.root, &DiffSource::Commit(head), &LoadOpts::lines(1000)).unwrap();
    assert!(adds(&full, "+line 1"), "no limit, no change");

    // The uncommitted changes: tracked and untracked files alike.
    let wt = load(&a.root, &DiffSource::WorkTree, &opts).unwrap();
    assert!(
        !adds(&wt, "+more") && !adds(&wt, "+line 1"),
        "{:#?}",
        wt.lines
    );
    let files = crate::git::commit_files::load_worktree(&a.root, 1000).unwrap();
    let big = files.iter().find(|f| f.path == "big.txt").unwrap();
    assert_eq!(big.counts, None, "counted as binary");
}

#[test]
fn a_cancelled_load_fails_without_running_git() {
    let a = crate::git::test_repo::TestRepo::new();
    a.write("a.txt", "one\n");
    let opts = LoadOpts::lines(1000);
    opts.cancel.cancel();
    let err = load(&a.root, &DiffSource::WorkTree, &opts).unwrap_err();
    assert_eq!(err.to_string(), crate::git::cancel::CANCELLED);
}
