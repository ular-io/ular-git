//! ular-git: a k9s-style TUI that shows many git repositories at a glance — their
//! status badges, linked worktrees, changed files, and diffs.
//!
//! The library crate holds every module so they are testable without starting
//! the binary; `main.rs` only calls [`run`].

pub mod cache;
pub mod config;
pub mod git;
mod runtime;
pub mod store;
pub mod theme;
pub mod ui;

pub use runtime::run;
