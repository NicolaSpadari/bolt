# VS Code git extension: scm/title "..." menu and command behaviour

Source: microsoft/vscode `main`, `extensions/git/` (`src/commands.ts`, `src/repository.ts`, `src/git.ts`,
`src/cloneManager.ts`, `package.json`, `package.nls.json`). Read 2026-10-15.

Conventions used below:
- "pickRef" = `createQuickPick` with `placeholder`, `sortByLabel = false`, `busy` while items load.
- Ref items (`RefItem`): label `$(git-branch) name` (local), `$(cloud) origin/x` (remote), `$(tag) name` (tag);
  description = relative commit date if `git.showReferenceDetails`, else short hash / `Remote branch at {hash}` /
  `Tag at {hash}`; detail (only with ref details) `author • hash • message`.
  `BranchItem` description = `{behind}↓ {ahead}↑` (+ `•` date) or short hash.
- Groups are introduced by separators labelled `branches`, `remote branches`, `tags` (only when non-empty).
- `RefItemsProcessor` always drops `origin/HEAD`; with `skipCurrentBranch` drops HEAD branch name, with
  `skipCurrentBranchRemote` drops `<upstream.remote>/<upstream.name>`.
- Uncaught git errors go to a generic handler: `Git: <first stderr line>` error notification with buttons
  `Open Git Log` / `Show Command Output`; special texts for some codes (see bottom).
- `Repository.run(...)` wraps every op (progress in SCM view + status bar).

## scm/title menu (single git repository, `scmProvider == git`)

Inline (navigation): `Commit` (git.commit, check icon), `Refresh` (git.refresh).

"..." menu, in order:

1. Group `1_header`: `Pull`, `Push`, `Clone`, `Checkout to...`, `Fetch`
2. Group `2_main` submenus:
   - **Commit**: `Commit`, `Commit Staged`, `Commit All`, `Undo Last Commit`, `Abort Rebase` |
     (if `git.allowNoVerifyCommit`) `Commit (No Verify)`, `Commit Staged (No Verify)`, `Commit All (No Verify)` |
     `Commit (Amend)`, `Commit Staged (Amend)`, `Commit All (Amend)` |
     (noVerify) `Commit (Amend, No Verify)`, `Commit Staged (Amend, No Verify)`, `Commit All (Amend, No Verify)` |
     `Commit (Signed Off)`, `Commit Staged (Signed Off)`, `Commit All (Signed Off)` |
     (noVerify) `Commit (Signed Off, No Verify)`, `Commit Staged (Signed Off, No Verify)`, `Commit All (Signed Off, No Verify)`
   - **Changes**: `Stage All Changes`, `Unstage All Changes`, `Discard All Changes`
   - **Pull, Push**: `Sync`, `Sync (Rebase)` (gitState == idle) | `Pull`, `Pull (Rebase)`, `Pull from...` |
     `Push`, `Push (Force)` (if `git.allowForcePush`), `Push to...`, `Push to... (Force)` (if allowForcePush) |
     `Fetch`, `Fetch (Prune)`, `Fetch From All Remotes`
   - **Branch**: `Merge...`, `Rebase Branch...` | `Create Branch...`, `Create Branch From...` |
     `Rename Branch...`, `Delete Branch...`, `Delete Remote Branch...` | `Publish Branch...`
   - **Remote**: `Add Remote...`, `Remove Remote`
   - **Stash**: `Stash`, `Stash (Include Untracked)`, `Stash Staged` (git >= 2.35) | `Apply Latest Stash`, `Apply Stash...` |
     `Pop Latest Stash`, `Pop Stash...` | `Drop Stash...`, `Drop All Stashes...` | `View Stash...`
   - **Tags**: `Create Tag...`, `Delete Tag...`, `Delete Remote Tag...` | `Push Tags`
   - **Worktrees**: for a normal repo (`scmProviderContext == repository`) only `Create Worktree...`.
     For a worktree repo: `Open Worktree in Current Window`, `Open Worktree in New Window` | `Delete Worktree` (git.deleteWorktree2).
3. Group `3_footer`: `Show Git Output`

(`|` = separator between groups.)

---

## git.pull ("Pull")
- No remotes → warning `Your repository has no remotes configured to pull from.`
- Else `repository.pull(HEAD)`: if HEAD has upstream → `git pull <upstream.remote> <upstream.name>`, else bare `git pull`.
- Wrapped in `maybeAutoStash`; flags: `--tags` if `git.pullTags` (default true), `--autostash` if `git.autoStash` (git >= 2.27).
- If `git.fetchOnPull` → `git fetch --all` first. Rebase check (`git log --oneline --cherry b...b@{upstream}`) may prompt
  unless `git.ignoreRebaseWarning`. Tag conflicts handled with a prompt. Conflict → warning
  `There are merge conflicts. Please resolve them before committing your changes.` + `Show Changes`.

## git.pullRebase ("Pull (Rebase)")
Same as pull but `git pull -r [remote branch]`.

## git.push ("Push")
`_push(pushType: Push)`:
- No remotes → (non-modal) warning `Your repository has no remotes configured to push to.` button `Add Remote` → git.addRemote.
- HEAD detached → warning `Please check out a branch to push to a remote.`
- With upstream: `git push <remote> <head>:<upstream.name>`; without upstream: `git push` → fails with NoUpstreamBranch →
  modal `The branch "{0}" has no remote branch. Would you like to publish this branch?` [`OK`, `OK, Don't Ask Again`]
  (skipped if previously "don't ask") → git.publish (1 remote: `git push -u <remote> <branch>`; several: quick pick
  `Pick a remote to publish the branch "{0}" to:` with remotes (label name, description pushUrl) + `$(plus) Add a new remote...`).
- Rejected → `Can't push refs to remote. Try running "Pull" first to integrate your changes.`

## git.pushForce ("Push (Force)")
Same as push with force:
- If `git.allowForcePush` false → error `Force push is not allowed, please enable it with the "git.allowForcePush" setting.`
- Mode: `--force-with-lease` if `git.useForcePushWithLease` (default true), plus `--force-if-includes` if
  `git.useForcePushIfIncludes` (git >= 2.30); else `--force`.
- If `git.confirmForcePush` (default true): modal warning
  `You are about to force push your changes, this can be destructive and could inadvertently overwrite changes made by others.\n\nAre you sure to continue?`
  buttons [`OK`, `OK, Don't Ask Again`].
- Lease rejected → `Can't force push refs to remote. The tip of the remote-tracking branch has been updated since the last checkout. Try running "Pull" first to pull the latest changes from the remote branch first.`

## git.clone ("Clone")
- `pickRemoteSource` quick pick (git-base) with entries `Clone from URL` and `Clone from {provider}` (e.g. GitHub).
  Input `git clone <url>` prefix is stripped.
- If the URL is in the repository cache: quick pick `Open Existing Repository Clone` listing known folders + `Clone again`.
- Folder dialog: title `Choose a folder to clone {url} into`, button `Select as Repository Destination`, default
  `git.defaultCloneDirectory` or home.
- Progress notification `Cloning git repository "{url}"...` (cancellable). Runs `git clone <url> <dir> --progress`.
- Afterwards (per `git.openAfterClone`, default `prompt`): modal info `Would you like to open the repository?`
  [`Open`, `Open in New Window`] (+ `Add to Workspace`, message `...or add it to the current workspace?` if a workspace is open).

## git.checkout ("Checkout to...")
Quick pick, placeholder `Select a branch or tag to checkout`, `sortByLabel=false`, `matchOnDetail=false`.
- Items when filter empty: commands first: `$(plus) Create new branch...`, `$(plus) Create new branch from...`,
  `$(debug-disconnect) Checkout detached...`; then `branches` (local, protected ones labelled `$(lock) name`),
  `remote branches`, `tags` (per `git.checkoutType`, default all). Only `origin/HEAD` excluded; the current branch is NOT excluded.
- When the user types: matching refs first, separator, then the command items (so "Create new branch..." uses the typed text as name).
- Remote items get item buttons from remote source providers (e.g. open on GitHub).
- Actions: local → `git checkout -q <name>` (fast-forward first if `git.pullBeforeCheckout`); remote → if a local branch
  tracks it, checkout that, else `git checkout -q --track <origin/x>`; tag → `git checkout -q <tag>` (detached).
- Create new branch... → git.branch flow with typed value as default name (no input box if value given: name is sanitised directly).
- Create new branch from... → ref picker then name. Checkout detached... → same picker, placeholder
  `Select a branch to checkout in detached mode`, no tags, `git checkout -q --detach <commit>`.
- Dirty tree error → modal warning `Your local changes would be overwritten by checkout.` buttons
  [`Stash & Checkout`, `Migrate Changes`, `Force Checkout`]. Force = discard all then checkout; Stash = stash (with
  untracked, prompts stash message) then checkout; Migrate = stash, checkout, `stash pop`.
- Branch used by another worktree → modal `Branch "{0}" is already checked out in the worktree at "{1}".`
  [`Open Worktree in Current Window`, `Open Worktree in New Window`].

## git.fetch ("Fetch")
- No remotes → warning `This repository has no remotes configured to fetch from.`
- 1 remote → `git fetch` (+`--prune` if `git.pruneOnFetch`).
- Several → quick pick `Select a remote to fetch`: `$(cloud) <name>` (description fetchUrl), upstream remote moved to top,
  separator, `$(cloud-download) Fetch all remotes`. Remote → `git fetch <remote>`; all → `git fetch --all`.

## git.commit ("Commit")
Uses SCM input box message. `smartCommit`:
1. Dirty editors (per `git.promptToSaveFilesBeforeCommit`, default `always`): modal
   `The following file has unsaved changes which won't be included in the commit if you proceed: {0}.\n\nWould you like to save it before committing?`
   (or `There are {0} unsaved files.\n\nWould you like to save them before committing?`) [`Save All & Commit Changes`, `Commit Changes`].
2. Unstaged changes but nothing staged, smart commit off, `git.suggestSmartCommit` on: modal
   `There are no staged changes to commit.\n\nWould you like to stage all your changes and commit them directly?` [`Yes`, `Always`, `Never`].
3. No changes at all: info (non-modal) `There are no changes to commit.` [`Create Empty Commit`].
4. Empty input box message → input box placeholder `Message (commit on "{branch}")`, prompt `Please provide a commit message`.
5. Protected branch (`git.branchProtection`) with `branchProtectionPrompt=alwaysPrompt`: modal
   `You are trying to commit to a protected branch. How would you like to proceed?` [`Commit to a New Branch`, `Commit Anyway`].
6. `git commit --quiet [--all] --file -` (message via stdin), `--amend`, `--signoff`, `-S`, `--no-verify`, `--allow-empty` as applicable.
Commit Staged = `all: false`; Commit All = `all: true`.

## git.undoCommit ("Undo Last Commit")
```ts
if (!HEAD || !HEAD.commit) { showWarningMessage('Can\'t undo because HEAD doesn\'t point to any commit.'); return; }
const commit = await repository.getCommit('HEAD');
if (commit.parents.length > 1) { // modal: 'The last commit was a merge commit. Are you sure you want to undo it?' ['Undo merge commit'] }
if (commit.parents.length > 0) await repository.reset('HEAD~');     // git reset --soft HEAD~
else { await repository.deleteRef('HEAD'); await this.unstageAll(repository); } // git update-ref -d HEAD; unstage
repository.inputBox.value = commit.message;   // message restored into SCM input
```
No confirmation for a normal commit. Changes stay staged (soft reset).

## git.stageAll ("Stage All Changes")
Working tree + untracked resources → `git add -A -- <paths>`; if `git.untrackedChanges` != `mixed` uses `{update: true}`
(`git add -u`). No prompt.

## git.unstageAll ("Unstage All Changes")
`repository.revert([])` → `git reset -q HEAD -- .` (if no branches yet: `git rm --cached -r -- .`). No prompt.

## git.sync ("Sync")
- No upstream → behaves like Push (publish prompt).
- If `git.confirmSync` (default true) and remote not read-only: modal
  `This action will pull and push commits from and to "{remote}/{branch}".` [`OK`, `OK, Don't Show Again`].
- Pull (`git pull [--tags] [-r if git.rebaseWhenSync] <remote> <upstream>`), then push `git push <remote> <head>:<upstream>`
  (with `--follow-tags` if `git.followTagsWhenSync`) only if ahead > 0. Errors matching "Cancelled" swallowed.
- Sync (Rebase) forces `rebase=true`.

## git.merge ("Merge...")
pickRef placeholder `Select a branch or tag to merge from`; items `branches`, `remote branches`, `tags` (BranchItem
style with ahead/behind); excludes current branch, its upstream, `origin/HEAD`. Runs `git merge <name>`; conflicts →
merge-conflict warning + `Show Changes`.

## git.branch ("Create Branch...")
No ref picker; target = `HEAD`. Name input (`promptForBranchName`):
- `createInputBox`, placeholder `Branch name`, prompt `Please provide a new branch name`, `ignoreFocusOut`.
- Initial value = random name if `git.branchRandomName.enable` else `git.branchPrefix` (cursor after prefix);
  refresh button `Regenerate Branch Name` when random names enabled.
- Live validation: existing branch → `Branch "{0}" already exists`; regex `git.branchValidationRegex` fail →
  `Branch name needs to match regex: {0}`; if sanitised differs → info `The new branch will be "{0}"`.
- Sanitisation: spaces/invalid chars → `git.branchWhitespaceChar` (default `-`).
- Runs `git checkout -q -b <name> --no-track HEAD` (creates and switches).

## git.branchFrom ("Create Branch From...")
`showQuickPick` placeholder `Select a ref to create the branch from`; items: `HEAD` (description short hash, alwaysShow),
then `branches`, `remote branches`, `tags` (only `origin/HEAD` excluded). Then same name input; runs
`git checkout -q -b <name> --no-track <ref>`.

## git.renameBranch ("Rename Branch...")
Same `promptForBranchName` input box, pre-filled with the current branch name (the "already exists" validation fires on
the unchanged name). Only the current branch can be renamed: `git branch -m <newName>`. Errors:
`Invalid branch name`, `A branch named "{0}" already exists`. Empty/escape → nothing.

## git.deleteBranch ("Delete Branch...")
pickRef placeholder `Select a branch to delete`; items: local branches only (`branches` separator, BranchItem with
ahead/behind), excluding the current branch. No up-front confirmation. Runs `git branch -d <name>`, then
`git config --local --unset branch.<name>.vscode-merge-base`.
```ts
} catch (err) {
  if (err.gitErrorCode !== GitErrorCodes.BranchNotFullyMerged) throw err;   // stderr /branch '.+' is not fully merged/
  const message = l10n.t('The branch "{0}" is not fully merged. Delete anyway?', name);
  const yes = l10n.t('Delete Branch');
  const pick = await window.showWarningMessage(message, { modal: true }, yes);
  if (pick === yes) await run(true);   // git branch -D <name>
}
```
Delete Remote Branch...: placeholder `Select a remote branch to delete`, remote branches minus current upstream,
`git push <remote> --delete <branch>`.

## git.addRemote ("Add Remote...")
1. `pickRemoteSource` with `Add remote from URL` / `Add remote from {provider}`.
2. Input box placeholder `Remote name`, prompt `Please provide a remote name`; validation `Remote name format invalid`,
   `Remote "{0}" already exists.`. Name sanitised (invalid chars → `-`).
3. `git remote add <name> <url>` then `git fetch <name>`.

## git.removeRemote ("Remove Remote")
- No remotes → error `Your repository has no remotes.`
- `showQuickPick` placeholder `Pick a remote to remove`, items `$(cloud) <name>` description fetchUrl.
- `git remote remove <name>`. No confirmation.

## Stash family
Stash item (`StashItem`): label `#{index}: {description}` (description = stash message, e.g. `On main: msg`),
description = `{relative date} • {branch}` (date + branch name when available). Picker `pickStash` is `showQuickPick`;
empty → single item `$(info) This repository has no stashes.` (selecting it does nothing).

- **git.stash ("Stash")**: no changes → info `There are no changes to stash.` Dirty editors → modal
  `...won't be included in the stash if you proceed: {0}.\n\nWould you like to save it before stashing?` [`Save All & Stash`, `Stash Anyway`].
  Then input box prompt `Optionally provide a stash message`, placeholder `Stash message` (pre-filled with SCM input if
  `git.useCommitInputAsStashMessage`); Escape cancels, empty is OK. `git stash push [-m msg]`. No commits yet → info
  `The repository does not have any commits. Please make an initial commit before creating a stash.`
- **git.stashIncludeUntracked**: same, `git stash push -u`; "no changes" also considers untracked.
- **git.stashStaged**: requires staged changes (`There are no staged changes to stash.`), `git stash push -S`.
- **git.stashApplyLatest**: none → info `There are no stashes in the repository.`; `git stash apply`.
- **git.stashApply**: picker `Pick a stash to apply`; `git stash apply stash@{n}`.
- **git.stashPopLatest**: none → same info; `git stash pop`.
- **git.stashPop**: picker `Pick a stash to pop`; `git stash pop stash@{n}`.
  Apply/pop conflict → warning `There are merge conflicts while applying the stash. Please resolve them before committing your changes.` [`Show Changes`].
- **git.stashDrop**: picker `Pick a stash to drop`, then modal `Are you sure you want to drop the stash: {description}?` [`Yes`];
  `git stash drop stash@{n}`.
- **git.stashDropAll**: none → info; modal `Are you sure you want to drop ALL stashes? There is 1 stash that will be subject to pruning, and MAY BE IMPOSSIBLE TO RECOVER.`
  (plural: `There are {0} stashes ...`) [`Yes`]; `git stash clear`.
- **git.stashView**: picker `Pick a stash to view`; `git stash show stash@{n} --name-status -z -u`; opens multi-diff editor
  titled `Git Stash #{n}: {description}` (parent1 vs stash, untracked from 3rd parent).

## git.createTag ("Create Tag...")
Input 1: placeholder `Tag name`, prompt `Please provide a tag name`. Input 2: placeholder `Message`, prompt
`Please provide a message to annotate the tag` (Escape/empty = lightweight). Name sanitised. `git tag -a <name> -m <msg>`
or `git tag <name>` (on HEAD).

## git.deleteTag ("Delete Tag...")
pickRef placeholder `Select a tag to delete`; items `$(tag) name` (description `Tag at {hash}` / date); none →
`$(info) This repository has no tags.`. `git tag -d <name>`, no confirmation.

## git.createWorktree ("Create Worktree...")
1. pickRef `Select a branch or tag to create the new worktree from`: `$(plus) Create new branch...`, separator,
   `branches`, `remote branches`, `tags`.
2. New branch → branch name input (as git.branch), commitish `HEAD`. Current branch picked → modal
   `Branch "{0}" is already checked out in the current repository.` [`Create New Branch`]. Branch used by another worktree →
   modal `Branch "{0}" is already checked out in the worktree at "{1}".` [`Open Worktree in Current Window`, `Open Worktree in New Window`].
3. Path input: placeholder `Worktree path`, prompt `Please provide a worktree path`, default
   `<parent>/<repo>.worktrees/<name>` (or last-used root), folder button `Select Worktree Destination` (dialog
   `Select as Worktree Destination`); warning validation `A worktree already exists at "{0}".`
4. `git worktree add [-b <branch>] <path> <commitish>`.

## git.deleteWorktree (palette) / git.deleteWorktree2 (menu, inside a worktree)
- Palette: pickRef `Select a worktree to delete`; items `$(list-tree) name`, description `branch • hash • subject`, detail path;
  none → `$(info) This repository has no worktrees.`
- Menu (worktree repo): deletes the current worktree via the main repo; if main repo not open → modal
  `You cannot delete the worktree you are currently in. Please switch to the main repository first.`
- `git worktree remove <path>`; dirty → modal `The worktree contains modified or untracked files. Do you want to force delete?`
  (detail `Worktree: {path}`) [`Force Delete`] → `git worktree remove --force <path>`.
- Open Worktree in Current/New Window → `vscode.openFolder` (reuse / new window).

## Generic error texts (commands.ts createCommand)
- DirtyWorkTree: `Please clean your repository working tree before checkout.`
- PushRejected, ForcePushWithLease*: see push.
- Conflict / StashConflict / CherryPickConflict: warnings with `Show Changes`.
- NoUserName/Email: `Make sure you configure your "user.name" and "user.email" in git.` [`Learn More`].
- Default: `Git: <line>` + [`Open Git Log`, `Show Command Output`].
