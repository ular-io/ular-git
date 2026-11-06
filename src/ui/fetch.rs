//! Fetch: `ctrl+f` fetches the selected repository (a worktree row fetches
//! its repository, whose remote-tracking refs every worktree shares) or every
//! repository of the selected group; `ctrl+u` fetches every repository after
//! starting the rescan.
//!
//! - At most [`FETCH_JOBS`] fetches run at once, each on its own thread, so
//!   scans never wait behind the network.
//! - A queued or running fetch shows `fetch…` on its repository row (and on a
//!   folded group header) and holds push / pull in the repository and its
//!   worktrees: they all write the same remote-tracking refs. A repository
//!   with a push or pull running is not fetched.
//! - A fetched repository is rescanned (its worktrees follow), which updates
//!   `↓N`.
//! - One repository reports in the footer, or in an error dialog when it
//!   fails. Several (`ctrl+u`, a group) form a round summed up in one footer
//!   line when the last one finishes.

use super::overlays::message::MessageKind;
use super::sync::folder_name;
use super::tree::Row;
use super::{App, Level};
use crate::git::remote;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Fetches running at once.
const FETCH_JOBS: usize = 8;
/// Failed repositories named in a round's summary.
const NAMED_FAILURES: usize = 3;

#[derive(Debug, Default)]
pub struct FetchState {
    /// Repositories with a fetch queued or running.
    active: HashSet<PathBuf>,
    /// Waiting for a free job, with whether each belongs to the round.
    queue: VecDeque<(PathBuf, bool)>,
    running: usize,
    round: Option<Round>,
}

/// Several repositories fetched together, reported as one line.
#[derive(Debug, Default)]
struct Round {
    pending: usize,
    fetched: usize,
    failed: Vec<String>,
}

fn repositories(n: usize) -> &'static str {
    if n == 1 {
        "repository"
    } else {
        "repositories"
    }
}

impl App {
    /// Whether `repo` has a fetch queued or running.
    pub(crate) fn fetching(&self, repo: &Path) -> bool {
        self.fetch.active.contains(repo)
    }

    /// `ctrl+u`: reload the list and rescan, then fetch every repository.
    pub(crate) fn refresh_and_fetch(&mut self) {
        self.refresh_all(false);
        let paths: Vec<PathBuf> = self.store.repos.iter().map(|r| r.path.clone()).collect();
        let total = paths.len();
        let n = self.queue_round(paths);
        let text = if n == total {
            format!("Rescanning and fetching {n} {}", repositories(n))
        } else {
            format!("Rescanning {total} {}, fetching {n}", repositories(total))
        };
        self.notify(Level::Info, text);
    }

    /// `ctrl+f`: the selected repository, or every repository of the
    /// selected group.
    pub(crate) fn start_fetch(&mut self) {
        let group = match self.current_row() {
            None => {
                self.notify(Level::Info, "Select a repository to fetch");
                return;
            }
            Some(Row::Group { name, .. }) => name.clone(),
            Some(_) => {
                let Some(i) = self.current_repo_index() else {
                    return;
                };
                let repo = self.store.repos[i].path.clone();
                if let Some(op) = self.repo_busy(&repo) {
                    self.notify(Level::Info, format!("A {op} is still running here"));
                    return;
                }
                self.notify(Level::Info, format!("{}: fetch…", folder_name(&repo)));
                self.enqueue_fetch(repo, false);
                self.pump_fetches();
                return;
            }
        };
        let label = group.clone().unwrap_or_else(|| "Ungrouped".into());
        let paths: Vec<PathBuf> = self
            .store
            .repos
            .iter()
            .filter(|r| r.group == group)
            .map(|r| r.path.clone())
            .collect();
        if paths.is_empty() {
            self.notify(Level::Info, format!("{label} has no repositories to fetch"));
            return;
        }
        let text = match self.queue_round(paths) {
            0 => format!("Every repository in {label} is busy"),
            n => format!("Fetching {n} {} in {label}", repositories(n)),
        };
        self.notify(Level::Info, text);
    }

    /// The push, pull, or fetch running in `repo` or one of its worktrees.
    fn repo_busy(&self, repo: &Path) -> Option<&'static str> {
        if self.fetching(repo) {
            return Some("fetch");
        }
        let worktrees = self
            .statuses
            .get(repo)
            .and_then(|t| t.status.as_ref())
            .map(|s| s.worktrees.as_slice())
            .unwrap_or_default();
        std::iter::once(repo)
            .chain(worktrees.iter().map(|w| w.path.as_path()))
            .find_map(|p| self.remote_ops.get(p))
            .map(|op| op.label())
    }

    /// Queues every repository of `paths` that is not busy into the round.
    /// Returns how many were queued.
    fn queue_round(&mut self, paths: Vec<PathBuf>) -> usize {
        let mut n = 0;
        for path in paths {
            if self.repo_busy(&path).is_none() {
                self.enqueue_fetch(path, true);
                n += 1;
            }
        }
        if n > 0 {
            self.fetch.round.get_or_insert_with(Round::default).pending += n;
            self.pump_fetches();
        }
        n
    }

    fn enqueue_fetch(&mut self, repo: PathBuf, in_round: bool) {
        self.fetch.active.insert(repo.clone());
        self.fetch.queue.push_back((repo, in_round));
    }

    /// Starts queued fetches while fewer than [`FETCH_JOBS`] run.
    fn pump_fetches(&mut self) {
        while self.fetch.running < FETCH_JOBS {
            let Some((repo, in_round)) = self.fetch.queue.pop_front() else {
                return;
            };
            self.fetch.running += 1;
            self.bg.spawn(move || {
                let result = remote::fetch(&repo).map_err(|e| e.to_string());
                Box::new(move |app: &mut App| app.finish_fetch(repo, in_round, result))
            });
        }
    }

    fn finish_fetch(
        &mut self,
        repo: PathBuf,
        in_round: bool,
        result: Result<Option<usize>, String>,
    ) {
        self.fetch.running -= 1;
        self.fetch.active.remove(&repo);
        let name = folder_name(&repo);
        if in_round {
            if let Some(round) = self.fetch.round.as_mut() {
                round.pending -= 1;
                match result {
                    Ok(Some(_)) => round.fetched += 1,
                    Ok(None) => {}
                    Err(_) => round.failed.push(name),
                }
                if round.pending == 0 {
                    if let Some(round) = self.fetch.round.take() {
                        self.report_round(round);
                    }
                }
            }
        } else {
            match result {
                Ok(None) => self.notify(Level::Info, format!("{name}: no remote to fetch from")),
                Ok(Some(0)) => self.notify(Level::Ok, format!("{name}: fetched, nothing new")),
                Ok(Some(n)) => self.notify(
                    Level::Ok,
                    format!(
                        "{name}: fetched, {n} ref{} updated",
                        if n == 1 { "" } else { "s" }
                    ),
                ),
                Err(err) => self.show_message(" Fetch Failed ", vec![err], MessageKind::Error),
            }
        }
        if self.store.contains(&repo) {
            self.request_scan(repo, false);
        }
        self.pump_fetches();
    }

    fn report_round(&mut self, round: Round) {
        let failed = round.failed.len();
        if failed == 0 {
            if round.fetched == 0 {
                self.notify(Level::Info, "Nothing fetched: no repository has a remote");
            } else {
                let n = round.fetched;
                self.notify(Level::Ok, format!("Fetched {n} {}", repositories(n)));
            }
            return;
        }
        let mut names = round.failed[..failed.min(NAMED_FAILURES)].join(", ");
        if failed > NAMED_FAILURES {
            names.push_str(&format!(" and {} more", failed - NAMED_FAILURES));
        }
        let tried = failed + round.fetched;
        self.notify(
            Level::Warn,
            format!(
                "Fetch failed in {failed} of {tried} {}: {names}",
                repositories(tried)
            ),
        );
    }
}
