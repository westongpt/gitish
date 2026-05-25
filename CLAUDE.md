# gitish — Developer Agent Instructions

## Goal

Build `gitish`: a terminal-based git staging UI written in Rust. Think `git add -p` but
interactive — a TUI where the user can browse unstaged/staged hunks, stage/unstage
individual hunks or whole files, write a commit message, and commit, all without leaving
the terminal.

Once I complete a feature, I will build the app and stop to ask the human for feedback before continuing.

## Stack decisions (locked)

| Concern | Choice |
|---------|--------|
| TUI framework | `ratatui` + `crossterm` |
| Git backend | `git2` crate (libgit2 bindings) |
| Async runtime | none — keep it synchronous |
| Error handling | `anyhow` for application errors, `thiserror` for library-facing errors |
| Config | `serde` + `toml` via `dirs` crate for XDG paths |

## Architecture

```
src/
  main.rs          # entry point: parse args, open repo, run app
  app.rs           # App struct — top-level state machine, event loop
  git/
    mod.rs
    repo.rs        # repo discovery, status, diff parsing
    stage.rs       # stage/unstage hunks and files
    commit.rs      # commit with message
  ui/
    mod.rs
    layout.rs      # ratatui layout helpers
    file_list.rs   # left panel: file tree with staged/unstaged counts
    diff_view.rs   # right panel: hunk-by-hunk diff with syntax highlight
    commit_bar.rs  # bottom bar: commit message input + keybindings
  keybinds.rs      # all keybinding constants and handler dispatch
  config.rs        # user config (keybinds overrides, color theme)
```

## Coding conventions

- No `unwrap()` or `expect()` in non-test code — propagate with `?`.
- Prefer small, focused functions. If a function needs a comment to explain what it does,
  split it instead.
- Do not add comments that describe what the code does — only add a comment when the *why*
  is non-obvious (upstream quirk, hidden constraint, workaround).
- Write unit tests for all git backend logic (`git/` module). UI code does not need tests.
- Keep `main.rs` thin — it should do nothing except initialize and hand off to `app.rs`.

## Current status

> Update this section as work progresses.

- [x] Cargo.toml dependencies added
- [x] `git/repo.rs` — open repo, list changed files, parse diffs into hunks
- [x] `git/stage.rs` — apply/unapply patch hunks via libgit2
- [x] `git/commit.rs` — write commit
- [x] `app.rs` — event loop, Container/UseCase/Router pattern
- [x] `ui/file_panel.rs` — file panel with nerd font icons, partial-staging color
- [x] `ui/diff_panel.rs` — diff panel with hunk navigation
- [x] `ui/commit_bar.rs` — commit title + body input
- [x] `theme.rs` + `seeds.rs` — Catppuccin base16 theming, theme picker
- [x] `config.rs` — XDG prefs, theme persistence
- [x] `flake.nix` — nix dev shell with all build deps

---

## Rules
- We require 80% code coverage, Strive for 100%. We leave that 20% because we don't want to add tests for low value areas such as asserting static values.
- Keep README.md and help command updated. When adding new features, update the README.md and the help command.
- When making a PR, if a screenshot exists for a closed ticket in the /docs/screenshots folder, remove it. We want to keep the repo side managable.

<!-- ============================================================
     FEATURE LIST — paste your wishlist below this line
     ============================================================ -->

## Feature wishlist

- This app will be called gitish.
- The app will be focused on handling diffs and staging changes.
- There will be two panels
    - On the left panel, will be a list of changed files, they will be marked as New, Deleted, or Changed
    - On the right panel, will show a list of changes in the files. There will be bindings to jump to next and previous change, and buttons to stage or discard the change.
- At the bottom of the window will be a spot to compose a commit title and comment. Comment will be optional and be added as a second comment on the git commit as per the norm.
- Full unit tests
- Follow the architecture from the agent-library. Container, UseCase, Router, Gateway.
- Support for nerd font glyphs.
- Theming with built in support for Catppuccin. You can look at my other app ../agent-libary source for theming details.
- Commit button at the bottom will create the commit.
- Push, Pull, Fetch is out of scope for the first release, We can asses that later.




