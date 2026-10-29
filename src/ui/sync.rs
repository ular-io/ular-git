//! Push (`P`, after a confirmation) and pull (`p`, fast-forward only) of the
//! selected working tree. Both run in the background; the repository row
//! shows `push…` / `pull…` until they finish, and one operation per working
//! tree runs at a time.

use super::overlays::confirm::{ConfirmAction, ConfirmDialog};
use super::overlays::message::MessageKind;
use super::{App, Level, UiMode};
use crate::git::remote::{self, NewUpstream};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOp {
    Push,
    Pull,
}

impl RemoteOp {
    pub fn label(self) -> &'static str {
        match self {
            RemoteOp::Push => "push",
            RemoteOp::Pull => "pull",
        }
    }
}

/// Commits listed in the push confirmation.
const SHOWN_COMMITS: usize = 5;

fn folder_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl App {
    /// Working tree, branch name, and upstream for a remote operation, or a
    /// footer note saying why there is none.
    fn remote_target(&mut self, op: RemoteOp) -> Option<(PathBuf, String, Option<String>)> {
        let Some(target) = self.current_target() else {
            self.notify(
                Level::Info,
                format!("Select a repository to {}", op.label()),
            );
            return None;
        };
        if let Some(running) = self.remote_ops.get(&target) {
            self.notify(
                Level::Info,
                format!("A {} is still running here", running.label()),
            );
            return None;
        }
        let Some(status) = self.current_tree().and_then(|t| t.status.as_ref()) else {
            self.notify(Level::Info, "Status is still loading");
            return None;
        };
        let Some(branch) = status.branch.head.clone() else {
            self.notify(
                Level::Warn,
                format!("HEAD is detached: nothing to {}", op.label()),
            );
            return None;
        };
        Some((target, branch, status.branch.upstream.clone()))
    }

    /// `P`: confirmation listing what will be pushed. A branch without an
    /// upstream is offered to the default remote under the same name; finding
    /// that remote reads git config, so it runs in the background.
    pub(crate) fn open_push(&mut self) {
        let Some((target, branch, upstream)) = self.remote_target(RemoteOp::Push) else {
            return;
        };
        let status = self
            .current_tree()
            .and_then(|t| t.status.clone())
            .unwrap_or_default();
        if let Some(up) = upstream {
            if status.branch.ahead == 0 {
                self.notify(
                    Level::Info,
                    format!("Nothing to push: {branch} matches {up}"),
                );
                return;
            }
            let n = status.branch.ahead;
            let mut lines = vec![format!(
                "Push {n} commit{} on {branch} to {up}?",
                if n == 1 { "" } else { "s" }
            )];
            lines.push(String::new());
            for c in status.unpushed.iter().take(SHOWN_COMMITS) {
                lines.push(format!("  {} {}", c.short, c.subject));
            }
            let more = (n as usize).saturating_sub(SHOWN_COMMITS.min(status.unpushed.len()));
            if more > 0 {
                lines.push(format!("  … and {more} more"));
            }
            if status.branch.behind > 0 {
                lines.push(String::new());
                lines.push(format!(
                    "{up} has {} commits that are not here, so git will reject the push. Pull first.",
                    status.branch.behind
                ));
            }
            self.show_push_confirm(target, lines, None);
            return;
        }
        self.bg.spawn(move || {
            let remote = remote::default_remote(&target);
            Box::new(move |app: &mut App| {
                let Some(remote) = remote else {
                    app.notify(
                        Level::Warn,
                        format!("{branch} has no upstream and no default remote to push to"),
                    );
                    return;
                };
                let lines = vec![
                    format!("{branch} has no upstream."),
                    format!("Push it to {remote}/{branch} and set that as its upstream?"),
                ];
                let new = NewUpstream {
                    remote,
                    branch: branch.clone(),
                };
                // The selection may have moved while the remote was looked up.
                if app.current_target().as_deref() == Some(target.as_path())
                    && app.mode == UiMode::Normal
                {
                    app.show_push_confirm(target, lines, Some(new));
                }
            })
        });
    }

    fn show_push_confirm(&mut self, target: PathBuf, lines: Vec<String>, new: Option<NewUpstream>) {
        self.confirm = Some(ConfirmDialog {
            title: format!(" Push · {} ", folder_name(&target)),
            lines,
            ok_label: "Push".into(),
            danger: false,
            // Not destructive, and `P` `enter` should be quick.
            ok_focused: true,
            action: ConfirmAction::Push { target, new },
        });
        self.mode = UiMode::Confirm;
    }

    /// `p`: `git pull --ff-only` right away (git refuses anything that would
    /// need a merge, so there is nothing to confirm).
    pub(crate) fn start_pull(&mut self) {
        let Some((target, branch, upstream)) = self.remote_target(RemoteOp::Pull) else {
            return;
        };
        if upstream.is_none() {
            self.notify(
                Level::Warn,
                format!("{branch} has no upstream to pull from"),
            );
            return;
        }
        self.run_remote(target, RemoteOp::Pull, None);
    }

    pub(crate) fn run_remote(&mut self, target: PathBuf, op: RemoteOp, new: Option<NewUpstream>) {
        let name = folder_name(&target);
        self.remote_ops.insert(target.clone(), op);
        self.notify(Level::Info, format!("{name}: {}…", op.label()));
        self.bg.spawn(move || {
            let result = match op {
                RemoteOp::Push => remote::push(&target, new.as_ref()),
                RemoteOp::Pull => remote::pull(&target),
            }
            .map_err(|e| e.to_string());
            Box::new(move |app: &mut App| {
                app.remote_ops.remove(&target);
                match result {
                    Ok(summary) => {
                        let done = match op {
                            RemoteOp::Push => "pushed",
                            RemoteOp::Pull => "pulled",
                        };
                        app.notify(Level::Ok, format!("{name}: {done} {summary}"));
                    }
                    Err(err) => {
                        let title = match op {
                            RemoteOp::Push => " Push Failed ",
                            RemoteOp::Pull => " Pull Failed ",
                        };
                        app.show_message(title, vec![err], MessageKind::Error);
                    }
                }
                app.request_scan(target, true);
            })
        });
    }
}
