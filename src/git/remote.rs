//! Push, pull, and fetch: the only network operations.
//!
//! They run in the background with no terminal to prompt on, so every
//! credential prompt is disabled: HTTPS relies on the credential helper
//! (`GIT_TERMINAL_PROMPT=0` from [`git_command`]), SSH on keys already in the
//! agent (`BatchMode=yes`, unless the user configured their own ssh command).
//! A stalled connection is killed after [`TIMEOUT`].

use super::{git_command, run, run_ok};
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// Longest a push, pull, or fetch may run.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// Where a branch without an upstream would be pushed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUpstream {
    pub remote: String,
    pub branch: String,
}

/// Remote to publish a new branch to: `remote.pushDefault`, then `origin`,
/// then the only remote. `None` when the choice is not obvious.
pub fn default_remote(dir: &Path) -> Option<String> {
    let configured = run_ok(dir, &["config", "--get", "remote.pushDefault"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o).trim().to_string())
        .filter(|r| !r.is_empty());
    if configured.is_some() {
        return configured;
    }
    let out = run_ok(dir, &["remote"]).ok()?;
    let remotes: Vec<String> = String::from_utf8_lossy(&out)
        .lines()
        .map(str::to_string)
        .collect();
    if remotes.iter().any(|r| r == "origin") {
        return Some("origin".into());
    }
    match remotes.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// Pushes HEAD's branch to its upstream, or to `new` and sets it as upstream.
/// Returns git's summary line (`main -> main`).
pub fn push(dir: &Path, new: Option<&NewUpstream>) -> anyhow::Result<String> {
    let mut args = vec!["push", "--porcelain"];
    let refspec;
    if let Some(new) = new {
        refspec = format!("HEAD:refs/heads/{}", new.branch);
        args.extend(["--set-upstream", new.remote.as_str(), refspec.as_str()]);
    }
    let out = run_network(dir, &args)?;
    // `--porcelain` prints `<flag>\t<from>:<to>\t<summary>` per ref.
    let line = out
        .lines()
        .find(|l| l.contains('\t'))
        .map(|l| {
            let mut f = l.split('\t');
            let _flag = f.next();
            let refs = f.next().unwrap_or("").replace("refs/heads/", "");
            let summary = f.next().unwrap_or("").trim().to_string();
            format!("{} {summary}", refs.replace(':', " → "))
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "pushed".into());
    Ok(line)
}

/// `git pull --ff-only`: brings in upstream commits only when no merge is
/// needed; a diverged branch or a conflicting local change is refused by git
/// and leaves everything as it was.
pub fn pull(dir: &Path) -> anyhow::Result<String> {
    let out = run_network(dir, &["pull", "--ff-only", "--no-rebase"])?;
    let summary = out
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Already up to date") || l.starts_with("Fast-forward"))
        .map(|l| {
            if l.starts_with("Fast-forward") {
                let stat = out.lines().map(str::trim).rfind(|l| l.contains("changed"));
                match stat {
                    Some(stat) => format!("fast-forward, {stat}"),
                    None => "fast-forward".to_string(),
                }
            } else {
                "already up to date".to_string()
            }
        })
        .unwrap_or_else(|| "pulled".into());
    Ok(summary)
}

/// Plain `git fetch`: the remote of HEAD's upstream, else `origin`, with the
/// user's own fetch settings (`fetch.prune`, refspecs). Returns how many
/// remote-tracking refs changed, or `None` when the repository has no remote.
pub fn fetch(dir: &Path) -> anyhow::Result<Option<usize>> {
    let remotes = run_ok(dir, &["remote"])?;
    if remotes.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let out = run_network(dir, &["fetch"])?;
    // One `<old>..<new>  <ref> -> <remote ref>` line per changed ref.
    Ok(Some(out.lines().filter(|l| l.contains(" -> ")).count()))
}

/// Runs a network git command with prompts disabled and a time limit.
/// Returns stdout and stderr together (git reports progress on stderr).
fn run_network(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let mut cmd = git_command(dir);
    cmd.args([
        "-c",
        "http.lowSpeedLimit=1000",
        "-c",
        "http.lowSpeedTime=30",
    ]);
    if std::env::var_os("GIT_SSH_COMMAND").is_none() && !has_ssh_command(dir) {
        cmd.env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o ConnectTimeout=15",
        );
    }
    let mut child = cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("cannot run git: {e}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("timed out after {} s", TIMEOUT.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    if !status.success() {
        anyhow::bail!("{}", failure_summary(&err, &out));
    }
    Ok(format!("{out}\n{err}"))
}

fn has_ssh_command(dir: &Path) -> bool {
    run(dir, &["config", "--get", "core.sshCommand"])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The line that explains a failure: git's `fatal:`/`error:` line, a
/// rejected-ref hint, or the last output line.
fn failure_summary(err: &str, out: &str) -> String {
    let lines: Vec<&str> = err
        .lines()
        .chain(out.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let pick = lines
        .iter()
        .find(|l| l.starts_with("fatal:") || l.starts_with("error:"))
        .or_else(|| lines.iter().find(|l| l.contains("[rejected]")))
        .or(lines.last())
        .copied()
        .unwrap_or("git failed");
    pick.trim_start_matches("fatal: ")
        .trim_start_matches("error: ")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_summary_prefers_fatal_and_error_lines() {
        let err = "To x\n ! [rejected] main -> main (fetch first)\nerror: failed to push some refs to 'x'\nhint: pull first\n";
        assert_eq!(failure_summary(err, ""), "failed to push some refs to 'x'");
        assert_eq!(
            failure_summary("", " ! [rejected] main -> main (non-fast-forward)\nDone\n"),
            "! [rejected] main -> main (non-fast-forward)"
        );
        assert_eq!(failure_summary("", ""), "git failed");
    }
}
