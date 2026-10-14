## Handover Context

### Project
Bolt — /Users/administrator/Projects/bolt
Branch: — (not a git repo yet; Zed not cloned) | PR: —
Framework: Rust + GPUI (Zed fork) | Runtime: macOS arm64 (Darwin 24.6)

### Session Summary
- 2026-10-13: user wants "Bolt", a Zed fork with VS Code features they like. Feasibility agreed:
  Zed extensions (WASM) cannot add panels/UI, so a fork is required.
- Scope fixed: P1 VS Code Source Control panel with **all features except the graph** (Zed already has one),
  P2 search results in the sidebar, P3 antfu-style file nesting, more on request.
- Only documents were created. **No clone, no code** (explicit user instruction for this session).

### Current State
- Files: `PROGRESS.md` (plan, rules, phases), `HANDOVER.md` (this), `CLAUDE.local.md` (agent rules),
  `notes/vscode-scm-parity.md` (P1 checklist), `notes/upstream-sync.md`, `notes/minimal-strip.md`.
- Git: none. Zed is not cloned.
- Build: n/a.

### Decisions Made
- Path `~/Projects/bolt`; Zed is the base, VS Code only a behavior reference (no TS/Electron code).
- New features in `bolt_*` crates, minimal upstream diff, every upstream touch listed in PROGRESS.
- Full isolation from the installed Zed (bundle ID, config/data dirs, auto-update).
- GPL-3.0 for Bolt (inherited from Zed).
- 2026-10-14: user requirement — periodically update the Zed base without losing Bolt changes, safely and cheaply.
  Strategy (merge stable tags on a `sync/` branch, rerere, `// BOLT:` markers, touch-point registry) in
  `notes/upstream-sync.md`; confirmed by the user.
- 2026-10-14: bundle ID `com.spadari.bolt`; Minimal Bolt (no AI/Threads, collab, telemetry, auto-update) per
  `notes/minimal-strip.md`; Zed git panel replaced. Root `CLAUDE.md` deleted, Bolt rules in `CLAUDE.local.md`.

### Open Issues
- Open decisions in PROGRESS → ask the user: fork hosting; keep/drop extensions, remote dev, debugger, REPL, vim, terminal.

### Next Steps (P0, only after user go-ahead)
1. Clone into the non-empty folder **on the latest stable tag**, not `main` (see `notes/upstream-sync.md`):
   `cd ~/Projects/bolt && git init && git remote add upstream https://github.com/zed-industries/zed.git && git fetch upstream --tags && git checkout -b main vX.Y.Z`
   + `git config rerere.enabled true` + write `.bolt-zed-version`. Record base tag in the PROGRESS log.
2. Read upstream contributor rules (`CLAUDE.md`/`.rules`, `CONTRIBUTING.md`, macOS dev docs) and update PROGRESS.
3. Toolchain + first debug build (`cargo run`), then rebrand + isolation, then release bundle.
4. P1 gap analysis: fill the "Zed today" column of `notes/vscode-scm-parity.md` from `crates/git_ui` and the running app.

### Useful Context
- `~/Projects/Hedron/docs/zed-gpui-rendering-notes.md`: earlier GPUI notes from the Hedron project, useful for GPUI.
- Relevant Zed crates: `git`, `git_ui`, `project` (git_store), `search`, `project_panel`, `workspace` (Panel/Dock),
  `paths`, `release_channel`, `zed`.
- Zed dev builds take many minutes the first time and tens of GB in `target/`: check disk space before building.
- User preferences: Italian, concise; one feature at a time, verified with a build; close test app instances.
