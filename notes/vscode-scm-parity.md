# VS Code Source Control → Bolt parity checklist

Target: behave like VS Code's built-in Git SCM view (`extensions/git` + `src/vs/workbench/contrib/scm` in microsoft/vscode).
"Zed today" is filled during the P1 gap analysis (✅ exists / 🟡 partial / ❌ missing). Status uses the PROGRESS legend.

| # | Area | VS Code behavior | Zed today | Bolt |
|---|---|---|---|---|
| 1 | Entry | Activity bar icon with badge = number of changes; `ctrl-shift-g` opens/focuses | | ⏳ |
| 2 | Repositories | "Source Control Repositories" section when >1 repo; select which repos are shown; per-repo branch + sync | | ⏳ |
| 3 | Commit input | Multi-line box at top; placeholder `Message (⌘⏎ to commit on "<branch>")`; up/down = message history | | ⏳ |
| 4 | Input validation | Warn on subject length (`git.inputValidation`, 50/72) with inline hint | | ⏳ |
| 5 | Commit button | Primary button + dropdown: Commit, Commit (Amend), Commit & Push, Commit & Sync | | ⏳ |
| 6 | Smart commit | Nothing staged → ask / auto-stage all (`git.enableSmartCommit`) | | ⏳ |
| 7 | Action button | `actionButton.ts`: Commit when there are changes (or a merge to conclude) → else Publish Branch when the branch has no upstream → else Sync Changes n↓ m↑ when ahead/behind → else Commit (disabled); spinner while syncing. Sync = pull upstream, push `<branch>:<upstream>` | | ✅ |
| 8 | Groups | Merge Changes, Staged Changes, Changes, Untracked (`git.untrackedChanges`: mixed / separate / hidden); counts; collapsible | | ⏳ |
| 9 | Row | File icon, name, dimmed relative dir, status letter (M A D U R C !) colored, strikethrough for deleted | | ⏳ |
| 10 | Row inline actions (hover) | Open File, Discard Changes, Stage (+) / Unstage (−) | | ⏳ |
| 11 | Group inline actions | Stage All, Unstage All, Discard All (with confirm) | | ⏳ |
| 12 | Multi-select | cmd/shift-click, actions apply to selection | | ⏳ |
| 13 | View mode | List ↔ Tree toggle (`scm.defaultViewMode`); tree compacts single-child folders; folder-level stage/discard | | ⏳ |
| 14 | Sort | By name / path / status (list mode) | | ⏳ |
| 15 | Click | Opens diff (HEAD↔working or index↔working by group); preview vs pinned tab on double-click | | ⏳ |
| 16 | Row context menu | Open Changes, Open File, Open File (HEAD), Stage/Unstage, Discard, Stash file, Add to .gitignore, Reveal in Explorer/Finder, Copy Path/Relative Path | | ⏳ |
| 17 | Title bar | Commit ✓, Refresh, view toggle, "…" menu | | ⏳ |
| 18 | "…" menu | Pull, Push, Clone, Checkout to…, Fetch; submenus Commit, Changes, Pull/Push, Branch (create/rename/delete/merge/rebase/publish), Remote (add/remove), Stash (stash, incl. untracked, apply/pop latest/pick, drop, view), Tags (create/delete), Worktrees; Show Git Output | | ⏳ |
| 19 | Merge conflicts | Merge Changes group; open in merge editor / accept current/incoming; stage = mark resolved | | ⏳ |
| 20 | Status bar | `statusbar.ts`: branch name → checkout picker; with upstream: sync icon + "n↓ m↑" when ahead/behind; without: cloud-upload (Publish Branch). "Create Branch From…" uses `git checkout -q -b <name> --no-track <ref>` (`git.ts`) | | ✅ (no status bar spinner: not wanted) |
| 21 | Partial staging | Stage/revert selected ranges and hunks from diff editor and gutter | | ⏳ |
| 22 | Keyboard | Full keyboard navigation in the list; `cmd-enter` commit from input | | ⏳ |
| 23 | Git output | Log of executed git commands | | ⏳ |
| 24 | Graph | Incoming/Outgoing graph | ✅ (Zed git graph) | ⛔ reuse Zed |

Notes
- AI "generate commit message": out of scope (Bolt has no AI, see `minimal-strip.md`).
- User spec (2026-10-14, screenshots): header "CHANGES" with icons (view as tree/list, commit ✓, refresh ↻, "…");
  commit message is a single-line input (not a textarea) with placeholder `Message (⌘⏎ to commit on "<branch>")`;
  full-width "✓ Commit" button with a dropdown chevron; sections "Staged Changes" / "Changes" with counts; rows: file
  icon, name, hover actions open file / discard ↺ / stage + (unstage − in Staged), colored status letter; hovering a
  row shows a tooltip with the full path + status ("Index Modified"). "…" menu as VS Code: View & Sort ›, Pull, Push,
  Clone, Checkout to…, Fetch, Commit ›, Changes ›, Pull/Push ›, Branch ›, Remote ›, Stash › (Stash, Stash (Include
  Untracked), Stash Staged, Apply Latest Stash, Apply Stash…, Pop Latest Stash, Pop Stash…, Drop Stash…, Drop All
  Stashes…, View Stash…), Tags ›, Worktrees ›, Show Git Output.
- Clicking the branch (status bar) opens a picker with "Create new branch…", "Create new branch from…", "Checkout
  detached…", then branches with relative date and last commit.
- Git graph: keep Zed's, but off by default (user does not use it).
- Before implementing, verify each VS Code behavior on a real VS Code install (user has it) rather than from memory.
