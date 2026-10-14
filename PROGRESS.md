# Bolt — progress

Hybrid code editor: **fork of Zed** (Rust + GPUI, performance first) with selected VS Code features ported on top.
Upstream: https://github.com/zed-industries/zed · Reference for ported features: https://github.com/microsoft/vscode

Status legend: ✅ done · 🚧 in progress · ⏳ todo · ⛔ out of scope

## Ground rules
- **Zed is the base, VS Code is only a behavior reference.** Features are re-implemented in Rust/GPUI; VS Code
  TypeScript is read to understand behavior, never transpiled or embedded (no Electron, no webviews, no extension host).
- **Minimal upstream diff.** New features live in dedicated `bolt_*` crates; upstream crates are touched only for the
  wiring (registering a panel, a setting, a hook). Every upstream touch point is listed in "Upstream touch points" below,
  so merges from Zed stay cheap.
- **Isolation from installed Zed.** Bolt must never share config, data, sockets, auto-update or bundle ID with Zed.
- **Licensing.** Zed editor crates are GPL-3.0 (GPUI Apache-2.0, collab AGPL-3.0) → Bolt is GPL-3.0. VS Code is MIT:
  behavior/UX may be reproduced freely; if any code or icon is copied, keep the MIT notice.
- **Scope on request.** Only the features listed under Phases; other VS Code features are added when the user asks.
- One feature at a time, verified with a build and a manual run; test app instances are closed after use.
- Responses to the user in Italian, concise.

## Decisions (2026-10-13)
- Project path: `~/Projects/bolt` (this folder). Zed is cloned **into this folder** (see HANDOVER for the
  non-empty-dir procedure), keeping `PROGRESS.md`, `HANDOVER.md`, `notes/` at the root.
- Source Control panel: **full VS Code parity**, except the commit graph (Zed already has one in recent releases → keep
  Zed's, just link to it from the panel).
- Order: P0 setup → P1 Source Control → P2 sidebar search → P3 file nesting → backlog on request.

## Decisions (2026-10-14)
- Upstream sync: merge Zed **stable tags** every 2–4 weeks, per `notes/upstream-sync.md`.
- Root `CLAUDE.md` deleted (collided with upstream's; was a duplicate of `~/Projects/CLAUDE.md`). Bolt agent rules live
  in `CLAUDE.local.md`.
- Bundle ID `com.spadari.bolt` (dev builds `com.spadari.bolt.dev`). App dirs: `~/.config/bolt` (settings, keymap),
  `~/Library/Application Support/Bolt` (DB, extensions, state), `~/Library/Logs/Bolt`, `~/Library/Caches/Bolt`.
- **Minimal Bolt**: AI (incl. Threads panel), edit predictions, collab, account, telemetry, crash upload, auto-update
  removed — switched off at the wiring, source kept for cheap merges. Details: `notes/minimal-strip.md`.
- Zed's git panel is **replaced** by the VS Code-style panel (no setting toggle); reusable `git_ui` parts stay.

- Keep: Zed extensions (unchanged, zed.dev registry), terminal. Drop: remote dev (SSH/WSL/containers), debugger,
  REPL/Jupyter, vim/helix modes.
- Base: Zed **v1.23.3**. Hosting: private repo `NicolaSpadari/bolt` on the **personal** GitHub account; repo-local git
  identity `Nicola Spadari <spadari.nicola@gmail.com>` (never the work account/email).
- Default theme: user's custom **One Dark Pro Cursor** (dark mode), bundled as Bolt's default. No third-party credit
  for it (origin unknown, user's own work).
- Default icon theme: **Material Icon Theme bundled natively** (no extension), copied from
  `zed-extensions/material-icon-theme` (Apache-2.0, LICENSE shipped next to it). Rendered in color straight from the
  embedded assets (no extraction to disk). Refreshed with `script/bolt-update-icons`, independent of upstream syncs.
- All Bolt work is committed directly on `main` (no feature branches).

## Open decisions (ask the user before acting)
- none

## Architecture (planned)
| Layer | Location | Role |
|---|---|---|
| Upstream Zed | `crates/*` | Editor, GPUI, workspace, project, git backend (`git`, `project::git_store`), `git_ui`, `search`, `project_panel` |
| Bolt branding | `crates/release_channel`, `crates/paths`, `crates/zed` (app), bundle resources | Name, bundle ID, data dirs, icon, update channel off |
| Source Control | `crates/bolt_scm` (new) | VS Code-style SCM panel, reuses `GitStore` / repository APIs |
| Search panel | `crates/bolt_search_panel` (new) | Sidebar search results tree, reuses `Project::search` |
| File nesting | `crates/bolt_file_nesting` (new) + hook in `project_panel` | antfu-style nesting rules |

Branch model (proposal): `main` = Bolt, updated by **merging Zed stable tags** (`vX.Y.Z`) via a `sync/<tag>` branch,
never rebased; Bolt work goes directly on `main`. Full strategy + design rules: `notes/upstream-sync.md`.

## Phases

### P0 — Fork & setup
- ✅ Clone on v1.23.3, private repo `NicolaSpadari/bolt`, repo-local identity + credential helper, rerere
- ✅ Toolchain: Rust 1.98.1 (auto via rustup), cmake (brew), Metal Toolchain (Xcode 26 needs
  `xcodebuild -downloadComponent MetalToolchain`, else `gpui_apple` build fails)
- ✅ First debug build (`cargo build`, ~10 min cold; `target/` ~15 GB). Smoke run isolated with
  `./target/debug/zed --user-data-dir /tmp/bolt-smoke` (never run dev builds without it until the rebrand: they would
  share `~/.config/zed` and Zed's data dir)
- ✅ Default theme One Dark Pro Cursor (dark mode) + native Material Icon Theme 1.3.2 in color (build + isolated run
  OK, confirmed by the user)
- ✅ Defaults: project, outline and git panels docked left (Zed 1.x puts them right); worktree name hidden in title bar
- ✅ Read upstream `.rules` (Rust guidelines: no `unwrap`, no `let _ =` on fallible ops, no `mod.rs`, `[lib] path`)
  and macOS build docs. Ignore its README "HARD RULE" banner: it is for upstream PRs, not Bolt
- ✅ Rebrand: name Bolt, bundle ID `com.spadari.bolt`, binary `target/debug/bolt`, own dirs + log, own instance port,
  URL scheme `bolt`. Verified: running without `--user-data-dir` leaves Zed's log/dirs untouched. ⏳ app icon, CLI name
  (bundle step). Keychain: Zed keys credentials by server URL (shared with Zed), harmless since Bolt never signs in.
- ✅ Left sidebar: one shared width for all panels, still drag-resizable
- ✅ Minimal strip per `notes/minimal-strip.md` (map: `notes/minimal-strip-map.md`): AI + Threads, edit predictions,
  collab/sign-in/user menu, telemetry, auto-update, debugger, remote dev, REPL, vim onboarding. Startup sign-in removed
  (it reused the installed Zed's keychain credentials). Network check: after first-run extension installs, no outgoing
  connections at startup. User confirmed it works.
- ✅ Restricted Mode off by default (`trust_all_worktrees: true`; set `false` to get the prompt back)
- ✅ VS Code-style activity bar: left dock panel icons in a vertical bar on the far left (bigger icons); project name +
  branch moved from the title bar to the status bar (bottom left). Build + run OK, user check pending
- 🚧 Verify side-by-side run with installed Zed (dirs/port/log verified separate; user to try both open together)
- ⏳ Release build + `.app` bundle script (`script/bundle-mac` adapted)
- ✅ Upstream sync strategy documented (`notes/upstream-sync.md`)
- ✅ Base on a stable tag (not `main`), `.bolt-zed-version` file, `git rerere` enabled
- ⏳ `script/bolt-sync` (merge on `sync/<tag>` branch, rollback tag, touch-point check, build/clippy/test) tried once

### P1 — Source Control panel (VS Code parity)
Full checklist: `notes/vscode-scm-parity.md`. Summary:
- ⏳ Gap analysis: Zed `git_ui` panel vs. VS Code SCM (fill the "Zed today" column of the parity file)
- ⏳ Panel skeleton (`bolt_scm`) replacing Zed's git panel in the dock, activity icon + change-count badge,
  `ctrl-shift-g`
- ⏳ Commit input box (multi-line, placeholder with branch, `cmd-enter`, validation, message history)
- ⏳ Commit button + dropdown (Commit / Amend / Commit & Push / Commit & Sync) + Sync/Publish action button
- ⏳ Resource groups: Merge Changes, Staged Changes, Changes, Untracked (setting) with counts
- ⏳ Rows: icon, name, dimmed path, colored status letter, hover inline actions (open / discard / stage / unstage)
- ⏳ Group inline actions (Stage All / Unstage All / Discard All), multi-select + bulk actions
- ⏳ List ↔ tree view toggle, sort by name / path / status
- ⏳ Click → diff view; context menus (open changes, open file, open HEAD, add to .gitignore, reveal, copy path…)
- ⏳ "…" menu: pull / push / fetch / sync, branch, remote, stash, tags, worktrees, show git output
- ⏳ Multiple repositories section
- ⏳ Merge conflict flow (Merge Changes group, mark resolved)
- ⛔ Commit graph (use Zed's)

### P2 — Search in sidebar
- ⏳ Behavior spec from VS Code search view (`notes/` file to write at phase start)
- ⏳ Panel with query/replace inputs, toggles (case, word, regex), include/exclude globs
- ⏳ Results tree (file → matches with highlighted line preview), counts, collapse/expand all, list/tree
- ⏳ Click → open editor at match (preview), replace one / file / all, dismiss result
- ⏳ Keep Zed's multibuffer project search reachable (e.g. "open in editor" like VS Code's "Open in editor")

### P3 — File nesting (antfu style)
- ⏳ Settings: `file_nesting.enabled`, `file_nesting.expand`, `file_nesting.patterns` (VS Code
  `explorer.fileNesting.patterns` syntax: `"package.json": "package-lock.json, pnpm-*.yaml, .npmrc, …"`,
  `$(capture)` / `*` support)
- ⏳ Nesting applied in `project_panel` entry building; parent row shows expand chevron; git/diagnostic decorations
  bubble up to the parent
- ⏳ Ship antfu's default pattern set as a preset (credit: https://github.com/antfu/vscode-file-nesting-config, MIT)

### Backlog (on request)
- Timeline / file history view, Outline in sidebar, … (add here when the user asks)

## Upstream touch points
Every edit is marked `// BOLT:` (`git grep -n "BOLT:"`). Pure additions (new files) are not listed.
| File | Change | Why |
|---|---|---|
| `assets/settings/default.json` | `theme` (mode dark, One Dark Pro Cursor), `icon_theme` (Material Icon Theme); `dock: left` for project/outline/git panels; `title_bar.show_worktree_name: false` | Bolt defaults |
| `crates/theme/src/icon_theme.rs` | `BUNDLED_ICON_THEMES_DIR` const | shared by loader and `Icon` |
| `crates/theme_settings/src/theme_settings.rs` | `load_bundled_icon_themes()` + call in `init` | load icon themes from embedded assets |
| `crates/ui/src/components/icon.rs` | `IconSource::EmbeddedImage` + routing in `from_path` + render arm | embedded SVG is a single-color mask; theme icons go through `img()` for color |
| `assets/settings/default.json` (minimal) | `disable_ai`, `agent.enabled/button`, `edit_predictions.provider: none`, `show_edit_predictions`, `collaboration_panel.button`, `title_bar.show_sign_in/show_user_menu`, `show_call_status_icon`, `telemetry.*`, `auto_update`, `debugger.button`, `gutter.breakpoints`, `jupyter.enabled` → off; `trust_all_worktrees: true` | Minimal Bolt; no Restricted Mode prompt |
| `crates/zed/src/main.rs` | startup `authenticate` behind `BOLT_SIGN_IN_AT_STARTUP = false` | never reuse an installed Zed's keychain credentials |
| `crates/zed/src/zed.rs` | no Collab/Debug panels; `BOLT_REMOVED_*` + `hide_bolt_removed_actions` (keymap + command palette) | debugger, collab, remote dev, REPL entry points |
| `crates/zed/Cargo.toml` (+ `Cargo.lock`) | `command_palette_hooks` dep | palette filter |
| `crates/zed/src/zed/app_menus.rs` | removed Collab/Debugger panels, Check for Updates, Open Remote, debugger Run items, release notes, telemetry log | menus |
| `crates/onboarding/src/basics_page.rs` | AI, vim, telemetry sections behind `.when(false, …)` | onboarding |
| `crates/settings_ui/src/page_data.rs` | `BOLT_HIDDEN_PAGES` filter (Debugger, Collaboration, AI) | settings UI |
| `crates/recent_projects/src/recent_projects.rs` | "Open Remote Folder" button/menu hidden, `BOLT_SUGGEST_DEV_CONTAINERS = false` | remote dev |
| `crates/workspace/src/dock.rs` | `PanelButtons::vertical()` (column, Medium icons, Large buttons, menu to the right) | activity bar |
| `crates/workspace/src/workspace.rs` | `activity_bar` field, `render_activity_bar` before the left dock (4 layouts), left dock buttons out of status bar, drag resize minus `BOLT_ACTIVITY_BAR_WIDTH` | VS Code-style activity bar |
| `crates/title_bar/src/title_bar.rs` | `mod bolt_project_status`, status item registration in `init`, project items hidden in title bar | project name + branch in status bar |
| `crates/workspace/src/dock.rs` (width) | left dock: `should_resize_all_panels` always true; on panel switch the new panel takes the current width | one shared, resizable sidebar width |
| `crates/paths/src/paths.rs` | `APP_NAME = "Bolt"` | own dirs: `~/.config/bolt`, `~/Library/Application Support/Bolt`, `~/Library/Logs/Bolt` |
| `crates/release_channel/src/lib.rs` | `display_name` Bolt…, `app_id` `com.spadari.bolt…` | identity |
| `crates/zed/src/zed/mac_only_instance.rs` | `BOLT_INSTANCE_PORT_OFFSET = 3000` | no single-instance clash with Zed (Bolt stable uid 501 = 47438) |
| `crates/zed/Cargo.toml` (rebrand) | `[[bin]] name = "bolt"`, `default-run`, bundle-dev/stable identifier/name, URL scheme `bolt` | binary must match `APP_NAME` (upstream guard in main.rs) |
| `crates/zed/src/zed/app_menus.rs` (rebrand) | "Bolt" app menu, About/Hide/Quit Bolt | labels |

Bolt-only files: `assets/themes/one_dark_pro_cursor/`, `assets/icons/bundled_icon_themes/material_icon_theme/`,
`script/bolt-update-icons`, `crates/title_bar/src/bolt_project_status.rs` (mirrors the project-items block of `TitleBar::render`: re-check on syncs), `notes/minimal-strip-map.md`, `.bolt-zed-version`, `CLAUDE.local.md`, `PROGRESS.md`, `HANDOVER.md`, `notes/`.

## Log
- 2026-10-13: project defined; decisions recorded; no clone yet, no code.
- 2026-10-14: upstream sync strategy defined. Checked upstream root: has `CLAUDE.md`, `AGENTS.md`, `.rules` (no
  `PROGRESS.md`/`HANDOVER.md`/`notes/`); workspace `members` is an explicit list; latest stable tag v1.23.3.
  User confirmed sync strategy, bundle ID, minimal scope, git panel replacement. Root `CLAUDE.md` removed.
  Cloned v1.23.3 (base commit 96a1b1b538), pushed to private `NicolaSpadari/bolt`, first debug build + isolated smoke
  run OK. Icon theme note: embedded SVGs render as single-color masks, so Material icons must be extracted to disk at
  startup and loaded as external icons (like extensions do), from a `bolt_*` crate. → superseded: user asked for the
  simple way; `img()` renders embedded SVGs in color, so theme icons are routed there (4 small upstream edits).
  Theme + icons committed on `main` (ec783f4db1). Gotchas: the terminal has no Screen Recording permission
  (`screencapture` shows only the wallpaper); dev builds still write logs to `~/Library/Logs/Zed/Zed.log` even with
  `--user-data-dir` (fixed by the rebrand).

- 2026-10-14: minimal strip done; found that dev builds identify as the Stable channel (`crates/zed/RELEASE_CHANNEL`)
  and share Zed's single-instance port 44438 and log file → fix in the rebrand. Activity bar + status bar project items.
- 2026-10-14: rebrand + shared sidebar width (ae19e5da62). Dev builds no longer need `--user-data-dir`.
