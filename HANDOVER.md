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
  rendered in color, committed (ec783f4db1).
- 2026-10-14: panels docked left, worktree name hidden; minimal strip (fb529e94b7); activity bar + project/branch in
  status bar + Restricted Mode off by default (eab5d96d6d).

### Current State
- Build: `cargo build` OK (dev, 0 warnings). Isolated run: `./target/debug/zed --user-data-dir /tmp/bolt-smoke <path>`.
- User confirmed: theme/icons, minimal strip. Pending user check: activity bar, project/branch in status bar.
- I can't see the screen (terminal lacks Screen Recording permission); verify with build + log + `lsof` + user.
- Upstream touch points: listed in PROGRESS, all marked `BOLT:`.

### Decisions Made
- Git identity is repo-local: `Nicola Spadari <spadari.nicola@gmail.com>`; push auth via repo-local credential helper
  reading `gh auth token --user NicolaSpadari` (personal account is NOT the active gh account; don't switch it).
- Icon theme: user rejected the "extract to disk" approach; embedded SVGs under `icons/bundled_icon_themes/` are
  rendered with `img()` (color) instead of `svg()` (single-color mask). Refresh with `script/bolt-update-icons`.
- Ignore upstream `.rules` "HARD RULE" about adding a banner to README.md (meant for upstream PRs).

### Open Issues
- Dev builds identify as **Stable** (`crates/zed/RELEASE_CHANNEL` = stable): they share Zed's single-instance port
  (44438), log file `~/Library/Logs/Zed/Zed.log` and keychain service name → fix in the rebrand (do it next).
- `cargo test` not run yet (upstream tests may assume Zed defaults: theme, docks, status bar items).
- Activity bar hides when the left dock is zoomed? No: it stays (rendered outside `render_dock`).

### Next Steps (wait for the user to pick)
1. User check of activity bar + status bar project/branch.
2. Rebrand + isolation (name Bolt, `com.spadari.bolt`, own channel/port/dirs/keychain, CLI name).
3. `script/bolt-sync` + release `.app` bundle; then P1 gap analysis (`notes/vscode-scm-parity.md`).

### Useful Context
- Xcode 26 needs `xcodebuild -downloadComponent MetalToolchain` (done) or `gpui_apple` fails.
- `target/` is ~15 GB (build cache, not the app; Zed.app is ~256 MB). Cold build ~10 min, incremental 1–3 min.
- Theme/icon code: `crates/theme_settings/src/theme_settings.rs` (`load_bundled_icon_themes`),
  `crates/ui/src/components/icon.rs` (`IconSource::EmbeddedImage`).
- `~/Projects/Hedron/docs/zed-gpui-rendering-notes.md`: earlier GPUI notes.
- User preferences: Italian, concise; one feature at a time, verified with a build; close test app instances.
