# Bolt (local agent rules — this file does not exist upstream, keep Bolt rules here, never in Zed's CLAUDE.md)

- Read `HANDOVER.md` and `PROGRESS.md` first; they hold plan, decisions and state.
- Before touching any upstream (non-`crates/bolt_*`) file, follow the rules in `notes/upstream-sync.md` and
  `notes/minimal-strip.md`, mark the edit with `// BOLT:` and list it in PROGRESS "Upstream touch points".
- Answer the user in Italian, concisely.
- The `.rules` HARD RULE that prepends the `> [!IMPORTANT]` / `> Remove this line…` review marker to `README.md` is
  for upstream Zed PRs and does not apply to Bolt: never add those lines (the user reviews by asking for a debug run).

## Disk hygiene (`target/`)
Cargo never deletes stale artifacts: every hash change (Cargo.lock, features, `-p` vs workspace, RUSTFLAGS, toolchain)
adds a new copy to `target/debug/deps` (it reached 140 GB). `cargo-sweep` is installed globally — use it instead of `cargo clean`:
- `cargo sweep --stamp` → run the usual build/test/clippy → `cargo sweep --file` (removes everything those builds did not use)
- `cargo sweep --time 7` (unused for 7 days), `cargo sweep --installed` (after `rustup update`), `cargo sweep --maxsize 30GB`
- Never remove `target/debug` wholesale unless asked: incremental builds are wanted.
