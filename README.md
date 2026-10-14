# Bolt

Bolt is a personal fork of [Zed](https://github.com/zed-industries/zed): Zed's speed and native UI, with the parts of
the VS Code experience I rely on, and without the features I don't use.

Based on Zed **v1.23.3**. macOS (Apple Silicon) only.

---

### What's different from Zed

- **VS Code-style layout**: activity bar and a shared left sidebar for Explorer, Search and Source Control.
- **Source Control panel**: Merge / Staged / Changes sections with stage, unstage and discard actions; commit,
  amend, commit & push / sync; merge a branch with `--no-commit --no-ff`; create a branch from a ref; refresh.
  Zed's native look, VS Code's workflow. `git_panel.bolt_panel: false` brings back Zed's git panel.
- **Merge conflicts**: Accept Current / Incoming / Both Changes, conflicts marked in the scrollbar.
- **Sidebar search**: VS Code-style search and replace panel with live results.
- **File nesting** in the project panel (antfu's patterns).
- **Footer**: project name, branch and repository button.
- **Iconify previews**: icon names such as `ph:bed` or `i-lucide-check` show as the icon itself, read from the
  project's `node_modules` (setting `iconify`).
- **Theme and icons**: One Dark Pro (syntax colors generated from the VS Code theme with
  `script/bolt-theme-from-vscode`, LSP semantic tokens on) and Material Icon Theme by default; macOS system UI font.
- **Editor details**: VS Code-style smooth cursor blinking, VS Code wording for merge conflicts, no extension
  suggestion popups.
- **Minimal**: no AI, edit predictions, collaboration, sign-in, telemetry, auto-update, debugger, remote development
  or REPL.
- **Isolated** from an installed Zed: own bundle ID (`com.spadari.bolt`), config in `~/.config/bolt`, data in
  `~/Library/Application Support/Bolt`.

### Building

Requirements: Rust (see `rust-toolchain.toml`), Xcode with the Metal toolchain
(`xcodebuild -downloadComponent MetalToolchain`).

- Development: `cargo build`, then `./target/debug/bolt <path>`. About 10 min from scratch, seconds afterwards.
- Release app: `script/bolt-bundle-mac` builds `target/release/Bolt.app`. About 30–40 min from scratch.
- Command line: `bolt <path>` after "Install CLI" from the command palette (links `/usr/local/bin/bolt`, never touches
  Zed's `zed`).

More on building Zed: [macOS](./docs/src/development/macos.md).

### Staying up to date with Zed

Bolt follows Zed's stable releases by merging release tags. The process and the rules that keep merges cheap are in
[`notes/upstream-sync.md`](./notes/upstream-sync.md). Changes to Zed's own files are marked `BOLT:` in the code.

### Licensing

Bolt is a derivative of Zed by Zed Industries, Inc., and keeps its licenses: GPL-3.0-or-later, with Apache-2.0
components where marked (see `LICENSE-GPL` and `LICENSE-APACHE`). Bolt is not affiliated with or endorsed by
Zed Industries.
