# Upstream sync — keeping Bolt on top of new Zed releases

Goal: update the Zed base every few weeks without losing Bolt changes, with low effort and an always-available rollback.

## Strategy (decided 2026-10-14, branch model updated 2026-10-15 by the user)
- **Two branches.** `main` = plain Zed: only fast-forwarded to Zed stable tags, never committed to. `bolt` = Bolt
  (GitHub default branch). `git diff main..bolt` = everything Bolt changes.
- **Merge, not rebase, and never cherry-pick upstream.** Each sync is one merge commit `Merge Zed vX.Y.Z into Bolt`
  on `bolt`; the whole release comes in (unwanted subsystems stay off via Cargo features / settings). Conflicts are
  resolved once per sync; rebase would re-resolve them for every Bolt commit and rewrite published history.
- **Sync to stable release tags, not `upstream/main`.** Tags are tested by Zed; `main` can be broken any day.
- **Cadence: every 2–4 weeks / every stable minor.** Small gaps = small conflicts.
- **`git rerere` on** (`rerere.enabled`, `rerere.autoupdate`): known conflict resolutions are replayed.
- **Never on `bolt` directly.** The merge happens on `sync/vX.Y.Z` (from `bolt`); `bolt` only fast-forwards after
  build + checks pass. Before that, `bolt` is tagged `bolt-pre-vX.Y.Z` → rollback is `git reset --hard <tag>`.

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

## Procedure (scripted: `script/bolt-sync`, `--help` for the steps)
```
git fetch upstream --tags
NEW=v1.24.0                                   # latest stable tag, no -pre
git switch main && git merge --ff-only $NEW   # main = plain Zed
git tag bolt-pre-$NEW bolt                    # rollback point
git switch -c sync/$NEW bolt
git merge --no-ff main -m "Merge Zed $NEW into Bolt"
#   conflicts → resolve (rerere replays known ones), recipes above
cargo build && ./script/clippy -p 'bolt_*' -p zed && cargo test -p zed --no-run
cargo check -p zed --features ai,collab,repl,debugger
#   manual smoke run with stderr captured; re-check notes/rebrand-map.md for new "Zed" strings
echo $NEW > .bolt-zed-version
git switch bolt && git merge --ff-only sync/$NEW && git branch -d sync/$NEW
git push origin main bolt --tags
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
