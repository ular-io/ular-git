//! Footer result line with an expiry. After it expires the footer goes back
//! to the pane shortcuts.

use super::App;
use std::time::{Duration, Instant};

/// How long a result stays in the footer.
pub const NOTICE_TTL: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Error,
}

impl Level {
    /// Text tag, so the level survives monochrome terminals.
    pub fn tag(self) -> &'static str {
        match self {
            Level::Info => "[INFO]",
            Level::Ok => "[OK]",
            Level::Warn => "[WARN]",
            Level::Error => "[ERROR]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: Level,
    pub text: String,
    pub until: Instant,
}

impl App {
    pub fn notify(&mut self, level: Level, text: impl Into<String>) {
        self.notice = Some(Notice {
            level,
            text: text.into(),
            until: Instant::now() + NOTICE_TTL,
        });
    }

    /// Drops an expired notice. Returns whether the footer changed.
    pub fn expire_notice(&mut self, now: Instant) -> bool {
        if self.notice.as_ref().is_some_and(|n| n.until <= now) {
            self.notice = None;
            self.quit_armed = false;
            return true;
        }
        false
    }
}
