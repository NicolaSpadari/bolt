//! BOLT: stash pickers of the source control panel, like VS Code's `git.stashApply` / `git.stashPop` /
//! `git.stashDrop` / `git.stashView` (`commands.ts` `pickStash`): one picker per action, so Enter does what the menu
//! item says (Zed's stash picker always applies on Enter).

use std::sync::Arc;

use git::stash::StashEntry;
use git_ui::commit_view::CommitView;
use gpui::{App, Context, Entity, PromptLevel, SharedString, Task, WeakEntity, Window};
use picker::{Picker, PickerDelegate};
use project::git_store::Repository;
use ui::{HighlightedLabel, ListItem, ListItemSpacing, prelude::*};
use workspace::Workspace;

use crate::merge_picker::relative_time;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Apply,
    Pop,
    Drop,
    View,
}

impl Mode {
    fn verb(self) -> &'static str {
        match self {
            Mode::Apply => "apply",
            Mode::Pop => "pop",
            Mode::Drop => "drop",
            Mode::View => "view",
        }
    }
}

pub(crate) fn open(
    workspace: &mut Workspace,
    mode: Mode,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(repository) = workspace.project().read(cx).active_repository(cx) else {
        return;
    };
    let entries = repository.read(cx).cached_stash().entries;
    let weak_workspace = workspace.weak_handle();
    workspace.toggle_modal(window, cx, |window, cx| {
        let delegate = StashPickerDelegate {
            mode,
            workspace: weak_workspace,
            repository,
            matches: (0..entries.len()).map(|ix| (ix, Vec::new())).collect(),
            entries,
            selected_index: 0,
        };
        Picker::uniform_list(delegate, window, cx)
    });
}

/// VS Code's stash item label: `#<index>: <message>`.
fn label(entry: &StashEntry) -> String {
    format!("#{}: {}", entry.index, entry.message)
}

pub(crate) struct StashPickerDelegate {
    mode: Mode,
    workspace: WeakEntity<Workspace>,
    repository: Entity<Repository>,
    entries: Arc<[StashEntry]>,
    /// Indices into `entries` with the matched character positions of their labels.
    matches: Vec<(usize, Vec<usize>)>,
    selected_index: usize,
}

impl PickerDelegate for StashPickerDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "BoltStashPicker"
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

    // Filtering moves rows under a still mouse; selecting on hover would make Enter pick that
    // row instead of the first match (VS Code's quick pick only highlights on hover).
    fn select_on_hover(&self) -> bool {
        false
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        format!("Pick a stash to {}", self.mode.verb()).into()
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        Some(if self.entries.is_empty() {
            "This repository has no stashes.".into()
        } else {
            "No matches".into()
        })
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.to_lowercase();
        self.matches = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(ix, entry)| {
                if query.is_empty() {
                    return Some((ix, Vec::new()));
                }
                let text = label(entry).to_lowercase();
                let start = text.find(&query)?;
                let positions = text[start..start + query.len()]
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
        let Some(entry) = self
            .matches
            .get(self.selected_index)
            .and_then(|(entry_ix, _)| self.entries.get(*entry_ix))
            .cloned()
        else {
            return;
        };
        let repository = self.repository.clone();
        let workspace = self.workspace.clone();
        let index = entry.index;
        match self.mode {
            Mode::View => CommitView::open(
                entry.oid.to_string(),
                repository.downgrade(),
                workspace,
                Some(index),
                None,
                window,
                cx,
            ),
            Mode::Apply | Mode::Pop => {
                let mode = self.mode;
                cx.spawn(async move |_, cx| {
                    let task = repository.update(cx, |repository, cx| {
                        if mode == Mode::Pop {
                            repository.stash_pop(Some(index), cx)
                        } else {
                            repository.stash_apply(Some(index), cx)
                        }
                    });
                    if let Err(error) = task.await {
                        report_error(workspace, mode, error, cx);
                    }
                })
                .detach();
            }
            Mode::Drop => {
                let prompt = window.prompt(
                    PromptLevel::Warning,
                    &format!(
                        "Are you sure you want to drop the stash: {}?",
                        entry.message
                    ),
                    None,
                    &["Yes", "Cancel"],
                    cx,
                );
                cx.spawn(async move |_, cx| {
                    if prompt.await != Ok(0) {
                        return;
                    }
                    let drop = repository
                        .update(cx, |repository, cx| repository.stash_drop(Some(index), cx));
                    let result = match drop.await {
                        Ok(result) => result,
                        Err(canceled) => Err(canceled.into()),
                    };
                    if let Err(error) = result {
                        report_error(workspace, Mode::Drop, error, cx);
                    }
                })
                .detach();
            }
        }
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
        let (entry_ix, positions) = self.matches.get(ix)?;
        let entry = self.entries.get(*entry_ix)?;
        let mut description = relative_time(entry.timestamp);
        if let Some(branch) = &entry.branch {
            description.push_str(" • ");
            description.push_str(branch);
        }
        Some(
            ListItem::new(("bolt-stash", ix))
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .child(HighlightedLabel::new(label(entry), positions.clone()).truncate())
                        .child(
                            Label::new(description)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                ),
        )
    }
}

fn report_error(
    workspace: WeakEntity<Workspace>,
    mode: Mode,
    error: anyhow::Error,
    cx: &mut gpui::AsyncApp,
) {
    log::error!("bolt_scm: git stash {} failed: {error:?}", mode.verb());
    if let Some(workspace) = workspace.upgrade() {
        cx.update(|cx| {
            git_ui_core::notifications::show_error_toast(
                workspace,
                format!("stash {}", mode.verb()),
                error,
                cx,
            )
        });
    }
}
