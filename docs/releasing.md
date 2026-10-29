# Releasing and Distribution Guide

> Status: Current operational procedure (no release published yet)
> Read when: Preparing or publishing a release.
> Entry point: `scripts/release.sh`

ular-git is distributed as macOS binaries on GitHub Releases and installed
through the Homebrew tap `ular-io/tap` (repository `ular-io/homebrew-tap`),
formula `Formula/ular-git.rb`.

> **A release is not complete until the Homebrew tap is updated.** Stage 1
> publishes the GitHub Release; stage 2 updates the tap so `brew upgrade`
> serves the new binaries. Both are mandatory parts of every release.

## Prerequisites

- Rust toolchain (pinned by `rust-toolchain.toml`).
- GitHub CLI (`gh auth status` must pass for an account that can push to
  `ular-io/ular-git` and `ular-io/homebrew-tap`).
- `shasum` (standard on macOS).

## 1. Prepare and run the release script

1. Review the changes since the previous tag. When they include a major
   user-facing feature, update `README.md`.
2. Bump `version` in `Cargo.toml`, run `scripts/check.sh` (it syncs
   `Cargo.lock`), commit, and push to `main`. The script pushes only the tag.
3. Run:

   ```bash
   ./scripts/release.sh v0.1.0
   ```

The script builds `aarch64-apple-darwin` and `x86_64-apple-darwin` release
binaries, packs `dist/ular-git-mac-arm64.tar.gz` and
`dist/ular-git-mac-amd64.tar.gz`, prints both SHA256 checksums, creates and
pushes an annotated tag, and creates the GitHub Release with generated notes.

## 2. Update the Homebrew formula (mandatory)

Locate the tap checkout portably (never hard-code a personal clone path):

```bash
TAP_DIR="$(brew --repository ular-io/tap 2>/dev/null)"
[ -d "$TAP_DIR" ] || { gh repo clone ular-io/homebrew-tap /tmp/homebrew-tap && TAP_DIR=/tmp/homebrew-tap; }
cd "$TAP_DIR" && git pull --ff-only
```

Update all five fields together in `Formula/ular-git.rb`: `version`, the ARM64
`url` and `sha256`, the AMD64 `url` and `sha256`. The explicit `version` keeps
Homebrew from reading the `64` in `arm64` as a version.

```ruby
class UlarGit < Formula
  desc "TUI to watch many git repositories, their worktrees, changes, and diffs"
  homepage "https://github.com/ular-io/ular-git"
  version "0.1.0"
  license "MIT"

  if Hardware::CPU.arm?
    url "https://github.com/ular-io/ular-git/releases/download/v0.1.0/ular-git-mac-arm64.tar.gz"
    sha256 "<ARM64_SHA256_HASH>"
  else
    url "https://github.com/ular-io/ular-git/releases/download/v0.1.0/ular-git-mac-amd64.tar.gz"
    sha256 "<AMD64_SHA256_HASH>"
  end

  def install
    bin.install "ular-git"
  end

  def caveats
    <<~EOS
      Homebrew core has an unrelated `ugit`. For a short name, add an alias:
        alias ugit='ular-git'
    EOS
  end

  test do
    system "#{bin}/ular-git", "--version"
  end
end
```

Recompute checksums from the published assets when needed:

```bash
V=v0.1.0
for arch in arm64 amd64; do
  echo -n "$arch: "
  curl -sL "https://github.com/ular-io/ular-git/releases/download/$V/ular-git-mac-$arch.tar.gz" | shasum -a 256 | awk '{print $1}'
done
```

Validate, commit (English only, tap repository rule), and push:

```bash
ruby -c Formula/ular-git.rb
git add Formula/ular-git.rb
git commit -m "chore: release ular-git v0.1.0"
git push origin main
```

Users then install or upgrade with:

```bash
brew tap ular-io/tap
brew install ular-git      # or: brew upgrade ular-git
```

After the first release, replace the "build from source" note at the top of
the README installation section with these commands.
