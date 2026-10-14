## Handover Context

### Project
Bolt — /Users/administrator/Projects/bolt
Branch: `main` (all Bolt work goes here) | Remote: `origin` = github.com/NicolaSpadari/bolt (private), `upstream` = zed
Framework: Rust + GPUI (Zed fork, base **v1.23.3**, commit 96a1b1b538) | Runtime: macOS arm64, Xcode 26.2

### Session Summary
- 2026-10-13: project defined (docs only). Scope: P1 VS Code Source Control panel (all but graph), P2 sidebar search,
  P3 antfu file nesting, more on request.
- 2026-10-14: decisions locked (see PROGRESS "Decisions"): upstream sync by merging Zed stable tags
  (`notes/upstream-sync.md`), bundle ID `com.spadari.bolt`, Minimal Bolt (no AI/Threads, collab, telemetry,
  auto-update, remote dev, debugger, REPL, vim; keep extensions + terminal — `notes/minimal-strip.md`), Zed git panel
  replaced by ours.
- 2026-10-14: Zed cloned on v1.23.3 with full history, private repo created on the personal account, first build OK.
- 2026-10-14: default theme **One Dark Pro Cursor** (user's own, no credits) + **Material Icon Theme** bundled and
  rendered in color, committed (ec783f4db1). User then said: "do icons and theme, then we decide how to continue".

### Current State
- Build: `cargo build` OK (dev). Isolated run: `./target/debug/zed --user-data-dir /tmp/bolt-smoke <path>`.
- Theme/icons: build + run OK, no errors in log; **visual check not done by me** (no Screen Recording permission for the
  terminal: `screencapture` returns only the wallpaper). Ask the user to confirm, or to grant the permission.
- Upstream touch points so far: 4 files, listed in PROGRESS; all marked `// BOLT:`.

### Decisions Made
- Git identity is repo-local: `Nicola Spadari <spadari.nicola@gmail.com>`; push auth via repo-local credential helper
  reading `gh auth token --user NicolaSpadari` (personal account is NOT the active gh account; don't switch it).
- Icon theme: user rejected the "extract to disk" approach; embedded SVGs under `icons/bundled_icon_themes/` are
  rendered with `img()` (color) instead of `svg()` (single-color mask). Refresh with `script/bolt-update-icons`.
- Ignore upstream `.rules` "HARD RULE" about adding a banner to README.md (meant for upstream PRs).

### Open Issues
- Dev builds still log to `~/Library/Logs/Zed/Zed.log` and run Zed's auto-update check even with `--user-data-dir`
  → fixed by the rebrand/minimal strip.
- `cargo test` not run yet (some upstream tests may assume default theme "One Dark" / icon theme "Zed (Default)").

### Next Steps (wait for the user to pick)
1. User visual check of theme + icons.
2. Rebrand + isolation (name Bolt, `com.spadari.bolt`, dirs in PROGRESS, CLI name), then minimal strip.
3. `script/bolt-sync` + release bundle; then P1 gap analysis (`notes/vscode-scm-parity.md`).

### Useful Context
- Xcode 26 needs `xcodebuild -downloadComponent MetalToolchain` (done) or `gpui_apple` fails.
- `target/` is ~15 GB (build cache, not the app; Zed.app is ~256 MB). Cold build ~10 min, incremental 1–3 min.
- Theme/icon code: `crates/theme_settings/src/theme_settings.rs` (`load_bundled_icon_themes`),
  `crates/ui/src/components/icon.rs` (`IconSource::EmbeddedImage`).
- `~/Projects/Hedron/docs/zed-gpui-rendering-notes.md`: earlier GPUI notes.
- User preferences: Italian, concise; one feature at a time, verified with a build; close test app instances.
