# Minimal Bolt — what is removed from Zed and how

Decision (2026-10-14): Bolt is minimal. No AI, no collab, no telemetry, no account. Zed's git panel is replaced by the
VS Code-style one.

## Principle: switch off at the wiring, do not delete source
Deleting upstream crates (e.g. `agent_ui`, `collab_ui`) would create modify/delete conflicts on every sync, because
upstream edits those crates constantly and many other crates import them. Instead:
1. **Settings defaults** — Bolt default settings turn features off (e.g. Zed's `disable_ai`, telemetry flags,
   `auto_update: false`, edit predictions off). Exact keys verified after clone.
2. **Wiring** — in `crates/zed` (app init / workspace setup / menus / title bar / status bar) the unwanted panels,
   actions, buttons and menu items are not registered. Few files, small `// BOLT:` edits.
3. **Network kill-switch** — server/telemetry/crash-upload/update URLs empty or unreachable in Bolt builds, so even a
   missed UI path cannot phone home. Exception: the extensions registry/download endpoints on zed.dev stay reachable.
   Verified with a network monitor (Little Snitch / `nettop`) during smoke runs: only extension traffic allowed.
4. **Later, optional** — cargo features to drop the dead crates from the binary (smaller app, faster build). Only if
   the diff stays small; otherwise accept the extra binary size.

## Removal list
| Area | What goes | Notes |
|---|---|---|
| AI | Agent panel + **Threads** panel/history, inline assistant, text threads, AI onboarding, model/provider settings UI, "generate commit message" | Also drop the AI button from git commit box (updates the note in `vscode-scm-parity.md`) |
| Edit predictions | Zeta, Copilot, Supermaven, Codestral, status-bar toggle | |
| Collab | Collab panel, channels, chat, notifications panel, calls/screen share, shared projects, sign-in / account menu | |
| Telemetry | Metrics, diagnostics/crash upload (minidumps, panic reports) | Local crash logs kept |
| Auto-update | Updater, "what's new"/release notes fetch | Updates = our upstream syncs |
| Git | Zed's git panel (hidden, replaced by `bolt_scm`) | Keep reusable `git_ui` pieces: diff views, blame, branch picker, Zed git graph |

| Remote dev | SSH / WSL / dev containers, remote projects UI | |
| Debugger | DAP debugger panel, breakpoints gutter, debug actions | |
| REPL | REPL / Jupyter kernels | |
| Vim | Vim and Helix modes (setting + toggle + keymaps) | |

## Kept as-is
- Extensions (zed.dev registry, languages/themes/icon themes) — needs zed.dev network access for the registry only.
- Terminal.
