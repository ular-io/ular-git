//! CLI parsing, the event loop, the terminal lifecycle, and the shell handover
//! for `!` commands and Edit Config.

use crate::config::Config;
use crate::ui::{self, App, TerminalRequest};
use anyhow::Result;
use clap::Parser;
use crossterm::{
    cursor::{MoveTo, Show},
    event::{
        self, DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
        Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{self, Stdout, Write},
    sync::atomic::{AtomicU8, Ordering},
    time::{Duration, Instant},
};

type Tui = Terminal<CrosstermBackend<Stdout>>;

mod terminal;

use terminal::*;

/// ular-git — watch many git repositories, their worktrees, changes, and diffs.
#[derive(Parser)]
#[command(
    name = "ular-git",
    version,
    about = "ular-git — many git repositories at a glance (TUI)",
    after_help = "\
FILES:  ~/.config/ular-git/repos.json   repository list and groups (managed by ular-git)
        ~/.config/ular-git/config.toml  settings (open with `:` → Edit Config)
        ~/.config/ular-git/themes/*.toml custom themes"
)]
struct Cli {
    /// Add this folder (a repository, or every repository below it) to the list and exit.
    #[arg(long, value_name = "DIR")]
    add: Option<std::path::PathBuf>,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(dir) = cli.add {
        return add_from_cli(&dir);
    }
    install_panic_hook();
    let mut app = App::new(Config::load());
    let mut session = TerminalSession::enter()?;
    let run_result = run_loop(&mut session, &mut app);
    let restore_result = session.suspend();
    app.shutdown();
    match (run_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) => Err(e),
        (Ok(()), Err(e)) => Err(e.into()),
        (Err(e), Err(r)) => Err(e.context(format!("terminal restore also failed: {r}"))),
    }
}

/// `ular-git --add <dir>`: registers repositories without opening a dialog.
fn add_from_cli(dir: &std::path::Path) -> Result<()> {
    let path = crate::config::repos_path();
    let mut store = crate::store::RepoStore::load(&path)?;
    let roots = crate::git::discover::repos_for(&std::fs::canonicalize(dir)?)?;
    let added = roots
        .into_iter()
        .filter(|r| store.add_repo(r.clone(), None))
        .count();
    store.save(&path)?;
    println!("ular-git: added {added} repositories");
    Ok(())
}

/// Waits for the first event, drains whatever else is queued, then draws
/// once. Background results and timers wake the loop without input.
fn run_loop(session: &mut TerminalSession, app: &mut App) -> Result<()> {
    loop {
        app.poll_background();
        app.tick(Instant::now());
        session.terminal_mut().draw(|f| ui::render::draw(f, app))?;
        loop {
            let timeout = if app.background_in_flight() {
                // Short enough for the scan spinner.
                Duration::from_millis(100)
            } else {
                app.next_deadline()
                    .map(|d| d.saturating_duration_since(Instant::now()))
                    .unwrap_or(Duration::from_secs(1))
                    .clamp(Duration::from_millis(20), Duration::from_secs(1))
            };
            if event::poll(timeout)? {
                dispatch(app, event::read()?);
                break;
            }
            let changed = app.poll_background() | app.tick(Instant::now());
            if changed || app.background_in_flight() {
                break;
            }
        }
        while !app.should_quit
            && app.terminal_request.is_none()
            && event::poll(Duration::from_millis(0))?
        {
            dispatch(app, event::read()?);
        }
        if let Some(req) = app.terminal_request.take() {
            handover(session, app, req)?;
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

fn dispatch(app: &mut App, ev: Event) {
    match ev {
        Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
        // One paste is one text insertion, never synthetic keys: a pasted
        // newline must not submit a dialog or run a command.
        Event::Paste(text) => app.on_paste(&text),
        Event::FocusGained => app.refresh_all(true),
        _ => {}
    }
}

/// Runs a shell command with the terminal released, then takes it back.
fn handover(session: &mut TerminalSession, app: &mut App, req: TerminalRequest) -> Result<()> {
    session.suspend()?;
    let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
    println!("▶ {}\n  {}\n", req.cwd.display(), req.command);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let status = std::process::Command::new(&shell)
        .arg("-c")
        .arg(&req.command)
        .current_dir(&req.cwd)
        .status();
    let failed = match &status {
        Ok(s) if s.success() => None,
        Ok(s) => Some(format!("exited with code {}", s.code().unwrap_or(-1))),
        Err(e) => Some(format!("could not start: {e}")),
    };
    if let Some(why) = &failed {
        eprintln!("\n⚠ Command {why}.");
    }
    if req.pause || failed.is_some() {
        pause_before_return();
    }
    session.resume()?;
    drain_pending_input();
    // The command may have changed config.toml or any repository.
    app.cfg = Config::load();
    app.refresh_all(false);
    let what = req.name.as_deref().unwrap_or(&req.command);
    let label = match failed {
        Some(why) => (ui::Level::Warn, format!("{what} {why}")),
        None => (ui::Level::Ok, format!("Returned from: {what}")),
    };
    app.notify(label.0, label.1);
    Ok(())
}

fn pause_before_return() {
    eprint!("\nPress any key to return to ular-git…");
    let _ = io::stderr().flush();
    if let Some(_raw) = RawModeGuard::enter() {
        drain_pending_input();
        loop {
            match event::read() {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => break,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
}

/// Drops keys typed while the child ran so they do not act on the TUI.
fn drain_pending_input() {
    while matches!(event::poll(Duration::from_millis(0)), Ok(true)) {
        if event::read().is_err() {
            break;
        }
    }
}
