//! Background work: a fixed pool for repository scans plus one-off threads for
//! diffs and writes. Every job returns a [`Patch`] that the event loop applies
//! in one place ([`App::poll_background`]), so key handling never waits on git.

use super::App;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

/// A state change computed off the UI thread, applied by the loop.
pub type Patch = Box<dyn FnOnce(&mut App) + Send>;

type Job = Box<dyn FnOnce() -> Patch + Send>;

/// Scan jobs waiting for a worker. A deque so the focused repository can jump
/// the queue during a full rescan.
#[derive(Default)]
struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    ready: Condvar,
}

pub struct Background {
    tx: Sender<Patch>,
    rx: Receiver<Patch>,
    queue: Arc<Queue>,
    /// Jobs submitted whose patch has not been applied yet.
    pending: usize,
}

impl Background {
    /// Starts `workers` scan threads. They live as long as the process: the
    /// queue is never closed, and an idle worker only waits on the condvar.
    pub fn new(workers: usize) -> Self {
        let (tx, rx) = channel();
        let queue = Arc::new(Queue::default());
        for _ in 0..workers.max(1) {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let mut jobs = queue.jobs.lock().expect("queue lock");
                    loop {
                        if let Some(job) = jobs.pop_front() {
                            break job;
                        }
                        jobs = queue.ready.wait(jobs).expect("queue lock");
                    }
                };
                if tx.send(job()).is_err() {
                    return;
                }
            });
        }
        Background {
            tx,
            rx,
            queue,
            pending: 0,
        }
    }

    /// Queues a pooled job; `urgent` puts it ahead of every waiting job.
    pub fn submit(&mut self, urgent: bool, job: impl FnOnce() -> Patch + Send + 'static) {
        self.pending += 1;
        let mut jobs = self.queue.jobs.lock().expect("queue lock");
        if urgent {
            jobs.push_front(Box::new(job));
        } else {
            jobs.push_back(Box::new(job));
        }
        self.queue.ready.notify_one();
    }

    /// Runs a job on its own thread, outside the scan pool, so a diff or a
    /// commit never waits behind a full rescan.
    pub fn spawn(&mut self, job: impl FnOnce() -> Patch + Send + 'static) {
        self.pending += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
        });
    }

    pub fn in_flight(&self) -> bool {
        self.pending > 0
    }

    fn try_recv(&mut self) -> Option<Patch> {
        let patch = self.rx.try_recv().ok()?;
        self.pending = self.pending.saturating_sub(1);
        Some(patch)
    }

    #[cfg(test)]
    fn recv_timeout(&mut self, timeout: std::time::Duration) -> Option<Patch> {
        let patch = self.rx.recv_timeout(timeout).ok()?;
        self.pending = self.pending.saturating_sub(1);
        Some(patch)
    }
}

impl App {
    /// Applies every finished job. Returns whether anything changed.
    pub fn poll_background(&mut self) -> bool {
        let mut changed = false;
        while let Some(patch) = self.bg.try_recv() {
            patch(self);
            changed = true;
        }
        if changed {
            self.after_background();
        }
        changed
    }

    pub fn background_in_flight(&self) -> bool {
        self.bg.in_flight()
    }

    /// Test helper: applies patches until no job is pending (or `secs` pass).
    #[cfg(test)]
    pub(crate) fn wait_background(&mut self, secs: u64) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        while self.bg.in_flight() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "background jobs did not finish");
            if let Some(patch) = self.bg.recv_timeout(left) {
                patch(self);
                self.after_background();
            }
        }
    }
}
