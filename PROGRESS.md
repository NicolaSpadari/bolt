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
  `zed-extensions/material-icon-theme` (MIT — its license notice must ship). Refreshed by a `script/bolt-update-icons`
  script, independent of upstream syncs.

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
never rebased; feature branches `bolt/<feature>`. Full strategy + design rules: `notes/upstream-sync.md`.

## Phases

### P0 — Fork & setup
- ⏳ Clone Zed into `~/Projects/bolt`, remote `upstream`, record base commit + Zed version in the Log
- ✅ Clone on v1.23.3, private repo `NicolaSpadari/bolt`, repo-local identity + credential helper, rerere
- ✅ Toolchain: Rust 1.98.1 (auto via rustup), cmake (brew), Metal Toolchain (Xcode 26 needs
  `xcodebuild -downloadComponent MetalToolchain`, else `gpui_apple` build fails)
- ✅ First debug build (`cargo build`, ~10 min cold; `target/` ~15 GB). Smoke run isolated with
  `./target/debug/zed --user-data-dir /tmp/bolt-smoke` (never run dev builds without it until the rebrand: they would
  share `~/.config/zed` and Zed's data dir)
- ⏳ Default theme One Dark Pro Cursor + native Material Icon Theme
- ⏳ Read upstream `CLAUDE.md` / `.rules` / `CONTRIBUTING.md` / `docs/src/development/macos.md` and adopt conventions
- ⏳ Rebrand: app name, bundle ID, icon, data/config/log/socket paths (`paths` crate), CLI binary name
- ⏳ Minimal strip per `notes/minimal-strip.md`: AI + Threads, edit predictions, collab/account, telemetry, crash
  upload, auto-update; verify no outgoing traffic to zed.dev
- ⏳ Verify side-by-side run with installed Zed (separate settings, no shared DB, no single-instance clash)
- ⏳ Release build + `.app` bundle script (`script/bundle-mac` adapted)
- ✅ Upstream sync strategy documented (`notes/upstream-sync.md`)
- ⏳ Base on a stable tag (not `main`), `.bolt-zed-version` file, `git rerere` enabled
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
(List every upstream file modified, with the reason. Empty until P0.)

## Log
- 2026-10-13: project defined; decisions recorded; no clone yet, no code.
- 2026-10-14: upstream sync strategy defined. Checked upstream root: has `CLAUDE.md`, `AGENTS.md`, `.rules` (no
  `PROGRESS.md`/`HANDOVER.md`/`notes/`); workspace `members` is an explicit list; latest stable tag v1.23.3.
  User confirmed sync strategy, bundle ID, minimal scope, git panel replacement. Root `CLAUDE.md` removed.
  Cloned v1.23.3 (base commit 96a1b1b538), pushed to private `NicolaSpadari/bolt`, first debug build + isolated smoke
  run OK. Icon theme note: embedded SVGs render as single-color masks, so Material icons must be extracted to disk at
  startup and loaded as external icons (like extensions do), from a `bolt_*` crate.
