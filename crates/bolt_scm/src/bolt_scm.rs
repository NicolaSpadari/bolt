//! BOLT: VS Code-style Source Control panel (left dock), replacing Zed's git panel in the UI.
//!
//! Zed's `GitPanel` stays loaded (without a dock button) because Zed's workspace-level git actions (push, pull,
//! fetch, stash, uncommit, …) are routed through it; this panel dispatches those actions and calls the repository
//! directly for staging, discarding and committing. The commit message lives in the repository's shared commit buffer,
//! so it is the same text in both panels. `git_panel.bolt_panel: false` shows Zed's panel instead.
//! Spec: `notes/vscode-scm-parity.md`.

mod merge_picker;

use std::{ops::Range, path::Path, sync::Arc};

use anyhow::Result;
use askpass::AskPassDelegate;
use editor::{Editor, EditorMode};
use file_icons::FileIcons;
use git::{
    repository::{CommitOptions, RepoPath},
    status::{FileStatus, StatusCode},
};
use git_ui::{git_panel::GitStatusEntry, solo_diff_view::SoloDiffView};
use git_ui_core::{askpass_modal::AskPassModal, notifications::show_error_toast};
use gpui::{
    Action, AnyElement, App, AsyncWindowContext, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Focusable, KeyContext, Pixels, PromptLevel, Render, SharedString, Subscription,
    Task, UniformListScrollHandle, WeakEntity, Window, actions, px, uniform_list,
};
use multi_buffer::MultiBuffer;
use project::{
    Project,
    git_store::{GitStoreEvent, Repository, RepositoryEvent},
};
use theme::ActiveTheme as _;
use ui::{
    ButtonLike, ContextMenu, Icon, IconButton, IconName, IconSize, Label, LabelSize, ListItem,
    ListItemSpacing, PopoverMenu, Tooltip, prelude::*,
};
use util::ResultExt as _;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

const SCM_PANEL_KEY: &str = "BoltScmPanel";
const MERGE_ICON: &str = "icons/bolt/git_merge.svg";

actions!(
    bolt_scm,
    [
        /// Toggles focus on the source control panel (Zed's git panel when Bolt's one is disabled).
        ToggleFocus,
        /// Commits the staged changes (all changes when nothing is staged).
        Commit,
        /// Merges a branch into the current one without committing (`git merge --no-commit --no-ff`).
        MergeBranch,
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
            merge_picker::open(workspace, window, cx);
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
    Push,
    Sync,
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
    scroll_handle: UniformListScrollHandle,
    pending_commit: Option<Task<()>>,
    open_commit_buffer: Option<Task<()>>,
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
                        this.update_placeholder(window, cx);
                    }
                    _ => {}
                },
            );
            let settings_subscription =
                cx.observe_global::<settings::SettingsStore>(|_, cx| cx.notify());
            let mut this = Self {
                workspace: weak_workspace,
                project,
                focus_handle: cx.focus_handle(),
                repository: None,
                commit_editor: None,
                sections: Vec::new(),
                collapsed: Vec::new(),
                selected: None,
                scroll_handle: UniformListScrollHandle::new(),
                pending_commit: None,
                open_commit_buffer: None,
                _subscriptions: vec![subscription, settings_subscription],
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
                    let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
                    Editor::new(
                        EditorMode::SingleLine,
                        multi_buffer,
                        Some(project),
                        window,
                        cx,
                    )
                });
                this.commit_editor = Some(editor);
                this.update_placeholder(window, cx);
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

    fn update_placeholder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = match self.branch_name(cx) {
            Some(branch) => format!("Message (⌘⏎ to commit on \"{branch}\")"),
            None => "Message (⌘⏎ to commit)".to_string(),
        };
        if let Some(editor) = &self.commit_editor {
            editor.update(cx, |editor, cx| {
                editor.set_placeholder_text(&placeholder, window, cx)
            });
        }
    }

    fn update_sections(&mut self, cx: &mut Context<Self>) {
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
                if status_entry.status.is_conflicted()
                    || repository.had_conflict_on_last_merge_head_change(&status_entry.repo_path)
                {
                    merge.push(entry);
                    continue;
                }
                let staging = status_entry.status.staging();
                if staging.has_staged() {
                    staged.push(entry.clone());
                }
                if staging.has_unstaged() {
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
        self.selected = None;
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

    fn report<T: 'static>(
        &self,
        operation: &'static str,
        task: Task<Result<T>>,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        cx.spawn(async move |_, cx| {
            if let Err(error) = task.await
                && let Some(workspace) = workspace.upgrade()
            {
                cx.update(|cx| show_error_toast(workspace, operation, error, cx));
            }
        })
        .detach();
    }

    fn stage(&mut self, paths: Vec<RepoPath>, stage: bool, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        let task = repository.update(cx, |repository, cx| {
            if stage {
                repository.stage_entries(paths, cx)
            } else {
                repository.unstage_entries(paths, cx)
            }
        });
        self.report(if stage { "stage" } else { "unstage" }, task, cx);
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
        cx.spawn(async move |_, cx| {
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
        let stage_first: Vec<RepoPath> = if self.has_staged() {
            Vec::new()
        } else {
            self.sections
                .iter()
                .filter(|section| section.kind == SectionKind::Changes)
                .flat_map(|section| section.entries.iter())
                .filter(|entry| !entry.status.is_created())
                .map(|entry| entry.repo_path.clone())
                .collect()
        };
        if !self.has_staged() && stage_first.is_empty() && !options.amend {
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
                if !stage_first.is_empty() {
                    repository
                        .update(cx, |repository, cx| {
                            repository.stage_entries(stage_first, cx)
                        })
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
                        match after {
                            AfterCommit::Nothing => {}
                            AfterCommit::Push => {
                                window.dispatch_action(git::Push.boxed_clone(), cx)
                            }
                            AfterCommit::Sync => {
                                // Repository jobs run in order: pull, then push.
                                window.dispatch_action(git::Pull.boxed_clone(), cx);
                                window.dispatch_action(git::Push.boxed_clone(), cx);
                            }
                        }
                    }
                    Err(error) => {
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

    fn drop_all_stashes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let count = repository.read(cx).cached_stash().entries.len();
        if count == 0 {
            return;
        }
        let prompt = window.prompt(
            PromptLevel::Warning,
            &format!("Are you sure you want to drop ALL {count} stashes?"),
            Some("This is IRREVERSIBLE!"),
            &["Drop All", "Cancel"],
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
                    menu.action("Pull", git::Pull.boxed_clone())
                        .action("Push", git::Push.boxed_clone())
                        .action("Clone", git::Clone.boxed_clone())
                        .action("Checkout to…", zed_actions::git::Branch.boxed_clone())
                        .action("Fetch", git::Fetch.boxed_clone())
                        .separator()
                        .submenu("Commit", |menu, _, _| {
                            menu.action("Commit", Commit.boxed_clone())
                                .action("Undo Last Commit", git::Uncommit.boxed_clone())
                        })
                        .submenu("Changes", |menu, _, _| {
                            menu.action("Stage All Changes", git::StageAll.boxed_clone())
                                .action("Unstage All Changes", git::UnstageAll.boxed_clone())
                        })
                        .submenu("Pull, Push", |menu, _, _| {
                            menu.action("Pull", git::Pull.boxed_clone())
                                .action("Pull (Rebase)", git::PullRebase.boxed_clone())
                                .action("Push", git::Push.boxed_clone())
                                .action("Force Push", git::ForcePush.boxed_clone())
                                .action("Fetch", git::Fetch.boxed_clone())
                        })
                        .submenu("Branch", |menu, _, _| {
                            menu.action("Merge Branch…", MergeBranch.boxed_clone())
                                .action("Create Branch…", zed_actions::git::Branch.boxed_clone())
                                .action(
                                    "Rename Branch…",
                                    git::RenameBranch { branch: None }.boxed_clone(),
                                )
                                .action("Delete Branch…", zed_actions::git::Branch.boxed_clone())
                        })
                        .submenu("Remote", |menu, _, _| {
                            menu.action("Add Remote…", zed_actions::git::CreateRemote.boxed_clone())
                        })
                        .submenu("Stash", move |menu, _, _| {
                            let drop_all = drop_all.clone();
                            menu.action("Stash", git::StashTracked.boxed_clone())
                                .action("Stash (Include Untracked)", git::StashAll.boxed_clone())
                                .action("Stash Staged", git::StashStaged.boxed_clone())
                                .separator()
                                .action("Apply Latest Stash", git::StashApply.boxed_clone())
                                .action("Apply Stash…", zed_actions::git::ViewStash.boxed_clone())
                                .separator()
                                .action("Pop Latest Stash", git::StashPop.boxed_clone())
                                .action("Pop Stash…", zed_actions::git::ViewStash.boxed_clone())
                                .separator()
                                .action("Drop Stash…", zed_actions::git::ViewStash.boxed_clone())
                                .entry("Drop All Stashes…", None, move |window, cx| {
                                    drop_all
                                        .update(cx, |panel, cx| panel.drop_all_stashes(window, cx))
                                        .ok();
                                })
                                .separator()
                                .action("View Stash…", zed_actions::git::ViewStash.boxed_clone())
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
            .h_8()
            .px_2()
            .justify_between()
            .child(
                Label::new("CHANGES")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_0p5()
                    .child(
                        IconButton::new("bolt-scm-commit", IconName::Check)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Commit"))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.commit(&Commit, window, cx)
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
        let committing = self.pending_commit.is_some();
        let panel = cx.entity().downgrade();
        v_flex()
            .w_full()
            .px_2()
            .pb_2()
            .gap_1p5()
            .child(
                div()
                    .key_context("BoltScmCommit")
                    .w_full()
                    .h_7()
                    .px_1p5()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(if focused { colors.border_focused } else { colors.border })
                    .bg(colors.editor_background)
                    .children(self.commit_editor.clone()),
            )
            .child(
                h_flex()
                    .w_full()
                    .rounded_sm()
                    .bg(colors.element_background)
                    .child(
                        ButtonLike::new("bolt-scm-commit-button")
                            .full_width()
                            .disabled(committing)
                            .child(
                                h_flex()
                                    .w_full()
                                    .justify_center()
                                    .gap_1()
                                    .child(Icon::new(IconName::Check).size(IconSize::Small))
                                    .child(Label::new(if committing { "Committing…" } else { "Commit" })),
                            )
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.commit(&Commit, window, cx)
                            })),
                    )
                    .child(div().h_4().w_px().bg(colors.border))
                    .child(
                        PopoverMenu::new("bolt-scm-commit-options")
                            .trigger(
                                IconButton::new("bolt-scm-commit-options-button", IconName::ChevronDown)
                                    .icon_size(IconSize::Small),
                            )
                            .anchor(gpui::Anchor::TopRight)
                            .menu(move |window, cx| {
                                let panel = panel.clone();
                                Some(ContextMenu::build(window, cx, move |menu, _, _| {
                                    let entry = |label: &'static str, options: CommitOptions, after: AfterCommit| {
                                        let panel = panel.clone();
                                        move |menu: ContextMenu| {
                                            menu.entry(label, None, move |window, cx| {
                                                panel
                                                    .update(cx, |panel, cx| {
                                                        panel.commit_with(options, after, window, cx)
                                                    })
                                                    .ok();
                                            })
                                        }
                                    };
                                    let amend = CommitOptions {
                                        amend: true,
                                        ..CommitOptions::default()
                                    };
                                    let menu = entry("Commit", CommitOptions::default(), AfterCommit::Nothing)(menu);
                                    let menu = entry("Commit (Amend)", amend, AfterCommit::Nothing)(menu);
                                    let menu = entry("Commit & Push", CommitOptions::default(), AfterCommit::Push)(menu);
                                    entry("Commit & Sync", CommitOptions::default(), AfterCommit::Sync)(menu)
                                }))
                            }),
                    ),
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
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Discard All Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.discard(entries.clone(), window, cx)
                        })),
                )
            })
            .map(|this| match kind {
                SectionKind::Staged => this.child(
                    IconButton::new("bolt-scm-unstage-all", IconName::Dash)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Unstage All Changes"))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            let paths = this.section_paths(SectionKind::Staged);
                            this.stage(paths, false, cx)
                        })),
                ),
                SectionKind::Changes | SectionKind::Merge => this.child(
                    IconButton::new(("bolt-scm-stage-all", section_ix), IconName::Plus)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Stage All Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let paths = this.section_paths(kind);
                            this.stage(paths, true, cx)
                        })),
                ),
            });
        let group: SharedString = format!("bolt-scm-section-{section_ix}").into();
        ListItem::new(("bolt-scm-section", section_ix))
            .group_name(group.clone())
            .spacing(ListItemSpacing::Sparse)
            .toggle(Some(!collapsed))
            .on_toggle(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_section(kind, cx)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_section(kind, cx)))
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(Label::new(kind.title()).size(LabelSize::Small))
                    .child(div().flex_1())
                    .child(div().visible_on_hover(group).child(actions))
                    .child(count_badge(section.entries.len(), cx)),
            )
            .into_any_element()
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
        let path: Arc<Path> = Arc::from(entry.repo_path.as_std_path());
        let file_name: SharedString = entry
            .repo_path
            .file_name()
            .unwrap_or_default()
            .to_string()
            .into();
        let directory: SharedString = entry
            .repo_path
            .parent()
            .map(|parent| parent.display(path_style).to_string())
            .unwrap_or_default()
            .into();
        let (letter, color, description) = status_letter(entry.status, kind);
        let is_deleted = letter == "D";
        let icon = FileIcons::get_icon(&path, cx).map(|icon| {
            Icon::from_path(icon)
                .size(IconSize::Small)
                .color(Color::Muted)
        });
        let id = section_ix * 1_000_000 + entry_ix;
        let group: SharedString = format!("bolt-scm-entry-{id}").into();
        let tooltip = format!("{} • {description}", entry.repo_path.display(path_style));

        let open_entry = entry.clone();
        let discard_entry = entry.clone();
        let stage_path = entry.repo_path.clone();
        let actions = h_flex()
            .gap_0p5()
            .when(!is_deleted, |this| {
                this.child(
                    IconButton::new(("bolt-scm-open-file", id), IconName::File)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Open File"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.open_file(&open_entry, window, cx)
                        })),
                )
            })
            .when(kind == SectionKind::Changes, |this| {
                this.child(
                    IconButton::new(("bolt-scm-discard", id), IconName::Undo)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Discard Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.discard(vec![discard_entry.clone()], window, cx)
                        })),
                )
            })
            .child(match kind {
                SectionKind::Staged => IconButton::new(("bolt-scm-unstage", id), IconName::Dash)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text("Unstage Changes"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.stage(vec![stage_path.clone()], false, cx)
                    })),
                SectionKind::Changes | SectionKind::Merge => {
                    IconButton::new(("bolt-scm-stage", id), IconName::Plus)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Stage Changes"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.stage(vec![stage_path.clone()], true, cx)
                        }))
                }
            });

        ListItem::new(("bolt-scm-entry", id))
            .group_name(group.clone())
            .spacing(ListItemSpacing::Sparse)
            .indent_level(1)
            .indent_step_size(px(12.))
            .toggle_state(self.selected == Some(Row::Entry(section_ix, entry_ix)))
            .tooltip(Tooltip::text(tooltip))
            .start_slot::<Icon>(icon)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .child(
                        Label::new(file_name)
                            .size(LabelSize::Small)
                            .color(color)
                            .when(is_deleted, |label| label.strikethrough()),
                    )
                    .child(
                        Label::new(directory)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    )
                    .child(div().flex_1())
                    .child(div().visible_on_hover(group).child(actions))
                    .child(
                        div()
                            .w_3()
                            .flex_none()
                            .child(Label::new(letter).size(LabelSize::Small).color(color)),
                    ),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_diff(section_ix, entry_ix, window, cx)
            }))
            .into_any_element()
    }
}

fn count_badge(count: usize, cx: &App) -> impl IntoElement {
    div()
        .px_1p5()
        .rounded_full()
        .bg(cx.theme().colors().element_background)
        .child(
            Label::new(count.to_string())
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
}

/// VS Code's status letter, color and tooltip description for a file in a section.
fn status_letter(status: FileStatus, kind: SectionKind) -> (&'static str, Color, &'static str) {
    match status {
        FileStatus::Untracked => ("U", Color::VersionControlAdded, "Untracked"),
        FileStatus::Ignored => ("I", Color::Ignored, "Ignored"),
        FileStatus::Unmerged(_) => ("!", Color::VersionControlConflict, "Conflict"),
        FileStatus::Tracked(tracked) => {
            if kind == SectionKind::Merge {
                return ("!", Color::VersionControlConflict, "Conflict");
            }
            let staged = kind == SectionKind::Staged;
            let code = if staged {
                tracked.index_status
            } else {
                tracked.worktree_status
            };
            match (code, staged) {
                (StatusCode::Added, true) => ("A", Color::VersionControlAdded, "Index Added"),
                (StatusCode::Added, false) => ("A", Color::VersionControlAdded, "Added"),
                (StatusCode::Deleted, true) => ("D", Color::VersionControlDeleted, "Index Deleted"),
                (StatusCode::Deleted, false) => ("D", Color::VersionControlDeleted, "Deleted"),
                (StatusCode::Renamed, true) => {
                    ("R", Color::VersionControlModified, "Index Renamed")
                }
                (StatusCode::Renamed, false) => ("R", Color::VersionControlModified, "Renamed"),
                (StatusCode::Copied, _) => ("C", Color::VersionControlModified, "Copied"),
                (StatusCode::TypeChanged, _) => {
                    ("T", Color::VersionControlModified, "Type Changed")
                }
                (_, true) => ("M", Color::VersionControlModified, "Index Modified"),
                (_, false) => ("M", Color::VersionControlModified, "Modified"),
            }
        }
    }
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
            .child(self.render_header(cx))
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
