//! BOLT: VS Code-style Source Control panel (left dock), replacing Zed's git panel in the UI.
//!
//! Zed's `GitPanel` stays loaded (without a dock button) because Zed's workspace-level git actions (stash, uncommit,
//! …) are routed through it; this panel dispatches those actions and calls the repository directly for staging,
//! discarding, committing, fetch, pull and push. The commit message lives in the repository's shared commit buffer,
//! so it is the same text in both panels. `git_panel.bolt_panel: false` shows Zed's panel instead.
//! Spec: `notes/vscode-scm-parity.md`.

mod merge_picker;
mod stash_picker;

use std::{ops::Range, path::Path, sync::Arc, time::Duration};

use anyhow::Result;
use askpass::AskPassDelegate;
use editor::{Editor, EditorElement, EditorEvent, EditorMode, EditorStyle};
use file_icons::FileIcons;
use git::{
    repository::{
        CommitOptions, FetchOptions, PushOptions, Remote, RemoteCommandOutput, RepoPath,
        UpstreamTracking,
    },
    status::FileStatus,
};
use git_ui::{
    git_panel::GitStatusEntry,
    remote_output::{self, RemoteAction, SuccessMessage, SuccessStyle},
    solo_diff_view::SoloDiffView,
};
use git_ui_core::{askpass_modal::AskPassModal, notifications::show_error_toast};
use gpui::{
    Action, AnimationExt as _, AnyElement, App, AsyncWindowContext, ClickEvent, Context, Entity,
    EventEmitter, FocusHandle, Focusable, KeyContext, Pixels, PromptLevel, Render, SharedString,
    Subscription, Task, UniformListScrollHandle, WeakEntity, Window, actions, px, uniform_list,
};
use notifications::status_toast::StatusToast;
use project::{
    Project,
    git_store::{
        GitStoreEvent, Repository, RepositoryEvent,
        pending_op::{GitStatus, JobStatus},
    },
};
use settings::Settings as _;
use settings::StatusStyle;
use theme::ActiveTheme as _;
use theme_settings::ThemeSettings;
use ui::{
    ButtonLike, ButtonStyle, CommonAnimationExt as _, ContextMenu, ElevationIndex, Icon,
    IconButton, IconName, IconSize, Label, LabelSize, PopoverMenu, PopoverMenuHandle, SplitButton,
    SplitButtonStyle, Tab, TintColor, Tooltip, prelude::*, rems_from_px,
};
use util::ResultExt as _;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const SCM_PANEL_KEY: &str = "BoltScmPanel";
/// Zed's git job descriptions matching the VS Code operations with `showProgress: true` (`operation.ts`): status
/// refreshes and writes, but not reads such as diffs, blame or `show`.
const PROGRESS_JOBS: &[&str] = &[
    "schedule_scan",
    "paths_changed",
    "stage_or_unstage_entries",
    "spawn_set_index_text_job",
    "commit",
    "run_hook",
    "fetch",
    "pull",
    "push",
    "reset",
    "checkout_files",
    "change_branch",
    "create_branch",
    "delete_branch",
    "rename_branch",
    "branches",
    "stash_entries",
    "stash_staged",
    "stash_apply",
    "stash_pop",
    "stash_drop",
    "create_remote",
    "remove_remote",
    "create_ref",
    "edit_ref",
    "add_path_to_gitignore",
    "add_path_to_git_info_exclude",
];
const MERGE_ICON: &str = "icons/bolt/git_merge.svg";
// Zed's git panel row height.
const LIST_ITEM_HEIGHT: gpui::Rems = gpui::rems(1.75);

actions!(
    bolt_scm,
    [
        /// Toggles focus on the source control panel (Zed's git panel when Bolt's one is disabled).
        ToggleFocus,
        /// Commits the staged changes (all changes when nothing is staged).
        Commit,
        /// Merges a branch into the current one without committing (`git merge --no-commit --no-ff`).
        MergeBranch,
        /// Creates a new branch from HEAD and switches to it.
        CreateBranch,
        /// Creates a new branch from a chosen ref and switches to it.
        CreateBranchFrom,
        /// Deletes a local branch (asks before force-deleting a branch that is not fully merged).
        DeleteBranch,
        /// Deletes a branch on its remote (`git push <remote> :<branch>`).
        DeleteRemoteBranch,
        /// Discards all unstaged changes, untracked files included.
        DiscardAllChanges,
        /// Applies a chosen stash.
        ApplyStash,
        /// Pops a chosen stash.
        PopStash,
        /// Drops a chosen stash.
        DropStash,
        /// Shows the changes of a chosen stash.
        ViewStash,
        /// Pulls the current branch with `--rebase`.
        PullRebase,
        /// Force pushes the current branch (`--force-with-lease`).
        ForcePush,
        /// Pulls (when the branch has an upstream), then pushes the current branch.
        Sync,
        /// Pushes the current branch and sets its upstream.
        PublishBranch,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            if git_ui::bolt_panel_enabled(cx) {
                workspace.toggle_panel_focus::<ScmPanel>(window, cx);
            } else {
                workspace.toggle_panel_focus::<git_ui::git_panel::GitPanel>(window, cx);
            }
        });
        workspace.register_action(|workspace, _: &MergeBranch, window, cx| {
            merge_picker::open(workspace, merge_picker::Mode::Merge, window, cx);
        });
        workspace.register_action(|workspace, _: &CreateBranchFrom, window, cx| {
            merge_picker::open(workspace, merge_picker::Mode::CreateFrom, window, cx);
        });
        workspace.register_action(|workspace, _: &CreateBranch, window, cx| {
            merge_picker::open_new_branch_name(workspace, window, cx);
        });
        workspace.register_action(|workspace, _: &DeleteBranch, window, cx| {
            merge_picker::open(workspace, merge_picker::Mode::Delete, window, cx);
        });
        workspace.register_action(|workspace, _: &DeleteRemoteBranch, window, cx| {
            merge_picker::open(workspace, merge_picker::Mode::DeleteRemote, window, cx);
        });
        workspace.register_action(|workspace, _: &DiscardAllChanges, window, cx| {
            if let Some(panel) = workspace.panel::<ScmPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    let entries = panel
                        .sections
                        .iter()
                        .filter(|section| section.kind == SectionKind::Changes)
                        .flat_map(|section| section.entries.iter().cloned())
                        .collect();
                    panel.discard(entries, window, cx)
                });
            }
        });
        workspace.register_action(|workspace, _: &ApplyStash, window, cx| {
            stash_picker::open(workspace, stash_picker::Mode::Apply, window, cx);
        });
        workspace.register_action(|workspace, _: &PopStash, window, cx| {
            stash_picker::open(workspace, stash_picker::Mode::Pop, window, cx);
        });
        workspace.register_action(|workspace, _: &DropStash, window, cx| {
            stash_picker::open(workspace, stash_picker::Mode::Drop, window, cx);
        });
        workspace.register_action(|workspace, _: &ViewStash, window, cx| {
            stash_picker::open(workspace, stash_picker::Mode::View, window, cx);
        });
        workspace.register_action(|workspace, _: &PullRebase, window, cx| {
            if let Some(panel) = workspace.panel::<ScmPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.run_remote(RemoteOperation::PullRebase, window, cx)
                });
            }
        });
        workspace.register_action(|workspace, _: &ForcePush, window, cx| {
            if let Some(panel) = workspace.panel::<ScmPanel>(cx) {
                panel.update(cx, |panel, cx| panel.force_push(window, cx));
            }
        });
        workspace.register_action(|workspace, _: &Sync, window, cx| {
            if let Some(panel) = workspace.panel::<ScmPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.run_remote(RemoteOperation::Sync, window, cx)
                });
            }
        });
        workspace.register_action(|workspace, _: &PublishBranch, window, cx| {
            if let Some(panel) = workspace.panel::<ScmPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.run_remote(RemoteOperation::Push, window, cx)
                });
            }
        });
    })
    .detach();
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum SectionKind {
    Merge,
    Staged,
    Changes,
}

impl SectionKind {
    fn title(self) -> &'static str {
        match self {
            SectionKind::Merge => "Merge Changes",
            SectionKind::Staged => "Staged Changes",
            SectionKind::Changes => "Changes",
        }
    }
}

#[derive(Clone)]
struct Entry {
    repo_path: RepoPath,
    status: FileStatus,
}

struct Section {
    kind: SectionKind,
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Header(usize),
    Entry(usize, usize),
}

#[derive(Clone, Copy)]
enum AfterCommit {
    Nothing,
    Remote(RemoteOperation),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RemoteOperation {
    Fetch,
    Pull,
    PullRebase,
    Push,
    /// `git push --force-with-lease` (VS Code's default `git.useForcePushWithLease`).
    ForcePush,
    /// Pull (when the branch has an upstream), then push.
    Sync,
}

impl RemoteOperation {
    fn name(self) -> &'static str {
        match self {
            RemoteOperation::Fetch => "fetch",
            RemoteOperation::Pull => "pull",
            RemoteOperation::PullRebase => "pull --rebase",
            RemoteOperation::Push => "push",
            RemoteOperation::ForcePush => "push --force-with-lease",
            RemoteOperation::Sync => "sync",
        }
    }
}

/// What the main button does when there is nothing to commit (VS Code's "Publish Branch" / "Sync Changes").
enum PrimaryAction {
    Commit,
    Publish,
    Sync { ahead: u32, behind: u32 },
}

pub struct ScmPanel {
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    focus_handle: FocusHandle,
    repository: Option<Entity<Repository>>,
    commit_editor: Option<Entity<Editor>>,
    sections: Vec<Section>,
    collapsed: Vec<SectionKind>,
    selected: Option<Row>,
    hovered_row: Option<Row>,
    scroll_handle: UniformListScrollHandle,
    pending_commit: Option<Task<()>>,
    pending_remote: Option<RemoteOperation>,
    commit_menu_handle: PopoverMenuHandle<ContextMenu>,
    open_commit_buffer: Option<Task<()>>,
    _commit_editor_subscription: Option<Subscription>,
    prefilled_merge_message: Option<SharedString>,
    progress_visible: bool,
    hide_progress: Option<Task<()>>,
    _repository_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl ScmPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            Self::new(workspace, window, cx)
        })
    }

    fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let weak_workspace = workspace.weak_handle();
        let workspace_entity = cx.entity();
        cx.new(|cx| {
            let git_store = project.read(cx).git_store().clone();
            let subscription = cx.subscribe_in(
                &git_store,
                window,
                |this: &mut Self, _, event: &GitStoreEvent, window, cx| match event {
                    GitStoreEvent::ActiveRepositoryChanged(_)
                    | GitStoreEvent::RepositoryAdded
                    | GitStoreEvent::RepositoryRemoved(_) => this.set_repository(window, cx),
                    GitStoreEvent::RepositoryUpdated(
                        _,
                        RepositoryEvent::StatusesChanged
                        | RepositoryEvent::HeadChanged
                        | RepositoryEvent::BranchListChanged
                        | RepositoryEvent::StashEntriesChanged,
                        true,
                    ) => {
                        this.update_sections(cx);
                        this.prefill_merge_message(window, cx);
                    }
                    // Ahead / behind counts of the main button, merge message.
                    GitStoreEvent::RepositoryUpdated(..) => {
                        this.prefill_merge_message(window, cx);
                        cx.notify()
                    }
                    _ => {}
                },
            );
            let settings_subscription =
                cx.observe_global::<settings::SettingsStore>(|_, cx| cx.notify());
            let active_item_subscription = cx.subscribe(
                &workspace_entity,
                |this: &mut Self, _, event: &workspace::Event, cx| {
                    if matches!(event, workspace::Event::ActiveItemChanged) {
                        this.reveal_active_item(cx);
                    }
                },
            );
            let mut this = Self {
                workspace: weak_workspace,
                project,
                focus_handle: cx.focus_handle(),
                repository: None,
                commit_editor: None,
                sections: Vec::new(),
                collapsed: Vec::new(),
                selected: None,
                hovered_row: None,
                scroll_handle: UniformListScrollHandle::new(),
                pending_commit: None,
                pending_remote: None,
                commit_menu_handle: PopoverMenuHandle::default(),
                open_commit_buffer: None,
                _commit_editor_subscription: None,
                prefilled_merge_message: None,
                progress_visible: false,
                hide_progress: None,
                _repository_subscription: None,
                _subscriptions: vec![
                    subscription,
                    settings_subscription,
                    active_item_subscription,
                ],
            };
            this.set_repository(window, cx);
            this
        })
    }

    fn set_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repository = self.project.read(cx).active_repository(cx);
        let changed = repository.as_ref().map(|repo| repo.entity_id())
            != self.repository.as_ref().map(|repo| repo.entity_id());
        self.repository = repository.clone();
        if changed {
            self._repository_subscription = repository
                .as_ref()
                .map(|repository| cx.observe(repository, |this, _, cx| this.update_progress(cx)));
            self.update_progress(cx);
        }
        self.update_sections(cx);
        if !changed && self.commit_editor.is_some() {
            return;
        }
        self.commit_editor = None;
        let Some(repository) = repository else {
            cx.notify();
            return;
        };
        let languages = self.project.read(cx).languages().clone();
        let buffer_store = self.project.read(cx).buffer_store().clone();
        let open = repository.update(cx, |repository, cx| {
            repository.open_commit_buffer(Some(languages), buffer_store, cx)
        });
        self.open_commit_buffer = Some(cx.spawn_in(window, async move |this, cx| {
            let Some(buffer) = open.await.log_err() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let project = this.project.clone();
                let editor = cx.new(|cx| {
                    // Zed's git panel editor (no autoclose, Cut / Copy / Paste menu), growing like VS Code's SCM
                    // input: wraps, from `scm.inputMinLineCount` (1) to `scm.inputMaxLineCount` (10) lines, then
                    // scrolls (`scmInput.ts` `getContentHeight`). Placeholder drawn by the panel.
                    let mut editor = git_ui::git_panel::commit_message_editor(
                        buffer,
                        Some(SharedString::default()),
                        project,
                        true,
                        window,
                        cx,
                    );
                    editor.set_mode(EditorMode::AutoHeight {
                        min_lines: 1,
                        max_lines: Some(10),
                    });
                    editor
                });
                // The main button switches between Commit and Sync / Publish as the message is typed.
                this._commit_editor_subscription =
                    Some(cx.subscribe(&editor, |_, _, event: &EditorEvent, cx| {
                        if matches!(event, EditorEvent::BufferEdited) {
                            cx.notify();
                        }
                    }));
                this.commit_editor = Some(editor);
                this.prefill_merge_message(window, cx);
                cx.notify();
            })
            .log_err();
        }));
    }

    fn branch_name(&self, cx: &App) -> Option<String> {
        let repository = self.repository.as_ref()?.read(cx);
        repository
            .branch
            .as_ref()
            .map(|branch| branch.name().to_string())
    }

    fn placeholder(&self, cx: &App) -> String {
        match self.branch_name(cx) {
            Some(branch) => format!("Message (⌘⏎ to commit on \"{branch}\")"),
            None => "Message (⌘⏎ to commit)".to_string(),
        }
    }

    /// Fills the empty commit input with git's merge message (`.git/MERGE_MSG`), like VS Code. Once per merge, so a
    /// message cleared by the user stays cleared.
    fn prefill_merge_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(repository), Some(editor)) = (&self.repository, &self.commit_editor) else {
            return;
        };
        let merge_message = repository.read(cx).merge.message.clone();
        if merge_message == self.prefilled_merge_message {
            return;
        }
        self.prefilled_merge_message = merge_message.clone();
        // The input is single-line: keep the subject, dropping git's `# Conflicts:` comment block.
        let Some(subject) = merge_message.as_ref().and_then(|message| {
            message
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_string)
        }) else {
            return;
        };
        editor.update(cx, |editor, cx| {
            if editor.text(cx).trim().is_empty() {
                editor.set_text(subject, window, cx);
            }
        });
    }

    fn selected_entry(&self) -> Option<(SectionKind, &Entry)> {
        let Some(Row::Entry(section_ix, entry_ix)) = self.selected else {
            return None;
        };
        let section = self.sections.get(section_ix)?;
        Some((section.kind, section.entries.get(entry_ix)?))
    }

    /// VS Code's `scm.autoReveal` (`scmViewPane.ts` `onDidActiveEditorChange`): select and reveal the active editor's
    /// file, searching the groups from the last one (Changes, then Staged, then Merge); keep the selection when the
    /// file is already selected or not in the list.
    fn reveal_active_item(&mut self, cx: &mut Context<Self>) {
        let (Some(workspace), Some(repository)) = (self.workspace.upgrade(), &self.repository)
        else {
            return;
        };
        let Some(project_path) = workspace
            .read(cx)
            .active_item(cx)
            .and_then(|item| item.project_path(cx))
        else {
            return;
        };
        let Some(repo_path) = repository
            .read(cx)
            .project_path_to_repo_path(&project_path, cx)
        else {
            return;
        };
        if self
            .selected_entry()
            .is_some_and(|(_, entry)| entry.repo_path == repo_path)
        {
            return;
        }
        let Some((section_ix, entry_ix, kind)) =
            self.sections
                .iter()
                .enumerate()
                .rev()
                .find_map(|(section_ix, section)| {
                    let entry_ix = section
                        .entries
                        .iter()
                        .position(|entry| entry.repo_path == repo_path)?;
                    Some((section_ix, entry_ix, section.kind))
                })
        else {
            return;
        };
        self.collapsed.retain(|collapsed| *collapsed != kind);
        let row = Row::Entry(section_ix, entry_ix);
        self.selected = Some(row);
        if let Some(row_ix) = self.rows().iter().position(|candidate| *candidate == row) {
            self.scroll_handle
                .scroll_to_item(row_ix, gpui::ScrollStrategy::Center);
        }
        cx.notify();
    }

    fn update_sections(&mut self, cx: &mut Context<Self>) {
        let previous_selection = self
            .selected_entry()
            .map(|(kind, entry)| (kind, entry.repo_path.clone()));
        let mut merge = Vec::new();
        let mut staged = Vec::new();
        let mut changes = Vec::new();
        if let Some(repository) = &self.repository {
            let repository = repository.read(cx);
            for status_entry in repository.cached_status() {
                let entry = Entry {
                    repo_path: status_entry.repo_path.clone(),
                    status: status_entry.status,
                };
                // Unlike Zed's panel, resolved conflicts leave the section once staged (VS Code behavior).
                if status_entry.status.is_conflicted() {
                    merge.push(entry);
                    continue;
                }
                let staging = status_entry.status.staging();
                let (mut in_staged, mut in_changes) =
                    (staging.has_staged(), staging.has_unstaged());
                // VS Code's `git.optimisticUpdate`: a finished stage / unstage / revert moves the row right away,
                // before the status scan confirms it.
                if let Some(op) = repository
                    .pending_ops_for_path(&status_entry.repo_path)
                    .and_then(|ops| ops.ops.last().copied())
                    .filter(|op| op.job_status == JobStatus::Finished)
                {
                    match op.git_status {
                        GitStatus::Staged => (in_staged, in_changes) = (true, false),
                        GitStatus::Unstaged => (in_staged, in_changes) = (false, true),
                        GitStatus::Reverted => (in_staged, in_changes) = (false, false),
                        GitStatus::Unchanged => {}
                    }
                }
                if in_staged {
                    staged.push(entry.clone());
                }
                if in_changes {
                    changes.push(entry);
                }
            }
        }
        self.sections = [
            (SectionKind::Merge, merge),
            (SectionKind::Staged, staged),
            (SectionKind::Changes, changes),
        ]
        .into_iter()
        .filter(|(kind, entries)| !entries.is_empty() || *kind == SectionKind::Changes)
        .map(|(kind, mut entries)| {
            entries.sort_by(|a, b| a.repo_path.cmp(&b.repo_path));
            Section { kind, entries }
        })
        .collect();
        // Keep the selected file selected across refreshes, like VS Code's tree.
        self.selected = previous_selection.and_then(|(kind, repo_path)| {
            self.sections
                .iter()
                .enumerate()
                .filter(|(_, section)| section.kind == kind)
                .find_map(|(section_ix, section)| {
                    let entry_ix = section
                        .entries
                        .iter()
                        .position(|entry| entry.repo_path == repo_path)?;
                    Some(Row::Entry(section_ix, entry_ix))
                })
        });
        cx.notify();
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (section_ix, section) in self.sections.iter().enumerate() {
            rows.push(Row::Header(section_ix));
            if !self.collapsed.contains(&section.kind) {
                rows.extend((0..section.entries.len()).map(|ix| Row::Entry(section_ix, ix)));
            }
        }
        rows
    }

    fn change_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.entries.len())
            .sum()
    }

    fn has_staged(&self) -> bool {
        self.sections
            .iter()
            .any(|section| section.kind == SectionKind::Staged && !section.entries.is_empty())
    }

    fn stage(&mut self, paths: Vec<RepoPath>, stage: bool, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        if stage {
            // User request: staged files are always visible, even if the section was collapsed earlier.
            self.collapsed.retain(|kind| *kind != SectionKind::Staged);
        }
        let task = repository.update(cx, |repository, cx| {
            if stage {
                repository.stage_entries(paths, cx)
            } else {
                repository.unstage_entries(paths, cx)
            }
        });
        let operation = if stage { "stage" } else { "unstage" };
        let workspace = self.workspace.clone();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.refresh_after_operation(&repository, cx))
                .log_err();
            if let Err(error) = result
                && let Some(workspace) = workspace.upgrade()
            {
                cx.update(|cx| show_error_toast(workspace, operation, error, cx));
            }
        })
        .detach();
    }

    /// Like VS Code's `Repository.run`: right after its own operation, show the optimistic state and re-read the
    /// status, instead of waiting for the file watcher to notice the index change.
    fn refresh_after_operation(&mut self, repository: &Entity<Repository>, cx: &mut Context<Self>) {
        self.update_sections(cx);
        repository.update(cx, |repository, cx| repository.bolt_rescan(cx));
    }

    /// VS Code's `ProgressManager`: the bar appears as soon as an operation with `showProgress` runs and hides
    /// once none has been running for 300 ms, so back-to-back operations don't make it flicker.
    fn update_progress(&mut self, cx: &mut Context<Self>) {
        let busy = self.repository.as_ref().is_some_and(|repository| {
            repository
                .read(cx)
                .job_debug_queue()
                .running()
                .any(|job| PROGRESS_JOBS.contains(&job.description.as_ref()))
        });
        if busy {
            self.hide_progress = None;
            if !self.progress_visible {
                self.progress_visible = true;
                cx.notify();
            }
        } else if self.progress_visible && self.hide_progress.is_none() {
            self.hide_progress = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                this.update(cx, |this, cx| {
                    this.progress_visible = false;
                    this.hide_progress = None;
                    cx.notify();
                })
                .ok();
            }));
        }
    }

    fn section_paths(&self, kind: SectionKind) -> Vec<RepoPath> {
        self.sections
            .iter()
            .filter(|section| section.kind == kind)
            .flat_map(|section| section.entries.iter().map(|entry| entry.repo_path.clone()))
            .collect()
    }

    /// Reverts files to HEAD (tracked) or moves them to the trash (untracked), after confirmation.
    fn discard(&mut self, entries: Vec<Entry>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let message = if let [entry] = entries.as_slice() {
            format!(
                "Are you sure you want to discard changes in \"{}\"?",
                entry.repo_path.display(util::paths::PathStyle::local())
            )
        } else {
            format!(
                "Are you sure you want to discard ALL changes in {} files?",
                entries.len()
            )
        };
        let prompt = window.prompt(
            PromptLevel::Warning,
            &message,
            Some("This is IRREVERSIBLE! Untracked files are moved to the trash."),
            &["Discard", "Cancel"],
            cx,
        );
        let project = self.project.clone();
        let workspace = self.workspace.clone();
        cx.spawn(async move |this, cx| {
            if prompt.await != Ok(0) {
                return;
            }
            let (untracked, tracked): (Vec<Entry>, Vec<Entry>) = entries
                .into_iter()
                .partition(|entry| entry.status.is_created());
            let result: Result<()> = async {
                if !tracked.is_empty() {
                    let paths: Vec<RepoPath> =
                        tracked.into_iter().map(|entry| entry.repo_path).collect();
                    repository
                        .update(cx, |repository, cx| {
                            repository.checkout_files("HEAD", paths, cx)
                        })
                        .await?;
                }
                for entry in untracked {
                    let trash = cx.update(|cx| {
                        let project_path = repository
                            .read(cx)
                            .repo_path_to_project_path(&entry.repo_path, cx)?;
                        project.update(cx, |project, cx| project.trash_file(project_path, cx))
                    });
                    if let Some(trash) = trash {
                        trash.await?;
                    }
                }
                Ok(())
            }
            .await;
            this.update(cx, |this, cx| this.refresh_after_operation(&repository, cx))
                .log_err();
            if let Err(error) = result
                && let Some(workspace) = workspace.upgrade()
            {
                cx.update(|cx| show_error_toast(workspace, "discard", error, cx));
            }
        })
        .detach();
    }

    fn open_diff(
        &mut self,
        section_ix: usize,
        entry_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let Some(entry) = self
            .sections
            .get(section_ix)
            .and_then(|section| section.entries.get(entry_ix))
            .cloned()
        else {
            return;
        };
        self.selected = Some(Row::Entry(section_ix, entry_ix));
        let in_merge = self
            .sections
            .get(section_ix)
            .is_some_and(|section| section.kind == SectionKind::Merge);
        if in_merge {
            // Zed's conflict actions (Accept Current / Incoming / Both) only exist in the plain file editor.
            self.open_file(&entry, window, cx);
            cx.notify();
            return;
        }
        if entry.status.is_deleted() || entry.status.is_untracked() {
            // Nothing to diff against on one side: open the file itself when it exists.
            if !entry.status.is_deleted() {
                self.open_file(&entry, window, cx);
            }
            cx.notify();
            return;
        }
        SoloDiffView::open_or_focus(
            GitStatusEntry::bolt_new(entry.repo_path, entry.status),
            repository,
            self.workspace.clone(),
            window,
            cx,
        )
        .detach_and_log_err(cx);
        cx.notify();
    }

    fn open_file(&self, entry: &Entry, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repository) = &self.repository else {
            return;
        };
        let Some(project_path) = repository
            .read(cx)
            .repo_path_to_project_path(&entry.repo_path, cx)
        else {
            return;
        };
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_path_preview(project_path, None, true, false, true, window, cx)
                    .detach_and_log_err(cx);
            });
        }
    }

    fn askpass(
        &self,
        operation: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AskPassDelegate {
        let workspace = self.workspace.clone();
        let window = window.window_handle();
        AskPassDelegate::new_with_cancellation(
            &mut cx.to_async(),
            move |prompt, tx, cancellation, cx| {
                window
                    .update(cx, |_, window, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.toggle_modal(window, cx, |window, cx| {
                                AskPassModal::new(
                                    operation.into(),
                                    prompt.into(),
                                    tx,
                                    cancellation,
                                    window,
                                    cx,
                                )
                            });
                        })
                    })
                    .ok();
            },
        )
    }

    fn commit_with(
        &mut self,
        options: CommitOptions,
        after: AfterCommit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(repository), Some(editor)) =
            (self.repository.clone(), self.commit_editor.clone())
        else {
            return;
        };
        if self
            .sections
            .iter()
            .any(|section| section.kind == SectionKind::Merge)
        {
            let prompt = window.prompt(
                PromptLevel::Warning,
                "There are still merge conflicts. Resolve and stage them before committing.",
                None,
                &["OK"],
                cx,
            );
            cx.spawn(async move |_, _| prompt.await.ok()).detach();
            return;
        }
        let mut message = editor.read(cx).text(cx).trim().to_string();
        if message.is_empty() && options.amend {
            message = repository
                .read(cx)
                .head_commit
                .as_ref()
                .map(|commit| commit.message.to_string())
                .unwrap_or_default();
        }
        if message.is_empty() {
            window.focus(&editor.focus_handle(cx), cx);
            return;
        }
        let commit_tracked = !self.has_staged();
        let has_tracked_changes = self
            .sections
            .iter()
            .filter(|section| section.kind == SectionKind::Changes)
            .flat_map(|section| section.entries.iter())
            .any(|entry| !entry.status.is_created());
        if commit_tracked && !has_tracked_changes && !options.amend {
            let prompt = window.prompt(
                PromptLevel::Info,
                "There are no changes to commit.",
                Some("Stage untracked files with + to include them."),
                &["OK"],
                cx,
            );
            cx.spawn(async move |_, _| prompt.await.ok()).detach();
            return;
        }
        let askpass = self.askpass("git commit", window, cx);
        let workspace = self.workspace.clone();
        self.pending_commit = Some(cx.spawn_in(window, async move |this, cx| {
            let result: Result<()> = async {
                // VS Code commits with `--all`: the tracked changes are read when committing, not from the list
                // shown, which can predate files saved just before.
                if commit_tracked {
                    repository
                        .update(cx, |repository, cx| repository.bolt_stage_tracked(cx))
                        .await?;
                }
                repository
                    .update(cx, |repository, cx| {
                        repository.commit(message.into(), None, options, askpass, cx)
                    })
                    .await??;
                Ok(())
            }
            .await;
            this.update_in(cx, |this, window, cx| {
                this.pending_commit = None;
                match result {
                    Ok(()) => {
                        editor.update(cx, |editor, cx| editor.clear(window, cx));
                        // VS Code refreshes the state right after committing: until then the merge (and so the main
                        // button showing Commit) would look still in progress.
                        repository.update(cx, |repository, cx| repository.bolt_rescan(cx));
                        if let AfterCommit::Remote(operation) = after {
                            this.run_remote(operation, window, cx);
                        }
                    }
                    Err(error) => {
                        log::error!("bolt_scm: git commit failed: {error:?}");
                        if let Some(workspace) = workspace.upgrade() {
                            show_error_toast(workspace, "commit", error, cx);
                        }
                    }
                }
                cx.notify();
            })
            .log_err();
        }));
        cx.notify();
    }

    fn commit(&mut self, _: &Commit, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_with(CommitOptions::default(), AfterCommit::Nothing, window, cx);
    }

    fn commit_message_empty(&self, cx: &App) -> bool {
        self.commit_editor
            .as_ref()
            .is_none_or(|editor| editor.read(cx).buffer().read(cx).len(cx).0 == 0)
    }

    fn merge_in_progress(&self, cx: &App) -> bool {
        self.repository
            .as_ref()
            .is_some_and(|repository| repository.read(cx).merge.message.is_some())
    }

    /// VS Code's action button (`extensions/git/src/actionButton.ts`): Commit when there are changes to commit (or a
    /// merge to conclude), else Publish Branch when the branch has no upstream, else Sync Changes when it is ahead /
    /// behind, else a disabled Commit.
    fn primary_action(&self, cx: &App) -> PrimaryAction {
        if self.change_count() > 0 || self.merge_in_progress(cx) {
            return PrimaryAction::Commit;
        }
        let Some(branch) = self
            .repository
            .as_ref()
            .and_then(|repository| repository.read(cx).branch.as_ref())
        else {
            return PrimaryAction::Commit;
        };
        let Some(upstream) = &branch.upstream else {
            return PrimaryAction::Publish;
        };
        match upstream.tracking.status() {
            Some(status) if status.ahead > 0 || status.behind > 0 => PrimaryAction::Sync {
                ahead: status.ahead,
                behind: status.behind,
            },
            _ => PrimaryAction::Commit,
        }
    }

    fn primary_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.primary_action(cx) {
            PrimaryAction::Commit => self.commit(&Commit, window, cx),
            PrimaryAction::Publish => self.run_remote(RemoteOperation::Push, window, cx),
            PrimaryAction::Sync { .. } => self.run_remote(RemoteOperation::Sync, window, cx),
        }
    }

    /// Runs fetch / pull / push on the repository directly instead of dispatching `git::Fetch` / `git::Pull` /
    /// `git::Push`, which are routed through Zed's hidden git panel.
    fn run_remote(
        &mut self,
        operation: RemoteOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if operation == RemoteOperation::Fetch {
            self.fetch(window, cx);
            return;
        }
        if self.pending_remote.is_some() {
            return;
        }
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let Some(branch) = repository.read(cx).branch.clone() else {
            if let Some(workspace) = self.workspace.upgrade() {
                show_error_toast(
                    workspace,
                    operation.name(),
                    anyhow::anyhow!("HEAD is detached: check out a branch first."),
                    cx,
                );
            }
            return;
        };
        let tracked_upstream = branch
            .upstream
            .clone()
            .filter(|upstream| matches!(upstream.tracking, UpstreamTracking::Tracked(_)));
        let upstream_remote: Option<SharedString> = tracked_upstream
            .as_ref()
            .and_then(|upstream| upstream.remote_name())
            .map(|name| name.to_string().into());
        let branch_name: SharedString = branch.name().to_string().into();
        // Like VS Code's sync: `<branch>:<upstream branch>`.
        let remote_branch: SharedString = tracked_upstream
            .as_ref()
            .and_then(|upstream| upstream.branch_name())
            .map(|name| name.to_string().into())
            .unwrap_or_else(|| branch_name.clone());
        let pull = match operation {
            RemoteOperation::Pull | RemoteOperation::PullRebase => true,
            RemoteOperation::Fetch | RemoteOperation::Push | RemoteOperation::ForcePush => false,
            RemoteOperation::Sync => tracked_upstream.is_some(),
        };
        let rebase = operation == RemoteOperation::PullRebase;
        let push = matches!(
            operation,
            RemoteOperation::Push | RemoteOperation::ForcePush | RemoteOperation::Sync
        );
        let push_options = if tracked_upstream.is_none() {
            Some(PushOptions::SetUpstream)
        } else if operation == RemoteOperation::ForcePush {
            Some(PushOptions::Force)
        } else {
            None
        };
        // `git pull <remote>` needs the branch name when there is no upstream to pull from.
        let pull_branch = branch.upstream.is_none().then(|| branch_name.clone());
        let pull_askpass = pull.then(|| self.askpass("git pull", window, cx));
        let push_askpass = push.then(|| self.askpass("git push", window, cx));
        // VS Code's `_sync` pushes only when HEAD is ahead after the pull (`shouldPush`), counted after the pull:
        // the snapshot may predate a commit just made (Commit & Sync) or the pull itself.
        let check_ahead = operation == RemoteOperation::Sync && tracked_upstream.is_some();
        let work_directory = repository.read(cx).work_directory_abs_path.clone();

        self.pending_remote = Some(operation);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<()> = async {
                let remote_name = match upstream_remote {
                    Some(name) => name,
                    None => {
                        let remotes = repository
                            .update(cx, |repository, _| {
                                repository.get_remotes(Some(branch_name.to_string()), push)
                            })
                            .await??;
                        remotes
                            .iter()
                            .find(|remote| remote.name.as_ref() == "origin")
                            .or_else(|| remotes.first())
                            .map(|remote| remote.name.clone())
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "No remote available. Add a remote to be able to publish changes."
                                )
                            })?
                    }
                };
                let remote = Remote {
                    name: remote_name.clone(),
                };
                let mut pull_output = None;
                if let Some(askpass) = pull_askpass {
                    log::info!("bolt_scm: git pull {remote_name}");
                    let output = repository
                        .update(cx, |repository, cx| {
                            repository.pull(pull_branch, remote_name.clone(), rebase, askpass, cx)
                        })
                        .await??;
                    pull_output = Some(output);
                }
                let push_askpass = match push_askpass {
                    Some(askpass) if check_ahead => {
                        let ahead = commits_ahead_of_upstream(work_directory, cx).await;
                        log::info!("bolt_scm: sync, ahead of upstream after pull: {ahead:?}");
                        ahead.is_none_or(|ahead| ahead > 0).then_some(askpass)
                    }
                    askpass => askpass,
                };
                if push_askpass.is_none()
                    && let Some(output) = pull_output
                {
                    this.update(cx, |this, cx| {
                        this.show_remote_output(RemoteAction::Pull(remote.clone()), output, cx)
                    })?;
                }
                if let Some(askpass) = push_askpass {
                    log::info!("bolt_scm: git push {remote_name} {branch_name}:{remote_branch}");
                    let output = repository
                        .update(cx, |repository, cx| {
                            repository.push(
                                branch_name.clone(),
                                remote_branch,
                                remote_name,
                                push_options,
                                askpass,
                                cx,
                            )
                        })
                        .await??;
                    this.update(cx, |this, cx| {
                        this.show_remote_output(RemoteAction::Push(branch_name, remote), output, cx)
                    })?;
                }
                Ok(())
            }
            .await;
            this.update(cx, |this, cx| this.finish_remote(operation, result, cx))
                .log_err();
        })
        .detach();
    }

    /// VS Code's `git.deleteRemoteBranch` (`repository.deleteRemoteRef`): no confirmation, the remote-tracking ref is
    /// removed by the push itself.
    pub(crate) fn delete_remote_branch(
        &mut self,
        remote: String,
        branch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let askpass = self.askpass("git push", window, cx);
        let workspace = self.workspace.clone();
        cx.spawn(async move |_, cx| {
            log::info!("bolt_scm: git push {remote} :{branch}");
            let result: Result<()> = async {
                repository
                    .update(cx, |repository, cx| {
                        repository.push(
                            SharedString::default(),
                            branch.clone().into(),
                            remote.clone().into(),
                            None,
                            askpass,
                            cx,
                        )
                    })
                    .await??;
                Ok(())
            }
            .await;
            cx.update(|cx| {
                repository.update(cx, |repository, cx| repository.bolt_rescan(cx));
                if let Err(error) = result {
                    log::error!("bolt_scm: deleting {remote}/{branch} failed: {error:?}");
                    if let Some(workspace) = workspace.upgrade() {
                        show_error_toast(workspace, "delete remote branch", error, cx);
                    }
                }
            });
        })
        .detach();
    }

    /// VS Code's `git.confirmForcePush` prompt (`commands.ts` `_push`).
    fn force_push(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = window.prompt(
            PromptLevel::Warning,
            "You are about to force push your changes, this can be destructive and could inadvertently overwrite \
             changes made by others.\n\nAre you sure to continue?",
            None,
            &["OK", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if prompt.await != Ok(0) {
                return;
            }
            this.update_in(cx, |this, window, cx| {
                this.run_remote(RemoteOperation::ForcePush, window, cx)
            })
            .log_err();
        })
        .detach();
    }

    fn fetch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_remote.is_some() {
            return;
        }
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let askpass = self.askpass("git fetch", window, cx);
        self.pending_remote = Some(RemoteOperation::Fetch);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            log::info!("bolt_scm: git fetch --all");
            let result: Result<()> = async {
                let output = repository
                    .update(cx, |repository, cx| {
                        repository.fetch(FetchOptions::All, askpass, cx)
                    })
                    .await??;
                this.update(cx, |this, cx| {
                    this.show_remote_output(RemoteAction::Fetch(None), output, cx)
                })?;
                Ok(())
            }
            .await;
            this.update(cx, |this, cx| {
                this.finish_remote(RemoteOperation::Fetch, result, cx)
            })
            .log_err();
        })
        .detach();
    }

    fn finish_remote(
        &mut self,
        operation: RemoteOperation,
        result: Result<()>,
        cx: &mut Context<Self>,
    ) {
        self.pending_remote = None;
        // Zed doesn't rescan when only remote refs change, so the ahead / behind counts would stay stale.
        if let Some(repository) = &self.repository {
            repository.update(cx, |repository, cx| repository.bolt_rescan(cx));
        }
        if let Err(error) = result {
            log::error!("bolt_scm: git {} failed: {error:?}", operation.name());
            if let Some(workspace) = self.workspace.upgrade() {
                show_error_toast(workspace, operation.name(), error, cx);
            }
        }
        cx.notify();
    }

    fn show_remote_output(
        &self,
        action: RemoteAction,
        output: RemoteCommandOutput,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let SuccessMessage { message, style } = remote_output::format_output(&action, output);
        workspace.update(cx, |workspace, cx| {
            let toast = StatusToast::new(message, cx, move |this, _| {
                let this = this.icon(
                    Icon::new(IconName::GitBranch)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                );
                match style {
                    SuccessStyle::PushPrLink { label, url } => {
                        this.action(label, move |_, cx| cx.open_url(&url))
                    }
                    SuccessStyle::Toast | SuccessStyle::ToastWithLog { .. } => this,
                }
                .dismiss_button(true)
            });
            workspace.toggle_status_toast(toast, cx)
        });
    }

    fn drop_all_stashes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let count = repository.read(cx).cached_stash().entries.len();
        if count == 0 {
            return;
        }
        let detail = if count == 1 {
            "There is 1 stash that will be subject to pruning, and MAY BE IMPOSSIBLE TO RECOVER."
                .to_string()
        } else {
            format!(
                "There are {count} stashes that will be subject to pruning, and MAY BE IMPOSSIBLE TO RECOVER."
            )
        };
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Are you sure you want to drop ALL stashes?",
            Some(&detail),
            &["Yes", "Cancel"],
            cx,
        );
        let workspace = self.workspace.clone();
        cx.spawn(async move |_, cx| {
            if prompt.await != Ok(0) {
                return;
            }
            for _ in 0..count {
                let drop =
                    repository.update(cx, |repository, cx| repository.stash_drop(Some(0), cx));
                let result: Result<()> = async { drop.await? }.await;
                if let Err(error) = result {
                    if let Some(workspace) = workspace.upgrade() {
                        cx.update(|cx| show_error_toast(workspace, "stash drop", error, cx));
                    }
                    break;
                }
            }
        })
        .detach();
    }

    fn render_more_menu(&self, cx: &Context<Self>) -> impl IntoElement {
        let panel = cx.entity().downgrade();
        PopoverMenu::new("bolt-scm-more")
            .trigger_with_tooltip(
                IconButton::new("bolt-scm-more-button", IconName::Ellipsis)
                    .icon_size(IconSize::Small),
                Tooltip::text("More Actions…"),
            )
            .anchor(gpui::Anchor::TopRight)
            .menu(move |window, cx| {
                let panel = panel.clone();
                Some(ContextMenu::build(window, cx, move |menu, _, _| {
                    let drop_all = panel.clone();
                    let remote_entry = |label: &'static str, operation: RemoteOperation| {
                        let panel = panel.clone();
                        move |menu: ContextMenu| {
                            menu.entry(label, None, move |window, cx| {
                                panel
                                    .update(cx, |panel, cx| panel.run_remote(operation, window, cx))
                                    .ok();
                            })
                        }
                    };
                    let pull = remote_entry("Pull", RemoteOperation::Pull);
                    let push = remote_entry("Push", RemoteOperation::Push);
                    let submenu_pull = remote_entry("Pull", RemoteOperation::Pull);
                    let submenu_push = remote_entry("Push", RemoteOperation::Push);
                    let sync = remote_entry("Sync", RemoteOperation::Sync);
                    let fetch = remote_entry("Fetch", RemoteOperation::Fetch);
                    let submenu_fetch = remote_entry("Fetch", RemoteOperation::Fetch);
                    menu.map(pull)
                        .map(push)
                        .action("Clone", git::Clone.boxed_clone())
                        .action("Checkout to…", zed_actions::git::Branch.boxed_clone())
                        .map(fetch)
                        .separator()
                        .submenu("Commit", |menu, _, _| {
                            menu.action("Commit", Commit.boxed_clone())
                                .action("Undo Last Commit", git::Uncommit.boxed_clone())
                        })
                        .submenu("Changes", |menu, _, _| {
                            menu.action("Stage All Changes", git::StageAll.boxed_clone())
                                .action("Unstage All Changes", git::UnstageAll.boxed_clone())
                                .action("Discard All Changes", DiscardAllChanges.boxed_clone())
                        })
                        .submenu("Pull, Push", move |menu, _, _| {
                            menu.map(sync.clone())
                                .separator()
                                .map(submenu_pull.clone())
                                .action("Pull (Rebase)", PullRebase.boxed_clone())
                                .separator()
                                .map(submenu_push.clone())
                                .action("Push (Force)", ForcePush.boxed_clone())
                                .separator()
                                .map(submenu_fetch.clone())
                        })
                        .submenu("Branch", |menu, _, _| {
                            menu.action("Merge…", MergeBranch.boxed_clone())
                                .separator()
                                .action("Create Branch…", CreateBranch.boxed_clone())
                                .action("Create Branch From…", CreateBranchFrom.boxed_clone())
                                .separator()
                                .action(
                                    "Rename Branch…",
                                    git::RenameBranch { branch: None }.boxed_clone(),
                                )
                                .action("Delete Branch…", DeleteBranch.boxed_clone())
                                .action("Delete Remote Branch…", DeleteRemoteBranch.boxed_clone())
                        })
                        .submenu("Stash", move |menu, _, _| {
                            let drop_all = drop_all.clone();
                            menu.action("Stash", git::StashTracked.boxed_clone())
                                .action("Stash (Include Untracked)", git::StashAll.boxed_clone())
                                .action("Stash Staged", git::StashStaged.boxed_clone())
                                .separator()
                                .action("Apply Latest Stash", git::StashApply.boxed_clone())
                                .action("Apply Stash…", ApplyStash.boxed_clone())
                                .separator()
                                .action("Pop Latest Stash", git::StashPop.boxed_clone())
                                .action("Pop Stash…", PopStash.boxed_clone())
                                .separator()
                                .action("Drop Stash…", DropStash.boxed_clone())
                                .entry("Drop All Stashes…", None, move |window, cx| {
                                    drop_all
                                        .update(cx, |panel, cx| panel.drop_all_stashes(window, cx))
                                        .ok();
                                })
                                .separator()
                                .action("View Stash…", ViewStash.boxed_clone())
                        })
                        .submenu("Worktrees", |menu, _, _| {
                            menu.action("Worktrees…", zed_actions::git::Worktree.boxed_clone())
                        })
                }))
            })
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .flex_none()
            .h(Tab::container_height(cx))
            .pl_2p5()
            .pr_2()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("Changes")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_0p5()
                    .child(
                        IconButton::new("bolt-scm-refresh", IconName::RotateCw)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Refresh"))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                if let Some(repository) = &this.repository {
                                    repository
                                        .update(cx, |repository, cx| repository.bolt_rescan(cx));
                                }
                            })),
                    )
                    .child(
                        IconButton::new("bolt-scm-uncommit", IconName::Undo)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Undo Last Commit"))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(git::Uncommit.boxed_clone(), cx)
                            }),
                    )
                    .child(
                        ButtonLike::new("bolt-scm-merge")
                            .child(
                                Icon::from_path(MERGE_ICON)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .tooltip(Tooltip::text("Merge Branch… (no commit, no fast-forward)"))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(MergeBranch.boxed_clone(), cx)
                            }),
                    )
                    .child(self.render_more_menu(cx)),
            )
    }

    fn render_commit_area(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let focused = self
            .commit_editor
            .as_ref()
            .is_some_and(|editor| editor.focus_handle(cx).contains_focused(window, cx));
        let message_empty = self.commit_message_empty(cx);
        let committing = self.pending_commit.is_some();
        let busy = committing || self.pending_remote.is_some();
        let primary_action = self.primary_action(cx);
        let primary_disabled = busy
            || (matches!(primary_action, PrimaryAction::Commit)
                && self.change_count() == 0
                && !self.merge_in_progress(cx));
        let label: SharedString = if committing {
            "Committing…".into()
        } else if let Some(operation) = self.pending_remote {
            match operation {
                RemoteOperation::Fetch => "Fetching…".into(),
                RemoteOperation::Pull | RemoteOperation::PullRebase => "Pulling…".into(),
                RemoteOperation::Push | RemoteOperation::ForcePush => "Pushing…".into(),
                RemoteOperation::Sync => "Syncing…".into(),
            }
        } else {
            match primary_action {
                PrimaryAction::Commit if self.has_staged() => "Commit".into(),
                PrimaryAction::Commit => "Commit Tracked".into(),
                PrimaryAction::Publish => "Publish Branch".into(),
                PrimaryAction::Sync { ahead, behind } => {
                    let mut label = "Sync Changes".to_string();
                    if behind > 0 {
                        label.push_str(&format!(" {behind}↓"));
                    }
                    if ahead > 0 {
                        label.push_str(&format!(" {ahead}↑"));
                    }
                    label.into()
                }
            }
        };
        let panel = cx.entity().downgrade();
        let commit_menu_open = self.commit_menu_handle.is_deployed();
        v_flex()
            .w_full()
            .flex_none()
            .p_2()
            .gap_1p5()
            .child(
                div()
                    .key_context("BoltScmCommit")
                    .w_full()
                    .min_h(px(26.))
                    .px_1p5()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(if focused {
                        colors.border_focused
                    } else {
                        colors.border
                    })
                    .relative()
                    .bg(colors.editor_background)
                    .overflow_hidden()
                    // An AutoHeight editor measures itself from the available width: as a flex item it got the
                    // min-content width, wrapped every character and grew to max_lines with nothing visible.
                    .children(self.commit_editor.as_ref().map(|editor| {
                        div()
                            .w_full()
                            .child(EditorElement::new(editor, commit_input_style(cx)))
                    }))
                    // Drawn here rather than with the editor's placeholder, which is clipped instead of truncated.
                    .when(message_empty, |this| {
                        this.child(
                            div()
                                .absolute()
                                .inset_0()
                                .px_1p5()
                                .flex()
                                .items_center()
                                .child(
                                    Label::new(self.placeholder(cx))
                                        .buffer_font(cx)
                                        .size(LabelSize::Custom(rems_from_px(
                                            ThemeSettings::get_global(cx)
                                                .git_commit_buffer_font_size(cx)
                                                .as_f32(),
                                        )))
                                        .color(Color::Placeholder)
                                        .truncate(),
                                ),
                        )
                    }),
            )
            .child(
                SplitButton::new(
                    ButtonLike::new_rounded_left("bolt-scm-commit-button")
                        .layer(ElevationIndex::ModalSurface)
                        .full_width()
                        .disabled(primary_disabled)
                        .child(
                            h_flex()
                                .w_full()
                                .gap_1()
                                .justify_center()
                                .when(self.pending_remote.is_some(), |this| {
                                    this.child(
                                        Icon::new(IconName::ArrowCircle)
                                            .size(IconSize::Small)
                                            .color(Color::Muted)
                                            .with_rotate_animation(2),
                                    )
                                })
                                .child(Label::new(label).size(LabelSize::Small).color(
                                    if primary_disabled {
                                        Color::Disabled
                                    } else {
                                        Color::Default
                                    },
                                )),
                        )
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.primary_click(window, cx)
                        })),
                    PopoverMenu::new("bolt-scm-commit-options")
                        .trigger(
                            ButtonLike::new_rounded_right("bolt-scm-commit-options-button")
                                .layer(ElevationIndex::ModalSurface)
                                .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                                .width(rems_from_px(20_f32))
                                .child(
                                    Icon::new(if commit_menu_open {
                                        IconName::ChevronUp
                                    } else {
                                        IconName::ChevronDown
                                    })
                                    .size(IconSize::XSmall),
                                ),
                        )
                        .with_handle(self.commit_menu_handle.clone())
                        .anchor(gpui::Anchor::TopRight)
                        .offset(gpui::Point {
                            x: px(0.),
                            y: px(2.),
                        })
                        .menu(move |window, cx| {
                            let panel = panel.clone();
                            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                                let entry =
                                    |label: &'static str,
                                     options: CommitOptions,
                                     after: AfterCommit| {
                                        let panel = panel.clone();
                                        move |menu: ContextMenu| {
                                            menu.entry(label, None, move |window, cx| {
                                                panel
                                                    .update(cx, |panel, cx| {
                                                        panel
                                                            .commit_with(options, after, window, cx)
                                                    })
                                                    .ok();
                                            })
                                        }
                                    };
                                let amend = CommitOptions {
                                    amend: true,
                                    ..CommitOptions::default()
                                };
                                let menu = entry(
                                    "Commit",
                                    CommitOptions::default(),
                                    AfterCommit::Nothing,
                                )(menu);
                                let menu =
                                    entry("Commit (Amend)", amend, AfterCommit::Nothing)(menu);
                                let menu = entry(
                                    "Commit & Push",
                                    CommitOptions::default(),
                                    AfterCommit::Remote(RemoteOperation::Push),
                                )(menu);
                                entry(
                                    "Commit & Sync",
                                    CommitOptions::default(),
                                    AfterCommit::Remote(RemoteOperation::Sync),
                                )(menu)
                            }))
                        })
                        .into_any_element(),
                )
                .style(SplitButtonStyle::Outlined),
            )
    }

    fn render_row(&self, row: Row, cx: &Context<Self>) -> AnyElement {
        match row {
            Row::Header(section_ix) => self.render_section_header(section_ix, cx),
            Row::Entry(section_ix, entry_ix) => self.render_entry(section_ix, entry_ix, cx),
        }
    }

    fn render_section_header(&self, section_ix: usize, cx: &Context<Self>) -> AnyElement {
        let section = &self.sections[section_ix];
        let kind = section.kind;
        let collapsed = self.collapsed.contains(&kind);
        let entries = section.entries.clone();
        let actions = h_flex()
            .gap_0p5()
            .when(kind == SectionKind::Changes, |this| {
                let entries = entries.clone();
                this.child(
                    IconButton::new("bolt-scm-discard-all", IconName::Undo)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Discard All Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.discard(entries.clone(), window, cx)
                        })),
                )
            })
            .map(|this| match kind {
                SectionKind::Staged => this.child(
                    IconButton::new("bolt-scm-unstage-all", IconName::Dash)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Unstage All Changes"))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            let paths = this.section_paths(SectionKind::Staged);
                            this.stage(paths, false, cx)
                        })),
                ),
                SectionKind::Changes | SectionKind::Merge => this.child(
                    IconButton::new(("bolt-scm-stage-all", section_ix), IconName::Plus)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Stage All Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            let paths = this.section_paths(kind);
                            this.stage(paths, true, cx)
                        })),
                ),
            });
        let group: SharedString = format!("bolt-scm-section-{section_ix}").into();
        h_flex()
            .id(("bolt-scm-section", section_ix))
            .group(group)
            .h(LIST_ITEM_HEIGHT)
            .w_full()
            .pl_2p5()
            .pr_1()
            .gap_2()
            .justify_between()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().colors().ghost_element_hover))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                    )
                    .child(
                        Label::new(kind.title())
                            .color(Color::Muted)
                            .size(LabelSize::Small),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .when(self.hovered_row == Some(Row::Header(section_ix)), |this| {
                        this.child(div().flex_none().child(actions))
                    })
                    .child(
                        Label::new(section.entries.len().to_string())
                            .color(Color::Muted)
                            .size(LabelSize::Small),
                    ),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hovered_row(Row::Header(section_ix), *hovered, cx)
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_section(kind, cx)))
            .into_any_element()
    }

    /// Row actions are only rendered for the hovered row, so they take no space otherwise (VS Code). Toggling their
    /// visibility with `group_hover` styles instead panics in GPUI (layout changes between prepaint and paint).
    fn set_hovered_row(&mut self, row: Row, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered_row != Some(row) {
                self.hovered_row = Some(row);
                cx.notify();
            }
        } else if self.hovered_row == Some(row) {
            self.hovered_row = None;
            cx.notify();
        }
    }

    fn toggle_section(&mut self, kind: SectionKind, cx: &mut Context<Self>) {
        if let Some(position) = self
            .collapsed
            .iter()
            .position(|collapsed| *collapsed == kind)
        {
            self.collapsed.remove(position);
        } else {
            self.collapsed.push(kind);
        }
        cx.notify();
    }

    fn render_entry(&self, section_ix: usize, entry_ix: usize, cx: &Context<Self>) -> AnyElement {
        let section = &self.sections[section_ix];
        let kind = section.kind;
        let entry = section.entries[entry_ix].clone();
        let path_style = util::paths::PathStyle::local();
        let status = entry.status;
        let file_name: SharedString = entry
            .repo_path
            .file_name()
            .unwrap_or_default()
            .to_string()
            .into();
        let directory = entry
            .repo_path
            .parent()
            .map(|parent| parent.display(path_style).to_string())
            .filter(|parent| !parent.is_empty());
        let is_deleted = status.is_deleted();
        let (status_style, file_icons) = git_ui::bolt_panel_entry_style(cx);
        // Same colors as Zed's git panel entries.
        let label_color = if status_style == StatusStyle::LabelColor {
            if status.is_conflicted() || kind == SectionKind::Merge {
                Color::VersionControlConflict
            } else if status.is_created() {
                Color::VersionControlAdded
            } else if status.is_modified() {
                Color::VersionControlModified
            } else if is_deleted {
                Color::Disabled
            } else {
                Color::VersionControlAdded
            }
        } else {
            Color::Default
        };
        let path_color = if is_deleted {
            Color::Disabled
        } else {
            Color::Muted
        };
        let file_icon = file_icons.then(|| {
            FileIcons::get_icon(entry.repo_path.as_std_path(), cx)
                .map(Icon::from_path)
                .unwrap_or_else(|| Icon::new(IconName::File))
                .size(IconSize::Small)
                .color(Color::Muted)
        });
        let id = section_ix * 1_000_000 + entry_ix;
        let group: SharedString = format!("bolt-scm-entry-{id}").into();
        let selected = self.selected == Some(Row::Entry(section_ix, entry_ix));
        let colors = cx.theme().colors();
        let info_color = cx.theme().status().info;

        let open_entry = entry.clone();
        let discard_entry = entry.clone();
        let stage_path = entry.repo_path;
        let actions = h_flex()
            .gap_0p5()
            .when(!is_deleted, |this| {
                this.child(
                    IconButton::new(("bolt-scm-open-file", id), IconName::File)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Open File"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_file(&open_entry, window, cx)
                        })),
                )
            })
            .when(kind == SectionKind::Changes, |this| {
                this.child(
                    IconButton::new(("bolt-scm-discard", id), IconName::Undo)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Discard Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.discard(vec![discard_entry.clone()], window, cx)
                        })),
                )
            })
            .child(match kind {
                SectionKind::Staged => IconButton::new(("bolt-scm-unstage", id), IconName::Dash)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Unstage Changes"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.stage(vec![stage_path.clone()], false, cx)
                    })),
                SectionKind::Changes | SectionKind::Merge => {
                    IconButton::new(("bolt-scm-stage", id), IconName::Plus)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Stage Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.stage(vec![stage_path.clone()], true, cx)
                        }))
                }
            });

        let name_row = h_flex()
            .min_w_0()
            .flex_1()
            .gap_1()
            .children(file_icon)
            .when(status_style != StatusStyle::LabelColor, |this| {
                this.child(git_ui::git_status_icon(status))
            })
            .child(
                h_flex()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div().flex_none().child(
                            Label::new(format!("{file_name} "))
                                .single_line()
                                .color(label_color)
                                .when(is_deleted, Label::strikethrough),
                        ),
                    )
                    .when_some(directory, |this, directory| {
                        this.child(
                            Label::new(directory)
                                .single_line()
                                .color(path_color)
                                .truncate_start()
                                .when(is_deleted, Label::strikethrough),
                        )
                    }),
            );

        h_flex()
            .id(("bolt-scm-entry", id))
            .group(group)
            .h(LIST_ITEM_HEIGHT)
            .w_full()
            .pl_2p5()
            .pr_1()
            .gap_1p5()
            .when(selected, |this| this.bg(info_color.alpha(0.08)))
            .hover(|style| {
                style.bg(if selected {
                    info_color.alpha(0.12)
                } else {
                    colors.ghost_element_hover
                })
            })
            .child(name_row)
            .when(
                self.hovered_row == Some(Row::Entry(section_ix, entry_ix)),
                |this| this.child(div().flex_none().child(actions)),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hovered_row(Row::Entry(section_ix, entry_ix), *hovered, cx)
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_diff(section_ix, entry_ix, window, cx)
            }))
            .into_any_element()
    }
}

/// `git rev-list --count @{upstream}..HEAD`; `None` when git can't tell (VS Code then pushes anyway).
async fn commits_ahead_of_upstream(
    work_directory: Arc<Path>,
    cx: &mut AsyncWindowContext,
) -> Option<u32> {
    let output = cx
        .background_spawn(async move {
            util::command::new_command("git")
                .args(["rev-list", "--count", "@{upstream}..HEAD"])
                .current_dir(work_directory.as_ref())
                .output()
                .await
        })
        .await
        .log_err()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .log_err()
}

/// Zed's git panel commit editor style (buffer font, `git_commit_buffer_font_size`).
fn commit_input_style(cx: &App) -> EditorStyle {
    let font_size = ThemeSettings::get_global(cx).git_commit_buffer_font_size(cx);
    git_ui::git_panel::git_commit_editor_style(font_size, cx)
}

/// VS Code's infinite progress bar (`progressbar.css`): a 2px bit, 2% wide, that crosses the view in 4 s, keyframes
/// `translateX(0) scaleX(1)` → 50% `translateX(2500%) scaleX(3)` → `translateX(4900%) scaleX(1)`, scaled around its
/// center.
fn render_progress_bar(cx: &App) -> impl IntoElement {
    const BIT_WIDTH: f32 = 0.02;
    div()
        .absolute()
        .left_0()
        .bottom_0()
        .w_full()
        .h(px(2.))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .bg(cx.theme().colors().border_focused)
                .with_animation(
                    "bolt-scm-progress",
                    gpui::Animation::new(Duration::from_secs(4)).repeat(),
                    |bit, delta| {
                        let (translate, scale) = if delta <= 0.5 {
                            (25. * delta * 2., 1. + 2. * delta * 2.)
                        } else {
                            let delta = (delta - 0.5) * 2.;
                            (25. + 24. * delta, 3. - 2. * delta)
                        };
                        let width = BIT_WIDTH * scale;
                        let center = BIT_WIDTH * (translate + 0.5);
                        bit.left(relative(center - width / 2.)).w(relative(width))
                    },
                ),
        )
}

impl Render for ScmPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows();
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("BoltScmPanel");
        v_flex()
            .id("bolt-scm-panel")
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::commit))
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                div()
                    .relative()
                    .flex_none()
                    .child(self.render_header(cx))
                    .when(self.progress_visible, |this| {
                        this.child(render_progress_bar(cx))
                    }),
            )
            .map(|this| {
                if self.repository.is_none() {
                    this.child(
                        div().px_3().py_2().child(
                            Label::new("The folder currently open doesn't have a git repository.")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                } else {
                    this.child(self.render_commit_area(window, cx)).child(
                        uniform_list(
                            "bolt-scm-rows",
                            rows.len(),
                            cx.processor(move |this, range: Range<usize>, _window, cx| {
                                let rows = this.rows();
                                range
                                    .filter_map(|ix| rows.get(ix).copied())
                                    .map(|row| this.render_row(row, cx))
                                    .collect()
                            }),
                        )
                        .flex_1()
                        .size_full()
                        .track_scroll(&self.scroll_handle),
                    )
                }
            })
    }
}

impl Focusable for ScmPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for ScmPanel {}

impl Panel for ScmPanel {
    fn persistent_name() -> &'static str {
        "Source Control Panel"
    }

    fn panel_key() -> &'static str {
        SCM_PANEL_KEY
    }

    fn activation_focus_handle(&self, cx: &App) -> FocusHandle {
        self.commit_editor
            .as_ref()
            .map(|editor| editor.focus_handle(cx))
            .unwrap_or_else(|| self.focus_handle.clone())
    }

    fn position(&self, _: &Window, _: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left)
    }

    fn set_position(&mut self, _: DockPosition, _: &mut Window, _: &mut Context<Self>) {}

    fn default_size(&self, _: &Window, _: &App) -> Pixels {
        px(300.)
    }

    fn icon(&self, _: &Window, cx: &App) -> Option<IconName> {
        git_ui::bolt_panel_enabled(cx).then_some(IconName::GitBranch)
    }

    fn icon_tooltip(&self, _: &Window, _: &App) -> Option<&'static str> {
        Some("Source Control")
    }

    fn icon_label(&self, _: &Window, _: &App) -> Option<String> {
        let count = self.change_count();
        (count > 0).then(|| count.to_string())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        // After Explorer (1), Search (2) and Zed's hidden git panel (3).
        4
    }
}
