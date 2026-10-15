## Handover Context

### Project
Bolt — /Users/administrator/Projects/bolt
Branch: `bolt` (all Bolt work, GitHub default); `main` = plain Zed (stable tags only) | Remote: `origin` = github.com/NicolaSpadari/bolt (private), `upstream` = zed
Framework: Rust + GPUI (Zed fork, base **v1.23.3**, commit 96a1b1b538) | Runtime: macOS arm64, Xcode 26.2

### Session Summary (2026-10-13 → 2026-10-15)
- Decisions (PROGRESS "Decisions"): upstream sync by merging Zed stable tags (`notes/upstream-sync.md`), bundle ID
  `com.spadari.bolt`, Minimal Bolt (`notes/minimal-strip.md`), all work on `main`.
- Done: theme One Dark Pro Cursor + native Material icons (color); minimal strip (AI/Threads, edit predictions,
  collab, sign-in, telemetry, auto-update, debugger, remote dev, REPL); rebrand + isolation (own dirs, port, binary
  `bolt`); app icon; VS Code-style activity bar + shared left sidebar width; project name + branch + repo button in
  the footer; Restricted Mode off; file nesting (antfu patterns verbatim, git status on collapsed parents); sidebar
  search panel (`bolt_search_panel`); user's Zed settings/keybindings as defaults; source control panel
  (`bolt_scm`) with Merge Branch (`git merge --no-commit --no-ff`) and Create Branch From…; `git_panel.bolt_panel`
  switch (default Bolt's panel); release bundle script + first clean release build.
- 2026-10-15: SCM round 2 (fetch/pull/push/sync from `bolt_scm`, VS Code action button + status bar sync/publish,
  `--no-track` Create Branch From, merge message prefill, placeholder ellipsis). Verified by the user. Then: terminal
  "+" opens a terminal directly; TypeScript hover expanded one level (`expand_typescript_hovers`); SCM auto-reveal of
  the active file; debug builds on the `dev` channel (run next to the installed Bolt). Later: search panel restyle;
  Cargo features `repl`/`debugger`/`ai`/`collab` (off, 1056 → 959 crates); Vue `<script>` base indent; rebrand pass
  (`notes/rebrand-map.md`); branch pickers by recency; branch split `bolt`/`main`.

### Current State
- All work committed and pushed on `main`. Release bundle rebuilt on 2026-10-14 (`target/release/Bolt.app`); the copy in
  /Applications is replaced only when the user asks (quit Bolt first).
- Dev: `cargo build` → `./target/debug/bolt <path>` (dev channel: runs next to /Applications/Bolt.app, own DB) (incremental 10 s–2 min; touching `editor`/`project`/`settings_content`
  rebuilds most of the tree). The user prefers debug builds for checks.
- User test project: `~/Smartness/smartness-pms-webapp` ("PMS v4"). Its `typescript` is `typescript-native-bridge`
  (tsgo), used by vtsls.
- I can't see the screen: verify with build + `~/Library/Logs/Bolt/Bolt.log`, ask the user for visual checks.
- User settings were merged into Bolt defaults; `~/.config/bolt/settings.json` is empty (backup `.bak-2026-10-14`).

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
- `cargo test -p project_panel`: 3 tests fail only because of Bolt defaults (font size, dock left, autosave); `cargo test
  -p extensions_ui`: 8 suggestion tests fail because `suggest_extensions` is false in Bolt. Not bugs.
- `target/debug/incremental` grows fast (127 GB on 2026-10-15): delete it when the disk fills (cache only).
- Semantic tokens sometimes missing after startup (likely the tsgo bridge answering empty before the project loads and
  Zed not re-requesting); editing the file refreshes. Fix if it recurs: re-request when the server finishes loading.
- `.rules` contains a block injected by the context-mode plugin: never commit it.

### Next Steps (wait for the user to pick)
1. If not done yet: the user resets `main` to Zed v1.23.3 (force push, blocked for the agent):
   `git tag bolt-pre-split-2026-10-15 3cdc9304ef && git push origin bolt-pre-split-2026-10-15 && git branch -f main v1.23.3 && git push --force-with-lease=main:3cdc9304ef origin main`
2. `script/bolt-sync` for the new branch model (`notes/upstream-sync.md`), then the first real sync to the latest Zed
   stable tag.
3. Release build + /Applications update when the user asks (after the features are done).
4. Optional upstream PRs (user's call): Zed on-type format language fix, zed-extensions/vue `script_element` indent.
5. Parked idea: addon + hooks architecture (`notes/addon-architecture.md`).

### Useful Context
- Xcode 26 needs `xcodebuild -downloadComponent MetalToolchain` (done) or `gpui_apple` fails.
- `cargo build` dev ~10 min cold / seconds incremental; release from scratch ~30–40 min (thin LTO).
- Debug-only items (e.g. `zed_actions::wsl_actions`) break release builds: check `#[cfg(debug_assertions)]`.
- Bolt crates/files: `crates/bolt_search_panel`, `crates/bolt_scm`, `crates/project_panel/src/bolt_nesting*`,
  `crates/title_bar/src/bolt_project_status.rs`, `crates/bolt_iconify`, `script/bolt-*`, `assets/icons/bolt/`,
  `assets/icons/bundled_icon_themes/`, `assets/themes/one_dark_pro_cursor/`.
- User preferences: Italian, concise; simplest approach; one feature at a time, verified with a build.
- VS Code parity: read the VS Code source first (`microsoft/vscode/extensions/git/src/*.ts` for git) and replicate the
  exact logic; the user explicitly asked not to guess.
