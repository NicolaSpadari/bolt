# Idea: Bolt as an "addon" on top of Zed (evaluated 2026-10-14, parked)

Status: **parked**. The user's decision: stay a plain fork for now.

## The idea
Move every Bolt change into its own crates ("addon"). The only change to Zed would be one small "Bolt hooks" patch
(a registry of extension points the addon fills at startup), reapplied on every Zed update.

## Why not a Zed extension
Zed extensions are WASM and can only add languages, grammars, LSPs, themes, icon themes, snippets, slash commands,
MCP servers and debug adapters. No panels, no layout changes, no editor rendering, no menus. Only Bolt's theme, icons
and settings would fit.

## Feasible shape (compile-time, not runtime)
Rust can't patch code at runtime, and a `.dylib` plugin would need a rebuild for every Zed version (unstable ABI).
1. Bolt code in `bolt_*` crates (panels, nesting, footer, conflict markers, branding, defaults).
2. A "Bolt hooks" patch: a global registry of optional callbacks in Zed crates (scrollbar markers, project panel
   entry post-processing, footer items, conflict button labels, …), 2–5 lines per hook. Zed crates can't depend on
   `bolt_*` crates (dependency cycles), hence callbacks registered at startup.
3. Bolt defaults/keymaps as an overlay file loaded on top of Zed's `default.json` (the most conflict-prone file).
4. Sync = new Zed tag + reapply the patch + build. API drift in Zed internals still has to be fixed in the addon.

Size on 2026-10-14: ~30 existing Zed files touched, mostly 4–50 lines each (+ new Bolt files living inside Zed
crates). With hooks: estimated ~15 files / ~100 lines. Cost: a few sessions.
Suggested order: (1) defaults/keymap overlay, (2) move `bolt_nesting*`, `bolt_project_status`, branding into
`bolt_*` crates, (3) hook registry for editor / project panel / workspace / title bar, (4) rewrite
`notes/upstream-sync.md` + `script/bolt-sync` around "reapply patch + build".

## Upstream status (checked 2026-10-14)
- zed#21208 "Webview via Extensions": open, +449, no work in progress.
- Discussion zed#53403 "RFC: Visual Extension API" (panels, status bar, custom views): ⬆66. Zed team (May 2026):
  must be driven by the team, "not something we're likely to get to in the near future". No updates since.
- Discussion zed#48015 (UI via GPUI), zed#61727 (status-bar action): no team answer.
- zed#17325, zed#21400, zed#18496 (2024): closed as unactionable / not planned, "no timeline".
- 2026 extension API PRs: minor only (LSP commands, bzip2, debug actions).

Even a future visual extension API would target new panels/status items, not deep overrides (scrollbar markers,
project panel ordering, conflict wording, stripping features, rebrand). `bolt_scm` and `bolt_search_panel` could
move to it one day; the rest would still need hooks.
