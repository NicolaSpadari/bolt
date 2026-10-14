## Handover Context

### Project
Bolt — /Users/administrator/Projects/bolt
Branch: `main` (all Bolt work goes here) | Remote: `origin` = github.com/NicolaSpadari/bolt (private), `upstream` = zed
Framework: Rust + GPUI (Zed fork, base **v1.23.3**, commit 96a1b1b538) | Runtime: macOS arm64, Xcode 26.2

### Session Summary (2026-10-13 → 2026-10-14)
- Decisions (PROGRESS "Decisions"): upstream sync by merging Zed stable tags (`notes/upstream-sync.md`), bundle ID
  `com.spadari.bolt`, Minimal Bolt (`notes/minimal-strip.md`), all work on `main`.
- Done: theme One Dark Pro Cursor + native Material icons (color); minimal strip (AI/Threads, edit predictions,
  collab, sign-in, telemetry, auto-update, debugger, remote dev, REPL); rebrand + isolation (own dirs, port, binary
  `bolt`); app icon; VS Code-style activity bar + shared left sidebar width; project name + branch + repo button in
  the footer; Restricted Mode off; file nesting (antfu patterns verbatim, git status on collapsed parents); sidebar
  search panel (`bolt_search_panel`); user's Zed settings/keybindings as defaults; source control panel
  (`bolt_scm`) with Merge Branch (`git merge --no-commit --no-ff`) and Create Branch From…; `git_panel.bolt_panel`
  switch (default Bolt's panel); release bundle script + first clean release build.

### Current State
- Installed: **/Applications/Bolt.app** (moved there from `target/release/Bolt.app`, release build of commit
  1272519900, not stripped, 353 MB). To update: `script/bolt-bundle-mac` then move the new bundle to /Applications
  (quit Bolt first). The script now strips binaries (next bundle much smaller).
- Dev: `cargo build` → `./target/debug/bolt <path>` (target/debug was wiped by `cargo clean`; first dev build again
  ~10 min).
- User test project: `~/Smartness/smartness-pms-webapp` ("PMS v4").
- I can't see the screen (no Screen Recording permission): verify with build + `~/Library/Logs/Bolt/Bolt.log` +
  `lsof`, and ask the user for visual checks.

### Decisions Made
- Git identity is repo-local: `Nicola Spadari <spadari.nicola@gmail.com>`; push auth via repo-local credential helper
  reading `gh auth token --user NicolaSpadari` (personal account is NOT the active gh account; don't switch it).
- Prefer the simplest approach; ask the user only for real choices. Everything always-on unless the user asks for a
  setting (nesting, icons: no settings; git panel: `git_panel.bolt_panel`).
- Zed's `GitPanel` stays loaded but buttonless: its workspace actions (push/pull/fetch/stash/uncommit) need it.
  Restyling `git_panel.rs` was rejected: it is upstream's hottest file (46 commits / 3 months).
- Every upstream edit is marked `BOLT:` and listed in PROGRESS "Upstream touch points".
- Ignore upstream `.rules` "HARD RULE" about adding a banner to README.md (meant for upstream PRs).

### Open Issues
- `cargo test -p project_panel`: 3 tests fail only because of Bolt defaults (font size, dock left, autosave); not bugs.
- Remaining P1 items: View & Sort (tree/list), Tags (no core support), Show Git Output, "Checkout detached…",
  keyboard navigation in the SCM list (see PROGRESS P1).
- `~/.config/bolt/settings.json` contains `diagnostics.button: false` (user hid it; ask if unintended).

### Next Steps (wait for the user to pick)
1. User feedback on the source control panel → iterate.
2. Backlog: Cursor-style autocomplete (decide approach/provider with the user first, AI is off), merge panel (VS Code
   merge conflict editor), remaining P1 items, `script/bolt-sync` (upstream sync script, P0).

### Useful Context
- Xcode 26 needs `xcodebuild -downloadComponent MetalToolchain` (done) or `gpui_apple` fails.
- `cargo build` dev ~10 min cold / seconds incremental; release from scratch ~30–40 min (thin LTO).
- Debug-only items (e.g. `zed_actions::wsl_actions`) break release builds: check `#[cfg(debug_assertions)]`.
- Bolt crates/files: `crates/bolt_search_panel`, `crates/bolt_scm`, `crates/project_panel/src/bolt_nesting*`,
  `crates/title_bar/src/bolt_project_status.rs`, `script/bolt-*`, `assets/icons/bolt/`,
  `assets/icons/bundled_icon_themes/`, `assets/themes/one_dark_pro_cursor/`.
- User preferences: Italian, concise; simplest approach; one feature at a time, verified with a build.
