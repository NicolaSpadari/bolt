# Minimal strip — wiring map (Zed v1.23.3)

Research snapshot (2026-10-14) of where each removed feature is wired. Line numbers refer to v1.23.3.

## Key facts
- `disable_ai: true` (default.json) hides: agent panel, threads sidebar/multi-workspace, inline assistant, AI command
  palette namespaces + keybindings, View>Agent Panel, edit-prediction status button, Copilot/Zeta, editor AI context
  items, quick-action AI buttons, generate commit message (via `AgentSettings::enabled()`), MCP context servers, ACP
  registry fetch, Panel Layout actions, AI items in user menu. NOT hidden: onboarding AI section
  (`onboarding/src/basics_page.rs` `render_ai_section`), Settings UI "AI" page (`settings_ui/src/page_data.rs` `ai_page`),
  AI crate inits in `zed/src/main.rs` (gated internally).
- Client does not auto-connect when signed out (`authenticate()` only with stored credentials).
- ONE server URL (`server_url`, https://zed.dev) derives rpc, release notes, extensions API, telemetry, cloud, LLM.
  Extensions have no separate URL → keep `server_url`, switch the other features off at source.
- Telemetry gated by `telemetry.metrics` / `telemetry.diagnostics`; crash handler needs `ZED_MINIDUMP_ENDPOINT`
  (unset in local builds). Auto-update: setting `auto_update`, never polls on Dev channel; compile-time
  `ZED_UPDATE_EXPLANATION` disables it fully.
- Collab: panel `zed.rs` CollabPanel load/add; title bar sign-in (`title_bar.show_sign_in`), user menu
  (`title_bar.show_user_menu`), call status (`show_call_status_icon`). call/channel inits stay (title_bar needs ActiveCall).
- Remote dev: no setting; `recent_projects` (Open Remote handlers, picker buttons, dev-container suggest), menu
  "Open Remote…", keymaps ctrl-cmd-o / ctrl-cmd-shift-o.
- Debugger: `debugger_ui::init`, DebugPanel load/add in `zed.rs`, Run menu items, `debugger.button`,
  `gutter.breakpoints`, settings UI `debugger_page`. Keep `debug_adapter_extension::init` (extension compatibility).
- REPL: `jupyter.enabled: false` is enough.
- Vim/Helix: off by default; only the onboarding switch shows it.
- `title_bar.show_worktree_name` (title_bar.rs) is the only worktree element; false hides it.
