//! Cancelling a background git read. The diff pane loads one diff at a time;
//! when the cursor moves on, the load in flight is cancelled and the git
//! process it is waiting on is killed, so the next diff starts at once
//! instead of after the old one ran to the end.

use std::io::Read;
use std::process::{Child, ChildStdout, Command, ExitStatus, Output, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// Error text of a cancelled read (never shown: the loader drops it).
pub const CANCELLED: &str = "cancelled";

/// Shared by the loader thread and the UI. Cloning shares the token.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<Mutex<State>>);

#[derive(Debug, Default)]
struct State {
    cancelled: bool,
    /// The git process currently started through this token.
    child: Option<Child>,
}

impl Cancel {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Kills the running git process; every later run through this token
    /// fails at once with [`CANCELLED`].
    pub fn cancel(&self) {
        let mut s = self.state();
        s.cancelled = true;
        if let Some(child) = s.child.as_mut() {
            let _ = child.kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.state().cancelled
    }

    /// Starts `cmd` with piped output. Its stderr is collected on a thread so
    /// a long stderr never blocks git while stdout is being read.
    pub(crate) fn spawn(&self, mut cmd: Command) -> anyhow::Result<Tracked<'_>> {
        let mut s = self.state();
        if s.cancelled {
            anyhow::bail!(CANCELLED);
        }
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("cannot run git: {e}"))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let mut err = child.stderr.take().expect("piped stderr");
        let stderr = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            buf
        });
        s.child = Some(child);
        Ok(Tracked {
            cancel: self,
            stdout,
            stderr,
        })
    }

    /// Runs `cmd` to the end like [`Command::output`], killed on cancel.
    pub(crate) fn output(&self, cmd: Command) -> anyhow::Result<Output> {
        let mut run = self.spawn(cmd)?;
        let mut stdout = Vec::new();
        let _ = run.stdout.read_to_end(&mut stdout);
        let (status, stderr) = run.finish()?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

/// A git process started through a [`Cancel`].
pub(crate) struct Tracked<'a> {
    cancel: &'a Cancel,
    pub stdout: ChildStdout,
    stderr: JoinHandle<Vec<u8>>,
}

impl Tracked<'_> {
    /// Stops the process early (enough output was read).
    pub(crate) fn kill(&self) {
        if let Some(child) = self.cancel.state().child.as_mut() {
            let _ = child.kill();
        }
    }

    /// Waits for the process: its exit status and stderr. Fails with
    /// [`CANCELLED`] when the token was cancelled meanwhile.
    pub(crate) fn finish(self) -> anyhow::Result<(ExitStatus, Vec<u8>)> {
        drop(self.stdout);
        let (child, cancelled) = {
            let mut s = self.cancel.state();
            (s.child.take(), s.cancelled)
        };
        let status = child.expect("started child").wait()?;
        let stderr = self.stderr.join().unwrap_or_default();
        if cancelled {
            anyhow::bail!(CANCELLED);
        }
        Ok((status, stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sleeper() -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        cmd
    }

    #[test]
    fn cancel_kills_the_running_process_and_refuses_new_ones() {
        let cancel = Cancel::default();
        let other = cancel.clone();
        let started = Instant::now();
        let killer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            other.cancel();
        });
        let err = cancel.output(sleeper()).unwrap_err();
        killer.join().unwrap();
        assert_eq!(err.to_string(), CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(cancel.output(sleeper()).unwrap_err().to_string(), CANCELLED);
    }

    #[test]
    fn an_uncancelled_run_returns_its_output() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; exit 3"]);
        let out = Cancel::default().output(cmd).unwrap();
        assert_eq!(out.stdout, b"out\n");
        assert_eq!(out.stderr, b"err\n");
        assert_eq!(out.status.code(), Some(3));
    }
}
