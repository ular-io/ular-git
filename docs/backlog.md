# Backlog and Verification Debt

> Status: Current
> Read when: Planning the next feature or auditing unresolved risk.
> This file records candidates and gaps; it does not authorize implementation.

## Confirmed unimplemented behavior

- No Homebrew release yet; `docs/releasing.md` describes the first one.
- Commit messages are a single line (no body editor).
- No separate `fetch`: `↓N` reflects the last fetch done elsewhere (a pull
  fetches).

## Deferred ideas — not committed

- Fetch all repositories on demand, with the same no-prompt rules as push /
  pull.
- Stage / unstage individual files from the file pane.
- Show the repository order of the user's choice instead of by name.

## Verification debt

- Push / pull have been exercised against local bare remotes only, not yet
  against a hosted HTTPS remote with a credential helper or an SSH remote
  through `ssh-agent`.
- Scan cost has been measured with about 30 repositories; behavior with
  hundreds is unmeasured.
