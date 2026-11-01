//! Diff pane rendering tests: line numbers, file and commit headers, hunk
//! gaps, and message wrapping.

use crate::git::test_repo::TestRepo;
use crate::ui::render::pane_layout;
use crate::ui::test_support::*;
use crate::ui::Pane;
use ratatui::layout::Rect;

#[test]
fn only_code_lines_carry_the_line_number_gutter() {
    let a = TestRepo::new();
    a.write("a.txt", "one\n2\nthree\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let rows = screen(&app, 150, 30);
    let diff_x = pane_layout(Rect::new(0, 5, 150, 24), app.focus, 3, u16::MAX)[1]
        .1
        .x as usize
        + 2;
    let starts = |needle: &str| {
        let row = rows.iter().find(|r| r.contains(needle)).unwrap();
        row[..row.find(needle).unwrap()].chars().count()
    };
    // The section title sits on a border-to-border line.
    let section = rows
        .iter()
        .find(|r| r.contains(" Unstaged changes ──"))
        .unwrap();
    assert!(
        section.contains("├────") && section.trim_end().ends_with('┤'),
        "{section}"
    );
    // Hunk headers are not shown; the first hunk follows the title after
    // one blank row.
    assert!(!shows(&rows, "@@"));
    let at = rows
        .iter()
        .position(|r| r.contains(" Unstaged changes "))
        .unwrap();
    assert!(
        rows[at + 2].contains("│ 1 1  one"),
        "{:?}",
        &rows[at..at + 3]
    );
    // git's redundant file header lines are not shown.
    assert!(!shows(&rows, "diff --git") && !shows(&rows, "index ") && !shows(&rows, "+++ b/"));
    // Code lines start after the number gutter, sized to one-digit numbers.
    assert_eq!(starts("+2"), diff_x + 4);
}

#[test]
fn a_file_diff_starts_with_its_wrapped_path_and_counts() {
    let a = TestRepo::new();
    let path = "src/main/java/kr/example/service/inspection/InspectionService.java";
    a.write(path, "one\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Diff;
    app.wait_background(10);
    let rows = screen(&app, 60, 20);
    assert!(shows(&rows, "━ InspectionService.java ━"), "{rows:#?}");
    // The first body row; broken after a `/`, the file name whole below.
    assert!(
        rows[6].contains("┃ src/main/java/kr/example/service/inspection/  "),
        "{rows:#?}"
    );
    assert!(rows[7].contains("┃ InspectionService.java  "), "{rows:#?}");
    assert!(rows[8].contains("┃ +1  "), "{rows:#?}");
    // One blank row above the section title.
    assert_eq!(
        rows[9].trim_matches(|c| c == '┃' || c == ' '),
        "",
        "{rows:#?}"
    );
    assert!(rows[10].contains(" Untracked file "), "{rows:#?}");
}

#[test]
fn unpushed_commits_show_date_and_subject_and_their_hash_titles_the_diff() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(parent, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    a.write("a.txt", "next\n");
    a.git(&["commit", "-qam", "second commit\n\nbody text"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.history.meta = crate::ui::history_view::CommitMeta::None;
    app.wait_background(10);
    let commit = app
        .current_tree()
        .unwrap()
        .status
        .as_ref()
        .unwrap()
        .unpushed[0]
        .clone();
    let rows = screen(&app, 160, 30);
    assert!(shows(&rows, " ↑ second commit"), "{rows:#?}");
    assert!(
        shows(&rows, &format!(" {} +1 -3 ", commit.hash)),
        "{rows:#?}"
    );
    let byline = rows
        .iter()
        .position(|r| r.contains("  t <t@t>"))
        .expect("byline");
    assert!(
        rows[byline + 1].contains("second commit"),
        "message right below"
    );
    assert!(!shows(&rows, "CommitDate"));
    assert!(shows(&rows, "body text"));
}

#[test]
fn commit_messages_wrap_even_with_line_wrap_off() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(parent, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    let long = "word ".repeat(40);
    a.write("a.txt", &format!("{long}\n"));
    a.git(&["commit", "-qam", &format!("subject\n\n{long}END")]);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    assert!(!app.diff.wrap);
    let rows = screen(&app, 160, 40);
    assert!(
        shows(&rows, "END"),
        "the message's tail is wrapped into view"
    );
    // A code line still ends at the pane edge.
    assert!(
        rows.iter().any(|r| r.contains("+word") && r.contains('…')),
        "{rows:#?}"
    );
}

#[test]
fn commit_files_get_a_full_width_title_line() {
    let a = TestRepo::new();
    let parent = a.root.parent().unwrap();
    let remote = parent.join("remote.git");
    crate::git::test_repo::git(parent, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    a.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    a.git(&["push", "-q", "-u", "origin", "main"]);
    a.write("a.txt", "next\n");
    a.git(&["commit", "-qam", "second"]);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let rows = screen(&app, 160, 30);
    let title = rows
        .iter()
        .find(|r| r.contains(" a.txt ──"))
        .expect("file title, right-aligned and joined to the right border");
    assert!(
        title.contains("├────────────────────"),
        "runs from the left border: {title}"
    );
    assert!(!shows(&rows, "diff --git"));
    // git's `---` is a border-to-border rule carrying the stat summary, with
    // one blank row above and below every rule and title.
    let rule = rows
        .iter()
        .position(|r| r.contains("├──") && r.contains(" 1 file changed, ") && r.contains("──┤"))
        .expect("rule with the summary");
    assert!(!rows.iter().any(|r| r.contains(" --- ")));
    let title = rows.iter().position(|r| r.contains(" a.txt ──")).unwrap();
    assert_eq!(
        title,
        rule + 4,
        "rule, blank, one stat row, blank, then the file title"
    );
    // Blank inside the diff pane (from its left border on).
    let left = rows[rule].chars().position(|c| c == '├').unwrap();
    let blank = |y: usize| rows[y].chars().skip(left + 1).all(|c| c == ' ' || c == '│');
    assert!(blank(rule - 1) && blank(title + 1), "{rows:#?}");
    // Labels are centered: the dashes on both sides differ by at most one.
    let line = &rows[title];
    let start = line.find('├').unwrap();
    let seg: String = line[start..]
        .chars()
        .skip(1)
        .take_while(|c| *c != '┤')
        .collect();
    let left = seg.chars().take_while(|c| *c == '─').count();
    let right = seg.chars().rev().take_while(|c| *c == '─').count();
    assert!(left.abs_diff(right) <= 1, "{line}");
    assert!(
        rows[title - 2].contains("a.txt | "),
        "{:?}",
        &rows[rule..=title]
    );
}

#[test]
fn a_gap_line_separates_hunks() {
    let a = TestRepo::new();
    let body: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    a.write("a.txt", &body);
    a.git(&["commit", "-qam", "thirty lines"]);
    let changed = body
        .replace("line 2\n", "line two\n")
        .replace("line 28\n", "line 28b\n");
    a.write("a.txt", &changed);
    let mut app = scanned_app(store_of(&[&a.root]));
    // Files focus shows Files + Diff.
    app.focus = Pane::Files;
    app.wait_background(10);
    let rows = screen(&app, 160, 40);
    let gaps: Vec<&String> = rows.iter().filter(|r| r.contains("│  ⋯")).collect();
    assert_eq!(gaps.len(), 1, "one gap between the two hunks: {rows:#?}");
    assert!(!shows(&rows, "@@"));
}

#[test]
fn a_wide_diff_pane_shows_old_and_new_side_by_side() {
    let a = TestRepo::new();
    a.write("x.txt", "a\nb\nc\n");
    a.git(&["add", "x.txt"]);
    a.git(&["commit", "-qm", "x"]);
    a.write("x.txt", "a\nB\nc\nd\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.diff.split = true;
    app.focus = Pane::Diff;
    app.pane_count = 1;
    app.wait_background(10);
    // `1` gives the diff pane the screen to itself.
    let rows = screen(&app, 140, 24);
    let row = rows
        .iter()
        .find(|r| r.contains("-b"))
        .expect("the removed line");
    assert!(row.contains("+B"), "paired with the added line: {row}");
    // The right side's number starts one space after the divider.
    assert!(row.contains("│ 2 +B"), "{row}");
    assert!(row.find("-b").unwrap() < row.find("+B").unwrap());
    // `d` was added with nothing removed: alone on the right.
    let d = rows.iter().find(|r| r.contains("+d")).unwrap();
    assert!(!d.contains('-'), "{d}");
    // A narrower pane, or `s`, falls back to the unified layout.
    let narrow = screen(&app, 110, 24);
    assert!(!narrow.iter().any(|r| r.contains("-b") && r.contains("+B")));
    app.press(&[crate::ui::test_support::ch('s')]);
    assert!(!app.diff.split);
    let rows = screen(&app, 140, 24);
    assert!(!rows.iter().any(|r| r.contains("-b") && r.contains("+B")));
}

#[test]
fn the_changed_words_of_a_paired_line_are_reversed() {
    let a = TestRepo::new();
    a.write("x.txt", "let a = foo(1, 2);\nkeep\n");
    a.git(&["add", "x.txt"]);
    a.git(&["commit", "-qm", "x"]);
    a.write("x.txt", "let b = foo(1, 3);\nkeep\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Diff;
    app.wait_background(10);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
    terminal.draw(|f| crate::ui::render::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
    let reversed = |needle: &str, offset: usize| {
        let y = (0..24).find(|&y| row(y).contains(needle)).unwrap();
        let x = row(y).find(needle).unwrap() + offset;
        // `find` gives a byte index; these rows are ASCII up to it.
        let x = row(y)[..x].chars().count() as u16;
        buf[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    };
    // `-let a = foo(1, 2);`: `a` and `2` differ, `let` and `foo` do not.
    assert!(reversed("-let a", 5), "a");
    assert!(!reversed("-let a", 1), "let");
    assert!(reversed("+let b", 5), "b");
    assert!(!reversed("+let b", 9), "foo");
}

/// A diff of 60 numbered lines, each long enough to wrap several times.
fn long_lines_app() -> (TestRepo, crate::ui::App) {
    let a = TestRepo::new();
    let text: String = (0..60)
        .map(|i| format!("line{i:02} {}\n", "word ".repeat(60)))
        .collect();
    a.write("a.txt", &text);
    let mut app = scanned_app(store_of(&[&a.root]));
    app.focus = Pane::Diff;
    app.wait_background(10);
    (a, app)
}

#[test]
fn wrapping_on_and_off_keeps_the_top_line() {
    let (_a, mut app) = long_lines_app();
    app.diff.split = false;
    let top = |app: &crate::ui::App| {
        // The first row below the header and the pane's top border.
        let row = screen(app, 150, 30)[6].clone();
        row.split_whitespace()
            .find_map(|w| w.find("line").map(|at| w[at..].to_string()))
    };
    top(&app);
    app.diff.scroll_by(20);
    let before = top(&app).expect("a numbered line on top");
    for _ in 0..2 {
        app.press(&[ch('.')]);
        assert_eq!(
            top(&app).as_deref(),
            Some(before.as_str()),
            "wrap {}",
            app.diff.wrap
        );
    }
    // Scrolling after a toggle moves from the kept line, not from the top.
    app.press(&[ch('.')]);
    top(&app);
    app.diff.scroll_by(1);
    assert_ne!(top(&app).as_deref(), Some(before.as_str()));
}

#[test]
fn side_by_side_and_pane_width_changes_keep_the_top_line() {
    let (_a, mut app) = long_lines_app();
    app.diff.split = true;
    app.diff.wrap = true;
    // Files focus at 150 columns: a narrow unified diff beside the files.
    app.focus = Pane::Files;
    let line = |app: &crate::ui::App| {
        screen(app, 150, 30);
        app.diff.anchor.get().unwrap().line
    };
    line(&app);
    app.diff.scroll_by(40);
    let before = line(&app);
    assert!(before > 0);
    // Diff focus: the diff alone at full width, side by side.
    app.focus = Pane::Diff;
    assert_eq!(line(&app), before, "wider and side by side");
    app.press(&[ch('s')]);
    assert_eq!(line(&app), before, "unified at full width");
    app.focus = Pane::Files;
    assert_eq!(line(&app), before, "narrow again");
}

#[test]
fn the_diff_keeps_its_colors_when_another_pane_is_focused() {
    let a = TestRepo::new();
    a.write("a.txt", "fresh line\n");
    let mut app = scanned_app(store_of(&[&a.root]));
    app.wait_background(10);
    let added_fg = |app: &crate::ui::App| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 30)).unwrap();
        terminal.draw(|f| crate::ui::render::draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let row = |y: u16| (0..150).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let y = (0..30).find(|&y| row(y).contains("fresh line")).unwrap();
        let x = (0..150).find(|&x| buf[(x, y)].symbol() == "f").unwrap();
        buf[(x, y)].fg
    };
    app.focus = Pane::Diff;
    assert_eq!(added_fg(&app), app.theme.diff_add);
    app.focus = Pane::Files;
    assert_eq!(
        added_fg(&app),
        app.theme.diff_add,
        "content is never dimmed"
    );
}
