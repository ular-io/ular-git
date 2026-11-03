//! Serialize this instance's list operations off the input thread. Each write
//! reloads under the cross-process lock; watcher reloads never save anything.

use super::{App, Level};
use crate::store::{RepoStore, StoreWatcher};
use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

type Request = Box<dyn FnOnce(&mut App) + Send>;

#[derive(Default)]
pub(crate) struct StoreSync {
    watcher: Option<StoreWatcher>,
    watch_error: Option<String>,
    queue: VecDeque<Request>,
    busy: bool,
    writing: bool,
    pub(super) discoveries: usize,
    reload: bool,
    due: Option<Instant>,
    allow_create: bool,
    quit_deadline: Option<Instant>,
}

impl StoreSync {
    pub fn set_initially_loaded(&mut self, loaded: bool) {
        self.allow_create = !loaded;
    }

    pub fn new(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };
        let mut sync = Self {
            allow_create: !path.exists(),
            ..Self::default()
        };
        match StoreWatcher::new(path) {
            Ok(watcher) => sync.watcher = Some(watcher),
            Err(err) => sync.watch_error = Some(err.to_string()),
        }
        // Close the gap between the initial load and watcher registration.
        sync.reload = true;
        sync
    }
}

impl App {
    pub(crate) fn change_group_fold(&mut self, name: Option<String>, fold: Option<bool>) {
        if self.list_filtered() {
            self.notify(Level::Info, "Groups stay open while a filter is applied");
            return;
        }
        self.change_store(
            move |store| {
                store.require_group(name.as_deref())?;
                let collapsed = match &name {
                    Some(name) => {
                        &mut store
                            .groups
                            .iter_mut()
                            .find(|g| &g.name == name)
                            .unwrap()
                            .collapsed
                    }
                    None => &mut store.ungrouped_collapsed,
                };
                *collapsed = fold.unwrap_or(!*collapsed);
                Ok(())
            },
            |_, ()| {},
        );
    }

    pub(crate) fn change_store<T: Send + 'static>(
        &mut self,
        edit: impl FnOnce(&mut RepoStore) -> anyhow::Result<T> + Send + 'static,
        done: impl FnOnce(&mut App, T) + Send + 'static,
    ) {
        self.change_store_with_error(edit, done, |app, error| app.notify(Level::Error, error));
    }

    pub(crate) fn change_store_with_error<T: Send + 'static>(
        &mut self,
        edit: impl FnOnce(&mut RepoStore) -> anyhow::Result<T> + Send + 'static,
        done: impl FnOnce(&mut App, T) + Send + 'static,
        failed: impl FnOnce(&mut App, String) + Send + 'static,
    ) {
        let Some(path) = self.store_path.clone() else {
            let mut store = self.store.clone();
            match edit(&mut store) {
                Ok(value) => {
                    self.accept_store(store);
                    done(self, value);
                }
                Err(err) => failed(self, err.to_string()),
            }
            return;
        };
        self.store_sync.queue.push_back(Box::new(move |app| {
            let allow_create = app.store_sync.allow_create;
            let timeout = app
                .store_sync
                .quit_deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(5));
            app.bg.spawn(move || {
                let result = RepoStore::update_with_timeout(&path, allow_create, timeout, edit);
                Box::new(move |app| {
                    match result {
                        Ok((store, value)) => {
                            app.store_sync.allow_create = false;
                            app.accept_store(store);
                            done(app, value);
                        }
                        Err(err) => {
                            failed(app, format!("Cannot update repository list: {err:#}"));
                            app.store_sync.reload = true;
                        }
                    }
                    app.store_sync.busy = false;
                    app.store_sync.writing = false;
                    app.start_store_request();
                })
            });
        }));
        self.start_store_request();
    }

    fn start_store_request(&mut self) {
        if self.store_sync.busy {
            return;
        }
        if let Some(request) = self.store_sync.queue.pop_front() {
            self.store_sync.busy = true;
            self.store_sync.writing = true;
            request(self);
        } else if self.store_sync.reload && !self.should_quit {
            self.store_sync.reload = false;
            let Some(path) = self.store_path.clone() else {
                return;
            };
            self.store_sync.busy = true;
            let allow_missing = self.store_sync.allow_create;
            self.bg.spawn(move || {
                let result = RepoStore::read_existing(&path);
                Box::new(move |app| {
                    match result {
                        Ok(store) => {
                            app.store_sync.allow_create = false;
                            app.accept_store(store);
                        }
                        Err(err)
                            if allow_missing
                                && err.downcast_ref::<std::io::Error>().is_some_and(|err| {
                                    err.kind() == std::io::ErrorKind::NotFound
                                }) => {}
                        Err(err) => app.notify(
                            Level::Error,
                            format!("Cannot reload repository list; changes are not saved until repaired: {err:#}"),
                        ),
                    }
                    app.store_sync.busy = false;
                    app.start_store_request();
                })
            });
        }
    }

    pub(crate) fn reload_store(&mut self) {
        if self.should_quit {
            return;
        }
        self.store_sync.reload = true;
        self.store_sync.due = None;
        self.start_store_request();
    }

    pub(crate) fn poll_store(&mut self, now: Instant) -> bool {
        if self.should_quit {
            self.prepare_store_quit();
            return false;
        }
        if let Some(err) = self.store_sync.watch_error.take() {
            self.notify(
                Level::Warn,
                format!("Repository list watch failed: {err}. Use Refresh All to reload."),
            );
        }
        if let Some(watcher) = &self.store_sync.watcher {
            match watcher.changed() {
                Ok(true) => {
                    // A fixed window coalesces bursts without starving continuous writes.
                    self.store_sync
                        .due
                        .get_or_insert(now + Duration::from_millis(150));
                }
                Ok(false) => {}
                Err(err) => {
                    self.notify(Level::Warn, format!("Repository list watch failed: {err}"));
                    self.store_sync.reload = true;
                }
            }
        }
        if self.store_sync.due.is_some_and(|due| now >= due) {
            self.store_sync.due = None;
            self.store_sync.reload = true;
        }
        let pending = self.store_sync.reload;
        self.start_store_request();
        pending
    }

    pub(crate) fn store_deadline(&self) -> Option<Instant> {
        self.store_sync.due
    }

    pub(crate) fn prepare_store_quit(&mut self) {
        self.store_sync
            .quit_deadline
            .get_or_insert(Instant::now() + Duration::from_secs(5));
    }

    pub(crate) fn store_pending(&self) -> bool {
        self.store_sync.writing
            || !self.store_sync.queue.is_empty()
            || self.store_sync.discoveries > 0
    }

    fn accept_store(&mut self, store: RepoStore) {
        if self.store == store {
            return;
        }
        let added: Vec<_> = store
            .repos
            .iter()
            .filter(|repo| !self.store.contains(&repo.path))
            .map(|repo| repo.path.clone())
            .collect();
        self.store = store;
        // Old rows still identify the selected path while rebuilding.
        self.rebuild_rows();
        let keep: Vec<_> = self
            .statuses
            .keys()
            .filter(|path| self.is_known_tree(path))
            .cloned()
            .collect();
        self.statuses.retain(|path, _| keep.contains(path));
        for path in added {
            self.request_scan(path, false);
        }
    }
}
