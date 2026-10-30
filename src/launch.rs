//! Starting the GUI apps of Open With. They run detached from the terminal
//! (no stdin, no stdout, their own process group), so their output never
//! lands on the TUI and terminal signals never reach them.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a command may run before it counts as started. Launchers such as
/// `open` and `code` exit at once; a command still running is the app itself
/// and is left running (a thread reaps it when it exits).
const SETTLE: Duration = Duration::from_secs(3);

/// Runs `command` with `$SHELL -c` in `dir` and returns once it exited or
/// settled. A non-zero exit fails with the last line it wrote to stderr.
pub fn spawn_detached(dir: &Path, command: &str) -> anyhow::Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut cmd = Command::new(shell);
    cmd.arg("-c")
        .arg(command)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn()?;
    // Drained on its own thread so a chatty app never blocks on a full pipe.
    let (tx, rx) = std::sync::mpsc::channel();
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_string(&mut text);
        }
        let _ = tx.send(text);
    });
    let deadline = Instant::now() + SETTLE;
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            // An app started by the command may keep stderr open: do not wait.
            let text = rx
                .recv_timeout(Duration::from_millis(300))
                .unwrap_or_default();
            match text.lines().rev().map(str::trim).find(|l| !l.is_empty()) {
                Some(line) => anyhow::bail!("{line}"),
                None => anyhow::bail!("exited with code {}", status.code().unwrap_or(-1)),
            }
        }
        if Instant::now() >= deadline {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_success_and_the_last_stderr_line_of_a_failure() {
        let dir = std::env::temp_dir();
        assert!(spawn_detached(&dir, "true").is_ok());
        let err = spawn_detached(&dir, "echo first >&2; echo 'no such app' >&2; exit 3")
            .unwrap_err()
            .to_string();
        assert_eq!(err, "no such app");
        let err = spawn_detached(&dir, "exit 4").unwrap_err().to_string();
        assert_eq!(err, "exited with code 4");
    }
}
