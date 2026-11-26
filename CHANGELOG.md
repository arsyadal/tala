# Changelog

User-visible changes are recorded here. Tala follows [Semantic Versioning](https://semver.org/) and the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format.

## [Unreleased]

### Added

- Native Rust terminal workspace targeting Windows, macOS, and Linux on x64 and ARM64.
- Product logo asset (`assets/logo.webp`) embedded in English and Indonesian READMEs.
- Folder navigation, file and folder CRUD (create, rename, delete with confirmation), Quick Open (`Ctrl+P`), and recent workspace switching (`Ctrl+Shift+O`) persisted in `state.toml`.
- Lightweight syntax-highlighted UTF-8 viewer and editor (`e` to edit, `Ctrl+S` to save, unsaved-change confirmation).
- PTY terminal tabs and vertical/horizontal split panes (up to 4 panes per tab) with keyboard and mouse controls, scrollback search (`Ctrl+Shift+F`), and system clipboard support.
- Command Palette (`Ctrl+Shift+P`) and selectable color themes (`dark`, `light`, `ocean`, `forest`).
- Shell and coding-agent discovery (`codex`, `agy`, `omp`, `pi`) plus user-configured custom agents via `[[agents]]` in `config.toml`.
- Git status, diffs, staging, commits, fetch, fast-forward-only pull, push, local branch switching/creation, stash push/list/pop, and worktree switching.
- Native build and test matrix, gated GitHub release workflow, and Unix release installer script (`scripts/install.sh`).
- English and Indonesian setup, usage, and release documentation.
- Dual MIT / Apache-2.0 licensing (`LICENSE-MIT` and `LICENSE-APACHE`).
