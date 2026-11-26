# Tala

**Find your flow.**

[English](README.md) · [Bahasa Indonesia](README.id.md) · [Project knowledge](PROJECT.md)

[![Native CI on main](https://github.com/arsyadal/tala/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/arsyadal/tala/actions/workflows/ci.yml)

Tala is a local terminal workspace for browsing project folders, running shells and coding agents in tabs, and working with Git. It runs inside your terminal without a browser, server, or Node.js runtime. WSL is an optional shell profile on Windows, not a requirement.

**Release status:** Tala is in development. The matrix below describes the platforms the project targets, not a guarantee that every build is ready to ship. Check Native CI for the current results. A release also requires manual checks on Windows, macOS, and Linux.

## Quick start

Install [Rust 1.88 or newer](https://rustup.rs/) and a native linker for your platform. Run Tala from an interactive terminal. Install [Git](https://git-scm.com/downloads) to clone the repository and use the Git panel.

```sh
git clone https://github.com/arsyadal/tala.git
cd tala
cargo run --locked -- .
```

Press `F5` to open a shell. To switch projects, press `F2`, navigate to a folder, then press `o` to make it the workspace. New sessions start there; existing sessions keep their working directories.

To install a `tala` command from this checkout:

```sh
cargo install --path . --locked
tala .
```

Pass a folder path to open it, quoting paths with spaces. If you omit the path, Tala opens the current directory.

## Target platforms

| Platform | Architectures | Shells Tala detects |
| --- | --- | --- |
| Windows with ConPTY | x64, ARM64 | PowerShell, Command Prompt, Git Bash, WSL |
| macOS | Intel, Apple Silicon | Bash, Zsh, Fish |
| Linux | x64, ARM64 | Bash, Zsh, Fish |

Shells must be available on the machine. On Linux, the system clipboard needs an X11 session or a Wayland compositor with data-control support.

## Controls

| Key | Action |
| --- | --- |
| `F1` | Open help. |
| `F2` | Browse files. Use `↑` / `↓` to select, `Enter` to open, `Backspace` to go up, and `o` to use the current folder. |
| `F3` | Open the launcher. Press `Enter` to start the selected profile or `r` to rescan. |
| `F4` | Open Git. `Enter` shows a diff, `d` shows the staged diff, and `r` refreshes status. |
| `F5` / `F6` / `F7` | Open a terminal, switch to the next tab, or close the current tab. |
| `F8` / `F9` | Copy terminal selection or preview text, or paste from the clipboard. |
| `F10` | Quit Tala. |
| `Tab` / `Esc` in a sidebar | Focus the terminal or hide the sidebar. |
| `Ctrl+C` in a session | Interrupt the foreground program. |
| `Shift+Page Up` / `Shift+Page Down` | Scroll through terminal history. |

Click to switch tabs or use a panel. Drag over terminal text before copying. File previews are read-only and limited to UTF-8 text up to 1 MiB.

### Coding agents

Install the provider's CLI before launching it from Tala.

| Agent | Executable |
| --- | --- |
| Codex (ChatGPT) | `codex` |
| Antigravity | `agy` |
| Oh My Pi (OMP) | `omp` |
| Pi | `pi` |

Run `tala --list-profiles` to see what Tala detects. If a CLI is missing, check that its executable is on `PATH`. Restart Tala if the terminal's environment changed after you started it.

Agents run under your operating-system account with the same access they would have in a standalone terminal. Tala does not sandbox them or store their credentials. Sign-in, model selection, and platform support are specific to each CLI.

### Git

Use `s` to stage, `u` to unstage, and `c` to commit staged changes. Use `f` to fetch, `p` to pull with `--ff-only`, and uppercase `P` to push. The panel works when the selected folder is nested inside a repository. If Git needs interactive authentication, run it in a terminal tab instead.

## Configuration

Run `tala --config-path` for the exact location. Create `config.toml` there only if you want to override the defaults:

```toml
# Optional. Use a shell ID from `tala --list-profiles`.
# default_shell = "zsh"
scrollback = 2000
mouse = true
```

Without `default_shell`, Tala uses `$SHELL` if it matches a detected profile, or the first available shell. `scrollback` accepts `0` through `20000` rows per session. Set `mouse = false` to leave mouse handling to your terminal. Unknown keys produce an error at startup.

## Development and releases

```sh
cargo build --locked
cargo test --locked
```

[Native CI](.github/workflows/ci.yml) builds and tests all six OS and architecture combinations on pull requests and pushes to `main`. It also runs a desktop clipboard round-trip test, using a virtual X11 display on Linux. That test is skipped by default in local runs to avoid changing your clipboard.

| Area | Source |
| --- | --- |
| Startup, input, and workspace state | [`src/main.rs`](src/main.rs), [`src/app.rs`](src/app.rs) |
| Terminal UI, PTY sessions, and VT rendering | [`src/ui.rs`](src/ui.rs), [`src/terminal.rs`](src/terminal.rs), [`src/terminal_view.rs`](src/terminal_view.rs) |
| Files and native Git commands | [`src/workspace.rs`](src/workspace.rs) |

When reporting a bug, include the commit or version, OS and architecture, terminal emulator, and steps to reproduce. For a launcher problem, include the shell or agent CLI version and the relevant line from `tala --list-profiles`. Remove credentials and private paths from logs.

### Versioning

The [changelog](CHANGELOG.md) tracks changes under `Unreleased` until a version ships. The first intended release is `0.1.0`; it has not been published. Tala uses [Semantic Versioning](https://semver.org/) for its CLI, configuration, and documented behavior:

- Before `1.0.0`, bump the minor version for features or breaking changes and the patch version for compatible fixes.
- Since `1.0.0`, bump major for breaking changes, minor for backward-compatible features, or patch for backward-compatible fixes.
- Tag the release `vMAJOR.MINOR.PATCH`. For a prerelease, use a suffix such as `v0.1.0-rc.1`. Do not move a published tag or replace its assets; publish a new version instead.

To prepare a release, set `package.version` in `Cargo.toml` and run `cargo check` to update `Cargo.lock`. Move the shipped entries from `Unreleased` to a dated heading such as `## [0.1.0] - YYYY-MM-DD`, using the actual release date. Verify the candidate locally with `python scripts/release_notes.py v0.1.0` (Python 3.11+). The workflow rejects mismatched versions and uses that changelog section as the GitHub release notes.

### Release checklist

Before running the [Release workflow](.github/workflows/release.yml), check these on Windows, macOS, and Linux:

- [ ] Start two terminal sessions, switch tabs, resize, run a command, interrupt it, and close the sessions.
- [ ] Copy and paste Unicode text using the system clipboard.
- [ ] Open a folder whose name contains spaces, preview a file, then verify that a new session starts in that folder.
- [ ] Inspect a diff, stage and unstage a change, commit, and fetch, pull, and push against a test remote.
- [ ] Detect and launch each supported agent CLI on the platforms where its vendor supports it. Record unavailable combinations.

Run the workflow for an existing tag that points to the selected commit. It publishes six archives to [GitHub Releases](https://github.com/arsyadal/tala/releases) only after the checklist is confirmed and all native jobs pass.

Homebrew, winget, a Linux installer script, and the crates.io package are planned, not published install channels. Use the source instructions above until release assets exist. The repository does not currently declare a license.

