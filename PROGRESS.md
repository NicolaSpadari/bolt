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
- Stay a plain fork. "Addon + hooks patch" architecture evaluated and parked; Zed has no UI extension API planned
  soon. Details and upstream links: `notes/addon-architecture.md`.
- SCM panel: Zed's native UI (fonts, icons, sizing, components), VS Code's experience (layout, actions, wording).

- Keep: Zed extensions (unchanged, zed.dev registry), terminal. Drop: remote dev (SSH/WSL/containers), debugger,
  REPL/Jupyter, vim/helix modes.
- Base: Zed **v1.23.3**. Hosting: private repo `NicolaSpadari/bolt` on the **personal** GitHub account; repo-local git
  identity `Nicola Spadari <spadari.nicola@gmail.com>` (never the work account/email).
- Default theme: user's custom **One Dark Pro Cursor** (dark mode), bundled as Bolt's default. No third-party credit
  for it (origin unknown, user's own work).
- Default icon theme: **Material Icon Theme bundled natively** (no extension), copied from
  `zed-extensions/material-icon-theme` (Apache-2.0, LICENSE shipped next to it). Rendered in color straight from the
  embedded assets (no extraction to disk). Refreshed with `script/bolt-update-icons`, `crates/bolt_scm/`, `assets/icons/bolt/git_merge.svg` (Lucide, ISC), `script/bolt-update-file-nesting`, `crates/project_panel/src/bolt_nesting*` (code, antfu block verbatim `.jsonc`, MIT LICENSE), `script/bolt-generate-icons.swift` (gradient tile since 2026-10-14), `crates/zed/resources/bolt-icon.*`, `crates/bolt_iconify/`, `script/bolt-theme-from-vscode`, app-icon PNGs (replaced), independent of upstream syncs.
- Branches (2026-10-15, user decision): `bolt` = all Bolt work (GitHub default branch); `main` = plain Zed, only
  fast-forwarded to Zed stable tags, never committed to. Syncs merge `main` into `bolt` (`notes/upstream-sync.md`).

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
  URL scheme `bolt`. Verified: running without `--user-data-dir` leaves Zed's log/dirs untouched. ⏳ CLI name
  (bundle step).
- ✅ App icon: Lucide hexagon glyph in `#acb2be` (theme editor foreground) on a black macOS-grid tile. Source
  `crates/zed/resources/bolt-icon.svg` (+ `bolt-icon.LICENSE`, ISC), regenerate all channel PNGs with
  `swift script/bolt-generate-icons.swift`. Visible once bundled (unbundled dev binary shows a generic icon).
  `Document.icns` (file-type icon) still Zed's. Keychain: Zed keys credentials by server URL (shared with Zed), harmless since Bolt never signs in.
- ✅ Left sidebar: one shared width for all panels, still drag-resizable
- ✅ Minimal strip per `notes/minimal-strip.md` (map: `notes/minimal-strip-map.md`): AI + Threads, edit predictions,
  collab/sign-in/user menu, telemetry, auto-update, debugger, remote dev, REPL, vim onboarding. Startup sign-in removed
  (it reused the installed Zed's keychain credentials). Network check: after first-run extension installs, no outgoing
  connections at startup. User confirmed it works.
- ✅ Restricted Mode off by default (`trust_all_worktrees: true`; set `false` to get the prompt back)
- ✅ VS Code-style activity bar: left dock panel icons in a vertical bar on the far left (bigger icons); project name +
  branch moved from the title bar to the status bar (bottom left). Build + run OK, user check pending
- 🚧 Verify side-by-side run with installed Zed (dirs/port/log verified separate; user to try both open together)
- ✅ Release `.app`: `script/bolt-bundle-mac [--clean] [--install]` → `target/release/Bolt.app` (release + thin LTO,
  Bolt icon `AppIcon.icns`, Info.plist `com.spadari.bolt` + Zed's plist fragments, ad-hoc signature). Clean release
  build ≈ 30–40 min; `--install` copies to /Applications (only on user request).
- ✅ Upstream sync strategy documented (`notes/upstream-sync.md`)
- ✅ Base on a stable tag (not `main`), `.bolt-zed-version` file, `git rerere` enabled
- ⏳ `script/bolt-sync` (merge on `sync/<tag>` branch, rollback tag, touch-point check, build/clippy/test) tried once

### P1 — Source Control panel (VS Code parity)
Full checklist: `notes/vscode-scm-parity.md`. Architecture: new crate `bolt_scm` (own UI, repository API for
stage/unstage/discard/commit); Zed's `GitPanel` stays loaded without a dock button because Zed's workspace git actions
(push/pull/fetch/stash/uncommit/…) are routed through it — `bolt_scm` dispatches them. Shared commit message buffer.
Switch: `git_panel.bolt_panel` (true = Bolt's panel, false = Zed's). Chosen over restyling `git_panel.rs` because that
file is upstream's hottest (46 commits / 3 months).
- 🚧 v1 (2026-10-14): header CHANGES with Commit ✓, Undo Last Commit, Merge Branch…, "…" menu (Pull, Push, Clone,
  Checkout to…, Fetch; Commit / Changes / Pull, Push / Branch / Remote / Stash (all VS Code entries incl. Drop All
  Stashes) / Worktrees submenus); single-line message input (`Message (⌘⏎ to commit on "<branch>")`, enter/⌘⏎
  commit); Commit button + dropdown (Commit, Amend, Commit & Push, Commit & Sync); sections Merge Changes / Staged
  Changes / Changes with counts and hover actions (+ / − / discard all); rows with file icon, name, dir, colored status
  letter (VS Code letters), hover open file / discard / + / −, tooltip "path • Index Modified"; click = single-file
  diff (Zed SoloDiffView); change-count badge on the activity bar icon; `ctrl-shift-g`.
- 🚧 Merge Branch…: branch picker (relative date + last commit) → `git merge --no-commit --no-ff` (user's alias).
- ⛔ View & Sort, Tags, Show Git Output, branch picker extras, keyboard navigation in the list, partial staging UX
  (user, 2026-10-14: not needed).
- ⛔ Commit graph: Zed's stays available from the command palette, no button in Bolt's panel (off by default).

### P2 — Search in sidebar
- ✅ Spec: `notes/vscode-search-parity.md`
- 🚧 `crates/bolt_search_panel` (new crate): left-dock panel (activity bar, between Explorer and Git), query with
  Match Case / Whole Word / Regex, replace row (chevron, Replace All), include/exclude globs + ignore-files toggle
  ("…"), "N results in M files", results grouped by file (icon, name, dir, count, collapsible), line preview with the
  match highlighted and replace preview (strikethrough + new text), click = open at match (preview tab), double click
  = pin, hover actions (replace / dismiss), Refresh / Clear / Collapse-Expand All, keyboard up/down/enter,
  `cmd-shift-f` (seeded with the selection) / `cmd-shift-h`. Zed's multibuffer search still in the command palette.
- ✅ Live refresh: re-runs (600 ms debounce) on edits to files with results and on non-ignored file changes on disk,
  keeping collapsed files collapsed. Smaller preview font; "…" floats under the inputs (no vertical space).
- ⛔ Query history and tree view (user: not needed). ⏳ Open in Editor (optional).

### P3 — File nesting (antfu style)
- ✅ Always on, no setting (user decision). Patterns: `crates/project_panel/src/bolt_nesting_patterns.jsonc` = antfu's
  README settings block **verbatim** (update: `script/bolt-update-file-nesting`, or paste a newer block over it).
  Semantics: VS Code `explorer.fileNesting.patterns` (`*` + `$(capture)`, globs), files only, same directory, one level,
  case-insensitive; exact parent keys win over wildcard ones. Collapsed by default (like `expand: false`).
- ✅ UI: chevron on parents, nested files one level deeper; click on parent opens it and toggles; →/← expand/collapse.
- ✅ VS Code behavior (user choice): nested files stay normal files; a collapsed parent shows its git status combined
  with its nested files'; selecting/revealing a nested file (e.g. opening it) expands its group; ← on a nested file
  collapses its group and selects the parent.
- ⏳ Diagnostics (errors/warnings) of nested files are not yet combined on a collapsed parent.

### Backlog (on request)
- ✅ **SCM panel round 2** (2026-10-15, verified by the user on cb-frontend): logic copied from VS Code's git
  extension source (`actionButton.ts`, `statusbar.ts`, `repository.ts` `_sync`, `git.ts` `branch`), see
  `notes/vscode-scm-parity.md` rows 7 and 20.
  - Fetch / Pull / Push / Sync run on the repository from `bolt_scm` (no longer dispatched to Zed's hidden GitPanel,
    where push/fetch silently did nothing); toasts like Zed's, `bolt_scm:` lines in Bolt.log. After each remote
    operation the repo is rescanned (`bolt_rescan`): Zed doesn't rescan when only remote refs move.
  - Main button: Commit (changes / merge) → Publish Branch (no upstream) → Sync Changes n↓ m↑ → Commit disabled;
    spinner while a remote operation runs.
  - Status bar: sync button with `n↓ m↑` or cloud-upload Publish button next to the branch
    (`bolt_scm::Sync` / `bolt_scm::PublishBranch`). No spinner there (user: the panel button is enough).
  - "Create Branch From…" = `git checkout -q -b <name> --no-track <ref>` (VS Code): before, a branch created from
    `origin/main` tracked `origin/main`, so Sync pushed it to `main` (blocked by GitHub rules on cb-frontend).
    Branch popover (footer): "Create From…" button in the tab bar.
  - Commit input: merge message prefilled from `.git/MERGE_MSG` (subject line, once per merge); placeholder drawn by
    the panel as a truncated `Label` (ellipsis).
- ✅ Terminal panel "+" opens a new terminal directly (no New Terminal / Spawn Task menu) (user request 2026-10-15).
- ✅ Rebrand pass (2026-10-15, map in `notes/rebrand-map.md`): Bolt logo (`VectorName::BoltLogo`,
  `assets/images/bolt_logo.svg` = app icon glyph) and "Welcome to Bolt" on the welcome page / onboarding; "About Bolt",
  "Bolt — Settings"; Help menu without Zed bug report / feature request / email / Twitter / jobs (keeps "Zed
  Documentation", "Zed Repository"); ~40 visible strings (settings descriptions, extensions banners, move to
  Applications, update / CLI / trust messages, Keyboard Context); macOS permission prompts and "Bolt Text Document";
  Document.icns = Bolt app icon (bundle script); `bolt://` links handled like `zed://`. Kept on purpose: identifiers
  (`zed_actions`, `.zed/`, `ZED_*`, settings keys), "Zed (Default)" keymap name, Zed server / Zeta / Edit
  Predictions texts, Zed's DMG + auto-update (unused by Bolt).
- ✅ Merge / Create Branch From pickers sorted by most recent commit (VS Code `git.branchSortOrder` default
  `committerdate`, checked in `extensions/git/package.json`).
- 🚧 SCM "…" menu audit vs VS Code (2026-10-15, user report: Delete Branch only switched; reference
  `notes/vscode-git-commands.md`, from `extensions/git/src/commands.ts`). Built, waiting for the user's check.
  - Delete Branch… = own picker (local branches, current excluded) → `git branch -d`; "not fully merged" → VS Code
    modal → `-D`. Create Branch… = name input, `git checkout -q -b <name> --no-track HEAD`; "already exists" check.
  - Apply / Pop / Drop / View Stash… = own picker per action (`stash_picker.rs`); Zed's applied on Enter for all.
    Drop asks like VS Code; Drop All uses VS Code's wording.
  - Pull (Rebase) / Push (Force) run from bolt_scm (were routed through Zed's hidden GitPanel); force push asks
    VS Code's confirmation, `--force-with-lease`. Remote submenu dropped (user: not used; Add Remote had no handler).
  - Added (user request): Changes › Discard All Changes (Changes group, panel's discard prompt); Branch › Delete Remote
    Branch… (remote branches minus the current upstream, `git push <remote> :<branch>`: Zed's `delete_branch(is_remote)`
    is `git branch -dr`, which only drops the local remote-tracking ref).
  - Merge message race fixed: Zed's scan emits `StatusesChanged` only when statuses / conflicts change, so when
    `git merge` wrote MERGE_MSG after the index the panel missed it, then prefilled the stale message after the
    commit and the main button stayed "Commit" (second commit → error). Now: the scan also emits on a merge message
    change (upstream touch), and the panel rescans after its merge and after each commit (VS Code refreshes after its
    own operations, `repository.ts` `_updateModelState`). Commit errors are logged (`bolt_scm: git commit failed`).
  - Kept Zed's: Checkout to…, Rename Branch, Stash/Stash Staged/Include Untracked (message modal), Apply/Pop Latest,
    Stage/Unstage All, Undo Last Commit (Zed asks only if the commit was pushed; VS Code asks only for merge commits),
    Clone, Worktrees.
- ✅ SCM and search rows: row actions are rendered only for the hovered row (`hovered_row` + `on_hover`), so names are
  not cut. Gotcha: toggling `display` with `group_hover` styles panics in GPUI ("must call prepaint before paint").
- ✅ Vue `<script>` indentation (user request 2026-10-15, ESLint `vue/script-indent` `baseIndent: 1`, verified by the
  user). Root causes measured with a scratch test on the real Vue extension + injected TypeScript: (1)
  vue-language-server's on-type formatting (`;`, `}`, newline) reindents `<script>` without its base indent
  (`vue.format.script.initialIndent` false) → Bolt default `use_on_type_format: false` for Vue.js (like VS Code's
  `formatOnType` off), and Zed fix: inside `<script>` the setting was read for the injected TypeScript, so the Vue.js
  `false` was ignored → on-type format now needs both the cursor language and the buffer language to allow it; (2)
  the Vue extension's indents query has no `script_element` rule → first line after `<script>` at column 0 → Bolt
  appends the rule when loading the extension (`bolt_patch_vue_indents`). Optional upstream PRs: (1) to Zed, (2) to
  zed-extensions/vue.
- ✅ SCM auto-reveal (user request 2026-10-15): switching tab selects and reveals the active file in the panel, like
  VS Code's `scm.autoReveal` (`scmViewPane.ts` `onDidActiveEditorChange`: groups searched from the last one, selection
  kept when already selected or not listed); the selection also survives panel refreshes.
- ✅ TypeScript hover expanded one level (user request 2026-10-15), setting `global_lsp_settings.expand_typescript_hovers`
  (default true): VS Code's `hover.ts` sends tsserver `quickinfo` with `verbosityLevel` (TS ≥ 5.9, +/− change it);
  vtsls has the same provider but never passes a level over LSP, so `crates/project/src/bolt_typescript_hover.rs` asks
  `typescript.tsserverRequest` `quickinfo` with `verbosityLevel: 1` and replaces the signature code block of vtsls'
  hover. Level 1 = the hovered declaration expanded (`interface User { … }`), nested types by name. Works with
  vtsls' bundled TS 5.9.3 and the tsgo bridge (TS 6). Works in `.vue` files too (verified by the user).
- ✅ Search panel UI: restyle `bolt_search_panel` with Zed's native components and sizing, like the SCM panel
  (toolbar at tab height, Zed list rows, outlined buttons, Zed inputs). UI only: the logic stays as is (user request
  2026-10-14).
- ✅ Lighter builds (2026-10-15): `zed` Cargo features, all off by default: `repl`, `debugger` (`dap_adapters`,
  `debug_adapter_extension`, `debugger_tools`, `debugger_ui`), `ai` (agent, agent_ui, acp_*, language_models,
  copilot*, edit_prediction*, codestral, web_search*, sidebar; enables `settings_ui/ai` and `language_tools/ai`),
  `collab` (`collab_ui`). 1056 → 959 crates (debug and release). `call` / `channel` stay (−2 crates, 4 more upstream
  crates to touch). Built-in keymaps skip bindings to actions not in the build. Check with all features on:
  `cargo check -p zed --features ai,collab,repl,debugger`. Gotchas: verify startup with stderr captured (a keymap panic
  is not in Bolt.log); an excluded crate's `init` may also set up unrelated things (`collab_ui::init` initializes the
  title bar → without `collab` Bolt calls `title_bar::init` itself; missing title bar = no window drag, no footer
  project/branch): check what each gated `init` calls. `cargo test -p zed --no-run` must build without features.
- 🚧 Terminal "+" was still a dropdown in some cases (user request 2026-10-15). Cause: panes restored at startup (and
  MovePane splits) got the Bolt buttons only from agent_ui's `set_assistant_enabled`, not built without `ai`, so they
  kept Zed's default "New…" menu. Now applied after restore and on split. Built, waiting for the user's check.
- ✅ SCM stage / unstage / discard felt slow (user report 2026-10-15, backend repo). Not git: on cb-backend `git status`,
  `for-each-ref` (1681 refs) etc. take ≤ 60 ms. Not the hit area (tooltip shows on hover). Cause: the panel rebuilt its
  rows only on `StatusesChanged`, i.e. after `git add` → file watcher on `.git/index` (150–370 ms handling in Bolt.log)
  → full repository scan. VS Code (`git.optimisticUpdate`, default true, `repository.ts` `run` /
  `_updateModelState`) moves the rows as soon as the git command finishes, then re-reads the status right away. Now
  the same: rows follow Zed's finished pending ops (`Staged` / `Unstaged` / `Reverted`, kept until the next scan), and
  after stage / unstage / discard the panel refreshes and calls `bolt_rescan` without waiting for the watcher. Verified
  by the user on cb-frontend (100 modified files).
- 🚧 Footer sync "also pushed" (user report 2026-10-15). VS Code's status bar button is `git.sync` too (pull then
  push), but `repository.ts` `_sync` pushes only when HEAD is still ahead after the pull (`shouldPush`); Bolt always
  pushed (and showed only the push toast). Now: after the pull, `git rev-list --count @{upstream}..HEAD` (unknown →
  push, like VS Code); no push → the pull toast. Footer tooltip = VS Code's `syncTooltip` ("Pull n commits from
  origin/x", "Push m commits to …", "Pull n and push m commits between …"). Built, waiting for the user's check.
- ✅ SCM commit input wraps and grows like VS Code (`scmInput.ts` `getContentHeight`: word wrap on, height clamped to
  `scm.inputMinLineCount` 1 … `scm.inputMaxLineCount` 10 lines, then scrolls): `EditorMode::AutoHeight { 1, 10 }`,
  soft wrap from the "Git Commit" language (`editor_width`). Enter = new line, ⌘⏎ = commit (VS Code `scm.acceptInput`).
  Look reused from Zed's git panel (user request): its `commit_message_editor` (no autoclose, Cut / Copy / Paste menu)
  and `git_commit_editor_style` (buffer font, `git_commit_buffer_font_size`); the placeholder uses the same font.
  Bug in the 2026-10-15 release (user): input huge, no placeholder, typed text invisible. Cause: the AutoHeight
  `EditorElement` was a direct flex-row item, measured at min-content width → wrapped every character up to 10 lines.
  Fix: wrapped in a `w_full` div, container no longer flex. Verified by the user (debug build).
- ✅ Branch pickers show `n↓ m↑` against each branch's upstream (user request 2026-10-15), from VS Code's
  `BranchItem.description` (`commands.ts`: `behind↓ ahead↑`, then the date; used by checkout, merge, delete). Unlike
  VS Code, zero counts are hidden (user: less clutter), nothing shown when in sync. Zed's branch switcher: next to the name (upstream touch); Bolt's Merge / Create From / Delete pickers:
  between name and date. Data from Zed's `for-each-ref %(upstream:track)` (fresh after a fetch). Verified by the user.
- ✅ Empty editor area (project open, no tab): Zed's welcome page as is (Bolt logo, Get Started / Recent Projects,
  same actions), like the screen of a new empty window; before it was blank (user request 2026-10-15, like VS Code /
  Cursor's empty state). Not working yet (user, release 2026-10-15): start with tabs open, close them → still blank. Likely cause to check:
  only the pane created in `Workspace::new` gets `set_should_display_welcome_page(true)`; the center panes restored
  from the DB (workspace persistence) are new `Pane`s without it. Confirmed: restored panes come from
  `Workspace::add_pane`, which now sets the flag too (also for splits). Verified by the user (debug build).
- Timeline / file history view, … (add here when the user asks)

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
| `crates/workspace/src/pane.rs` (welcome) | welcome page also shown with worktrees (`should_display_welcome_page` only) | empty editor area like VS Code |
| `crates/workspace/src/workspace.rs` (welcome) | `add_pane` sets `set_should_display_welcome_page(true)` | welcome page in restored / split empty panes |
| `crates/workspace/src/workspace.rs` | `activity_bar` field, `render_activity_bar` before the left dock (4 layouts), left dock buttons out of status bar, drag resize minus `BOLT_ACTIVITY_BAR_WIDTH` | VS Code-style activity bar |
| `crates/title_bar/src/title_bar.rs` | `mod bolt_project_status`, status item registration in `init`, project items hidden in title bar | project name + branch in status bar |
| `crates/workspace/src/dock.rs` (width) | left dock: `should_resize_all_panels` always true; on panel switch the new panel takes the current width | one shared, resizable sidebar width |
| `crates/paths/src/paths.rs` | `APP_NAME = "Bolt"` | own dirs: `~/.config/bolt`, `~/Library/Application Support/Bolt`, `~/Library/Logs/Bolt` |
| `crates/release_channel/src/lib.rs` | `display_name` Bolt…, `app_id` `com.spadari.bolt…` | identity |
| `crates/release_channel/src/lib.rs` (dev) | debug builds default to the `dev` channel (`ZED_RELEASE_CHANNEL` still overrides) | debug build runs next to the installed Bolt (own instance port + DB) |
| `crates/zed/Cargo.toml`, `crates/zed/src/main.rs`, `crates/zed/src/zed.rs`, `crates/zed/src/zed/quick_action_bar.rs` | features `repl` / `debugger` (optional deps, `#[cfg(feature)]` on their init, DAP log toolbar item, REPL menu); `debugger_ui` also a dev-dependency for the tests; `load_default_keymap` uses `bolt_load_keymap_asset` (`load_asset_allow_partial_failure`: bindings to actions left out of the build are skipped instead of panicking) | lighter builds |
| `crates/zed/Cargo.toml`, `crates/zed/src/{main.rs,zed.rs}`, `crates/zed/src/zed/open_listener.rs` (ai/collab) | features `ai` / `collab`: optional deps, `#[cfg(feature)]` on inits, agent panel + threads sidebar, edit prediction button, ACP/agent-diff toolbars, collab panel action, channel notes, `zed://agent` (stand-in `ExternalSourcePrompt`) | lighter builds |
| `crates/settings_ui/{Cargo.toml,src/settings_ui.rs,src/pages.rs,src/components.rs,src/components/section_items.rs}` | feature `ai`; AI pages / Ollama picker replaced by stand-ins in `bolt_no_ai.rs` (new file) without it; 2 `allow(dead_code)` | lighter builds |
| `crates/language_tools/{Cargo.toml,src/lsp_log_view.rs}` | feature `ai` (Copilot server in the LSP log) | lighter builds |
| `crates/extension_host/src/extension_host.rs` | `bolt_patch_vue_indents` after loading extension queries | Vue `<script>` base indent |
| `crates/editor/src/completions.rs` | on-type formatting only if both the language at the cursor and the buffer language allow it | Vue `<script>` base indent |
| `crates/settings_content/src/extension.rs`, `crates/extension_host/src/extension_settings.rs`, `crates/extensions_ui/src/extension_suggestions.rs`, `crates/settings_ui/src/page_data.rs` | Zed 1.24's `suggest_extensions` as `Option<bool>` (a plain `bool` defaulting to true let every settings layer without the key overwrite `false`); Bolt default `false` in default.json | no extension suggestions; candidate upstream PR |
| rebrand (`// BOLT` on each line): `crates/ui/src/components/image.rs` (`BoltLogo`), `crates/workspace/src/{welcome.rs,notifications.rs,workspace.rs}`, `crates/onboarding/src/{onboarding.rs,basics_page.rs}`, `crates/zed/src/zed.rs` (About), `crates/zed/src/zed/{app_menus.rs,move_to_applications.rs,open_listener.rs}`, `crates/settings_ui/src/{settings_ui.rs,page_data.rs,pages/sandbox_settings.rs}`, `crates/extensions_ui/src/{extensions_ui.rs,components/extension_card.rs}`, `crates/language_tools/src/key_context_view.rs`, `crates/auto_update/src/auto_update.rs`, `crates/install_cli/src/install_cli_binary.rs`, `crates/zed/resources/info/{Permissions,DocumentTypes}.plist` | "Zed" → "Bolt" in visible text, Bolt logo, Help menu, `bolt://` alias | rebrand (on sync: re-check new "Zed" strings with `notes/rebrand-map.md`) |
| `crates/editor/src/editor_tests.rs` | `expand_typescript_hovers: None` in two `GlobalLspSettingsContent` literals | tests compile with the hover setting |
| `assets/settings/default.json` (Vue) | `Vue.js.use_on_type_format: false` | Vue `<script>` base indent |
| `crates/zed/src/zed/mac_only_instance.rs` | `BOLT_INSTANCE_PORT_OFFSET = 3000` | no single-instance clash with Zed (Bolt stable uid 501 = 47438) |
| `crates/zed/Cargo.toml` (rebrand) | `[[bin]] name = "bolt"`, `default-run`, bundle-dev/stable identifier/name, URL scheme `bolt` | binary must match `APP_NAME` (upstream guard in main.rs) |
| `crates/zed/src/zed/app_menus.rs` (rebrand) | "Bolt" app menu, About/Hide/Quit Bolt | labels |
| `crates/project_panel/src/project_panel.rs` | `mod bolt_nesting`, `State.bolt_nesting`, `bolt_nesting::apply` after sort, chevron + depth in `details_for_entry`, click toggles nest, ←/→ on nest parents | file nesting (logic in `bolt_nesting.rs`) |
| `Cargo.toml` (+ `Cargo.lock`) | member + workspace dep `bolt_search_panel` | new crate |
| `crates/zed/Cargo.toml`, `crates/zed/src/main.rs`, `crates/zed/src/zed.rs` | dep, `bolt_search_panel::init`, panel load/add in `initialize_panels` | search panel wiring |
| `assets/keymaps/default-macos.json` | `cmd-shift-f` (Workspace + Pane) → `bolt_search::FocusSearch`, `cmd-shift-h` → `bolt_search::ToggleReplace` | VS Code shortcuts |
| `crates/title_bar/src/title_bar.rs` (2) | project name back in title bar (branch only in status bar), project name `Color::Muted` | user request |
| `assets/settings/default.json` (2) | `title_bar.show_user_menu: true` again (no account items without sign-in) | user request |
| `assets/settings/initial_user_settings.json` | template emptied (no theme/font overrides) | Zed's template forced One Dark when Bolt first wrote the user settings file (e.g. "Hide Button") |
| `assets/settings/default.json` (user prefs) | user's Zed settings imported as defaults (marked `BOLT: user preference`): fonts (Google Sans Mono 14/450, UI 15), hard tabs, line length 150, VSCode base keymap, autosave on focus change, quit on last window closed, terminal (font 13, bar cursor blinking, env vars), git panel (group by staging, file icons), blame without avatar, file types, `diagnostics.button: false`, vtsls `updateImportsOnFileMove: never` | user request |
| `assets/keymaps/default-macos.json` (user prefs) | block at the end: `cmd-ò` → terminal panel, `cmd-k` → toggle comments | user request |
| `Cargo.toml`, `crates/zed/{Cargo.toml,src/main.rs,src/zed.rs}` (scm) | `bolt_scm` crate: member, dep, `init`, panel load/add | source control panel wiring |
| `crates/settings_content/src/settings_content.rs`, `crates/git_ui/src/git_panel_settings.rs`, `assets/settings/default.json` | `git_panel.bolt_panel` (default true) | Bolt vs Zed git panel switch |
| `crates/project/src/git_store.rs` | `Repository::bolt_rescan` (re-reads repo state via `schedule_scan`) | SCM panel Refresh |
| `crates/git_ui/src/git_ui.rs` | `bolt_panel_entry_style` (exposes `status_style`, `file_icons`) | SCM rows rendered like Zed's |
| `crates/editor/src/editor.rs`, `crates/editor/src/element.rs` | `ConflictsOuter` row highlights mark scrollbar markers dirty; full-width conflict markers (`version_control_conflict`) in singleton editors | merge conflicts visible in the scrollbar |
| `crates/git_ui/src/conflict_view.rs` | conflict buttons: Accept Current Change / Accept Incoming Change / Accept Both Changes | VS Code wording |
| `README.md` | rewritten for Bolt (on sync: keep ours) | user request |
| `crates/grammars/src/{typescript,javascript,tsx,css}/highlights.scm` | capture suffixes: `operator.arrow` (`=>`), `punctuation.special.template` (`${ }`), `property.css`, `function.css` (fall back to the unsuffixed capture) | VS Code-identical colors |
| `assets/settings/default.json` (colors) | `semantic_tokens: combined`; `global_lsp_settings.semantic_token_rules` (readonly/constant/defaultLibrary like VS Code); `diff_view_style: unified` | VS Code-identical colors; user preference |
| `crates/settings_content/src/editor.rs`, `crates/settings/src/vscode_import.rs`, `assets/settings/default.json` | `iconify` setting (default true) | `bolt_iconify` |
| `crates/editor/src/inlays.rs`, `crates/editor/src/display_map/inlay_map.rs` | `InlayContent::Image` + `Inlay::image` (rendered like color inlays) | Iconify icon before the name on the cursor row |
| `crates/editor/src/display_map.rs` | `HighlightKey::BoltIconify` | dimmed icon names |
| `crates/editor/src/fold.rs` | type-tagged (code-created) folds are not persisted | Iconify folds came back as "⋯" |
| `crates/editor/src/blink_manager.rs`, `crates/editor/src/editor.rs`, `crates/editor/src/element.rs` | 200 ms cursor fade on blink (off with Reduce Motion) | VS Code smooth caret blinking |
| `crates/project_panel/src/project_panel.rs` (indent guides) | nested files add one level to the guides too | file nesting tree lines |
| `assets/settings/default.json` (UI) | `ui_font_family: .SystemUIFont`; `project_panel.folder_indicator: both`, `hide_root: true` | like VS Code/Cursor |
| `crates/install_cli/src/install_cli_binary.rs`, `crates/cli/src/main.rs` | CLI installed as `/usr/local/bin/bolt` (never replaces Zed's `zed`), clap name `bolt` | `bolt ./folder` |
| `crates/zed/src/main.rs`, `Cargo.toml`, `crates/zed/Cargo.toml` | `bolt_iconify` crate wiring | Iconify |
| `crates/project/src/git_store.rs` (merge) | `compute_snapshot` also emits `StatusesChanged` when only the merge message changes | merge message prefill raced with `git merge` |
| `crates/settings_content/src/extension.rs` | `ExtensionSettingsContent`: `Default` derived instead of the manual impl | clippy `derivable_impls` after `suggest_extensions` became `Option` |
| `crates/git_ui/src/git_ui.rs` | `pub fn bolt_panel_enabled` | read the switch from bolt_scm |
| `crates/git_ui/src/git_panel.rs` | `GitStatusEntry::bolt_new`; `icon()` hidden when `bolt_panel`; `commit_message_editor` / `git_commit_editor_style` `pub` | single-file diff from bolt_scm; Zed panel stays loaded (its workspace git actions need it) but buttonless |
| `crates/git_ui/src/branch_picker.rs` | `n↓ m↑` next to each branch name (`render_match`) | VS Code's branch picker description |
| `assets/keymaps/default-macos.json` (scm) | `ctrl-shift-g` → `bolt_scm::ToggleFocus`; `BoltScmCommit > Editor`: cmd-enter → `bolt_scm::Commit`, enter → `editor::Newline` (VS Code) | shortcuts |
| `crates/title_bar/src/bolt_project_status.rs` (2) | folder icon before the footer project name; repo button (GitHub/GitLab/Bitbucket icon) opening the `origin` web page, hidden without a remote | user request |
| `crates/git_ui/src/git_ui.rs` (scm 2) | `remote_output` module `pub` | push/pull/fetch toasts from bolt_scm |
| `crates/git_ui/src/git_picker.rs` | "Create From…" button in the branch popover tab bar (action built by name: `bolt_scm` depends on `git_ui`) | VS Code "Create Branch From…" from the footer |
| `crates/title_bar/Cargo.toml` | `bolt_scm` dep | footer Publish / Sync button |
| `crates/terminal_view/src/terminal_panel.rs` | "+" in the terminal tab bar = `IconButton` dispatching `workspace::NewTerminal` (was a New Terminal / Spawn Task popover); same buttons applied to restored panes (`persistence.rs`) and MovePane splits: before, only agent_ui's `set_assistant_enabled` did it, absent without `ai` | user request |
| `crates/project/src/lsp_command.rs`, `crates/project/src/project.rs` | `GetHover::response_from_lsp` calls `bolt_typescript_hover::expand`; `mod bolt_typescript_hover` | TypeScript hover expanded |
| `crates/settings_content/src/project.rs`, `crates/project/src/project_settings.rs`, `assets/settings/default.json` | `global_lsp_settings.expand_typescript_hovers` (default true) | TypeScript hover toggle |

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
- 2026-10-14: file nesting (P3) done. `cargo test -p project_panel`: 137 pass, 2 pixel/mouse layout tests fail
  (`test_collapse_selected_entry_does_not_scroll_visible_parent`, `test_focus_follows_mouse_into_blank_area`), also
  without nesting → caused by earlier Bolt layout changes (activity bar / left docks), to look at later.
- 2026-10-14: sidebar search v1 (P2); title bar tweaks (user menu back, project name in title + status bar).
- 2026-10-14: footer repo button + folder icon; search live refresh; fixed theme reset caused by Zed's initial
  user settings template.
- 2026-10-14: user's Zed settings + keybindings imported as Bolt defaults (AI/collab/theme/dock-right ones skipped).
- 2026-10-14: source control panel v1 (P1) + Merge Branch; `git_panel.bolt_panel` switch.
- 2026-10-14: first clean release build + Bolt.app (fixed release-only error: WSL actions exist only in debug builds).
- 2026-10-14: backlog += merge panel. Bolt.app moved to /Applications.
- 2026-10-14: remaining P1 items dropped (user); `diagnostics.button` off by default.
- 2026-10-14: `cargo test -p project_panel`: 136 pass, 3 fail (2 layout + `undo_create_dirty_file`). Not bugs: tests load
  `default.json`, all 3 pass with Zed's values for `ui_font_size`, panel `dock` and `autosave`.
- 2026-10-14: SCM panel restyled with Zed's native UI (toolbar at tab height, Zed section headers/rows 1.75rem, status
  icon + colors from `git_panel` settings, outlined split Commit button); Refresh button; conflicted files open in the
  plain editor (Zed's Use HEAD / Use <branch> / Use Both).
- 2026-10-14: conflicts in the scrollbar + VS Code conflict wording; resolved conflicts move to Staged once added
  (Merge section = still conflicted only); header ✓ removed; commit input in UI font 13px.
- 2026-10-14: theme syntax regenerated from One Dark Pro 3.20.3 with `script/bolt-theme-from-vscode` (TextMate resolver, 77 keys); semantic tokens on; extension suggestions off; user settings merged into defaults (user file reset, backup `~/.config/bolt/settings.json.bak-2026-10-14`).
- 2026-10-14: `bolt_iconify` (inline Iconify icons from node_modules, icon + dimmed name on the cursor row); cursor blink fade;
  system UI font; project panel chevron + folder icon (Zed's `both`), root hidden, nesting tree lines; CLI as `bolt`;
  app icon with a soft gradient. Semantic tokens: if colors stay tree-sitter-only after startup (no "semantic token"
  entries in `dev: open highlights tree view`), it is the startup race noted above — edit the file to refresh.
- 2026-10-15: SCM round 2 (see Backlog). Lesson: for VS Code parity read the VS Code source
  (`microsoft/vscode/extensions/git/src`) before implementing; guessing the publish/sync rules produced a push to
  `main` attempt.
- 2026-10-15: merge panel dropped from the backlog: Zed's conflict view with VS Code wording (Accept Current / Incoming /
  Both Changes) is what the user wanted.
- 2026-10-15: branch split: `bolt` created from main (`3cdc9304ef`), GitHub default branch = `bolt`; `main` back to Zed
  v1.23.3 pending (force push blocked by the agent permissions: the user runs it). Rebrand pass, branch pickers by
  recency, SCM/search row actions only on hover, title bar fix (`collab_ui::init` also inits it).
- 2026-10-15: first upstream sync, Zed v1.23.3 → v1.24.2 (146 upstream commits, 490 files), on `sync/v1.24.2` from
  `bolt` (rollback tag `bolt-pre-v1.24.2`). Auto-merged everything but 8 files / 11 hunks: `Cargo.lock` (took
  upstream, rebuilt), `crates/zed/Cargo.toml` (version, optional `repl`, bundle ids + new `category`), keymap (kept
  both blocks), `default.json`, `mac_only_instance.rs` (new upstream port code + Bolt offset), `main.rs` (`ai` gate
  around the new Copilot setup), `suggest_extensions` (Zed 1.24 added the same setting → Bolt's copy dropped, Bolt
  default `false` kept in default.json). API drift: `linked_worktree_short_name` / `PathStyle::file_name` in
  `bolt_project_status.rs` (re-mirrored from `TitleBar::render`). Checks: debug build, all-features check, zed tests
  build, startup smoke run, BOLT markers (only the 3 suggest_extensions ones gone), no new visible "Zed" strings.
  About 30 min, most of it build time.
- 2026-10-15: `suggest_extensions` (Zed's own since 1.24) didn't honour `false` (plain `bool` + `default_true`: every
  settings layer without the key overwrote it) → made `Option<bool>`; verified with a scratch settings test (true →
  false with Bolt defaults). 8 `extensions_ui` suggestion tests now fail only because Bolt's default is `false`.
  Disk full during the build: `target/debug/incremental` was 127 GB → removed (cache only).
