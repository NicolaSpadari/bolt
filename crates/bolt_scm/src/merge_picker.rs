//! BOLT: branch picker for "Merge Branch…": `git merge --no-commit --no-ff <branch>` into the current branch, leaving
//! the result uncommitted for review (same as the user's `branch:merge` alias).

use std::sync::Arc;

use git::repository::Branch;
use gpui::{App, Context, Entity, SharedString, Task, WeakEntity, Window};
use picker::{Picker, PickerDelegate};
use project::git_store::Repository;
use ui::{HighlightedLabel, ListItem, ListItemSpacing, prelude::*};
use workspace::{Toast, Workspace, notifications::NotificationId};

pub(crate) fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let Some(repository) = workspace.project().read(cx).active_repository(cx) else {
        return;
    };
    let branches: Vec<Branch> = repository
        .read(cx)
        .branch_list
        .iter()
        .filter(|branch| !branch.is_head)
        .cloned()
        .collect();
    let current = repository
        .read(cx)
        .branch
        .as_ref()
        .map(|branch| SharedString::from(branch.name().to_string()));
    let weak_workspace = workspace.weak_handle();
    workspace.toggle_modal(window, cx, |window, cx| {
        let delegate = MergePickerDelegate {
            workspace: weak_workspace,
            repository,
            current,
            matches: (0..branches.len()).map(|ix| (ix, Vec::new())).collect(),
            branches: Arc::from(branches),
            selected_index: 0,
        };
        Picker::uniform_list(delegate, window, cx)
    });
}

pub(crate) struct MergePickerDelegate {
    workspace: WeakEntity<Workspace>,
    repository: Entity<Repository>,
    current: Option<SharedString>,
    branches: Arc<[Branch]>,
    /// Indices into `branches` with the matched character positions of their names.
    matches: Vec<(usize, Vec<usize>)>,
    selected_index: usize,
}

impl PickerDelegate for MergePickerDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "BoltMergePicker"
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, _: &mut Context<Picker<Self>>) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        match &self.current {
            Some(current) => format!("Select a branch to merge into \"{current}\"").into(),
            None => "Select a branch to merge".into(),
        }
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.to_lowercase();
        self.matches = self
            .branches
            .iter()
            .enumerate()
            .filter_map(|(ix, branch)| {
                let name = branch.name().to_lowercase();
                if query.is_empty() {
                    return Some((ix, Vec::new()));
                }
                let start = name.find(&query)?;
                let positions = name[start..start + query.len()]
                    .char_indices()
                    .map(|(offset, _)| start + offset)
                    .collect();
                Some((ix, positions))
            })
            .collect();
        self.selected_index = 0;
        cx.notify();
        Task::ready(())
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some((branch_ix, _)) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(branch) = self.branches.get(*branch_ix) else {
            return;
        };
        merge_no_commit(
            self.workspace.clone(),
            self.repository.clone(),
            branch.name().to_string(),
            window,
            cx,
        );
        cx.emit(gpui::DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(gpui::DismissEvent);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let (branch_ix, positions) = self.matches.get(ix)?;
        let branch = self.branches.get(*branch_ix)?;
        let commit = branch.most_recent_commit.as_ref();
        let subject = commit
            .map(|commit| commit.subject.clone())
            .unwrap_or_default();
        let author = commit
            .map(|commit| commit.author_name.clone())
            .unwrap_or_default();
        let when = commit
            .map(|commit| relative_time(commit.commit_timestamp))
            .unwrap_or_default();
        Some(
            ListItem::new(("bolt-merge-branch", ix))
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(
                    Icon::new(IconName::GitBranch)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(HighlightedLabel::new(
                                    branch.name().to_string(),
                                    positions.clone(),
                                ))
                                .child(Label::new(when).size(LabelSize::Small).color(Color::Muted)),
                        )
                        .when(!subject.is_empty(), |this| {
                            this.child(
                                Label::new(format!("{author} • {subject}"))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                        }),
                ),
        )
    }
}

/// Runs `git merge --no-commit --no-ff <branch>` in the repository and reports the outcome.
pub(crate) fn merge_no_commit(
    workspace: WeakEntity<Workspace>,
    repository: Entity<Repository>,
    branch: String,
    window: &mut Window,
    cx: &mut App,
) {
    let work_directory = repository.read(cx).work_directory_abs_path.clone();
    let current = repository
        .read(cx)
        .branch
        .as_ref()
        .map(|branch| branch.name().to_string())
        .unwrap_or_else(|| "HEAD".to_string());
    let output = cx.background_spawn(async move {
        util::command::new_std_command("git")
            .args(["merge", "--no-commit", "--no-ff", &branch])
            .current_dir(work_directory.as_ref())
            .output()
            .map(|output| (branch, output))
    });
    window
        .spawn(cx, async move |cx| {
            let result = output.await;
            workspace.update(cx, |workspace, cx| {
                let message = match result {
                    Ok((branch, output)) if output.status.success() => format!(
                        "Merged \"{branch}\" into \"{current}\" (not committed: review and commit)."
                    ),
                    Ok((branch, output)) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let details = format!("{stdout}{stderr}");
                        let details = details.trim();
                        if details.contains("CONFLICT") {
                            format!(
                                "Merging \"{branch}\" into \"{current}\" has conflicts: resolve them in Merge Changes."
                            )
                        } else {
                            format!("Merge of \"{branch}\" failed: {details}")
                        }
                    }
                    Err(error) => format!("Could not run git merge: {error}"),
                };
                struct MergeToast;
                workspace.show_toast(
                    Toast::new(NotificationId::unique::<MergeToast>(), message).autohide(),
                    cx,
                );
            })
        })
        .detach_and_log_err(cx);
}

pub(crate) fn relative_time(timestamp: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(timestamp);
    let seconds = (now - timestamp).max(0);
    let (value, unit) = match seconds {
        0..60 => return "just now".to_string(),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        86_400..2_592_000 => (seconds / 86_400, "day"),
        2_592_000..31_536_000 => (seconds / 2_592_000, "month"),
        _ => (seconds / 31_536_000, "year"),
    };
    format!("{value} {unit}{} ago", if value == 1 { "" } else { "s" })
}
