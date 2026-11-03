use super::*;
use crate::ui::test_support::{ch, ctrl};
use crate::ui::UiMode;

#[test]
fn quitting_ignores_new_input_and_waits_only_for_accepted_writes() {
    let mut app = App::with_store(Config::default(), Default::default(), None);
    let repo = crate::git::test_repo::TestRepo::new();
    app.open_add_repo();
    app.type_text(&repo.root.to_string_lossy());
    app.press(&[
        crate::ui::test_support::key(crossterm::event::KeyCode::Tab),
        crate::ui::test_support::key(crossterm::event::KeyCode::Tab),
        crate::ui::test_support::key(crossterm::event::KeyCode::Enter),
    ]);
    dispatch(&mut app, Event::Key(ch('q')));
    dispatch(&mut app, Event::Key(ch('q')));
    assert!(app.should_quit);
    assert!(app.store_pending());
    assert_eq!(app.notice.as_ref().unwrap().text, "Saving repository list…");
    dispatch(&mut app, Event::Key(ctrl('n')));
    dispatch(&mut app, Event::Key(ch('!')));
    dispatch(&mut app, Event::FocusGained);
    assert_eq!(app.mode, UiMode::Normal);
    assert!(app.group_name.is_none());
    assert!(app.terminal_request.is_none());
    app.wait_background(10);
    assert!(!app.store_pending());
}
