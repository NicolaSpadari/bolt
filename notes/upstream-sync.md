# Upstream sync — keeping Bolt on top of new Zed releases

Goal: update the Zed base every few weeks without losing Bolt changes, with low effort and an always-available rollback.

## Strategy (decided 2026-10-14, proposal pending user confirmation)
- **Merge, not rebase.** `main` = Bolt. Each sync is one merge commit `Merge Zed vX.Y.Z into Bolt`. Conflicts are
  resolved once per sync (rebase would replay every Bolt commit onto a moving base, re-resolving the same conflicts and
  rewriting published history).
- **Sync to stable release tags, not `upstream/main`.** Zed tags stable releases `vX.Y.Z` (preview = `vX.Y.Z-pre`).
  Tags are tested by Zed; `main` can be broken any day. Tags also give a precise "Bolt is based on Zed vX.Y.Z" record.
- **Cadence: every 2–4 weeks / every stable minor.** Small gaps = small conflicts. Skipping many releases is the main
  way this becomes expensive.
- **`git rerere` on** (`git config rerere.enabled true`, `rerere.autoupdate true`): git remembers how a conflict was
  resolved and reapplies it automatically next time.
- **Never on `main` directly.** The merge happens on a throwaway branch `sync/vX.Y.Z`; `main` only fast-forwards after
  build + checks pass. Before that, `main` is tagged `bolt-base-<old version>` → rollback is `git reset --hard <tag>`.

## Design rules that keep merges cheap (apply while coding)
1. New code only in `crates/bolt_*` (upstream never touches them → never conflicts).
2. Upstream files: touch as little as possible, prefer **additive** edits (a new line/registration call) over changing
   existing lines; mark each with `// BOLT:` so `git grep "BOLT:"` finds them all.
3. Every upstream touch is listed in PROGRESS "Upstream touch points". The sync script compares this list with the real
   diff (`git diff <zed-tag>..main --name-only -- . ':!crates/bolt_*' ':!notes' ':!PROGRESS.md' ':!HANDOVER.md'`);
   anything not listed is a bug in the bookkeeping.
4. Branding (name, bundle ID, paths) centralised in as few upstream spots as possible (constants, `release_channel`,
   `paths`, bundle script) — it is the largest unavoidable diff.
5. Don't fork Zed behaviour when a setting/hook exists; don't reformat or refactor upstream code.
6. Known always-conflicting files and their recipe:
   - `Cargo.toml` (explicit `members` list, ~270 lines): keep upstream version, re-add the `crates/bolt_*` lines.
   - `Cargo.lock`: take upstream (`git checkout --theirs Cargo.lock`), then `cargo build` re-adds Bolt entries.
   - `rust-toolchain.toml`: always take upstream (rustup follows automatically).

## Procedure (to be scripted as `script/bolt-sync` in P0)
```
git fetch upstream --tags
NEW=v1.23.3                                   # latest stable tag, no -pre
git tag bolt-base-$(cat .bolt-zed-version) main   # rollback point
git switch -c sync/$NEW main
git merge --no-ff $NEW -m "Merge Zed $NEW into Bolt"
#   conflicts → resolve (rerere replays known ones), recipes above
cargo build && cargo clippy -p 'bolt_*' && cargo test -p 'bolt_*'
#   manual smoke run: Bolt panels + side-by-side with installed Zed
echo $NEW > .bolt-zed-version
git switch main && git merge --ff-only sync/$NEW && git branch -d sync/$NEW
```
Then log the sync in PROGRESS (from → to, conflicts hit, time spent).

## Effort estimate
- Typical sync with the rules above: 15–60 min, mostly build time (Zed incremental build after a version bump is long).
- Real risk is not merge conflicts but **API drift**: upstream renames/refactors in `git_ui`, `project`, `workspace`,
  GPUI → Bolt crates fail to compile. Mitigation: depend on stable public APIs, keep Bolt crates thin wrappers over
  `GitStore`/`Project`, sync often.

## Optional later
- GitHub Action (if the fork is on GitHub): on each new stable tag, try the merge in CI and open a PR / issue with the
  conflict list. Turns syncing into "review a PR".
