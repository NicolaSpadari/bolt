//! BOLT: VS Code-style search view in the left sidebar.
//!
//! Results are grouped by file and listed in the panel; clicking a match opens the file at that match (no
//! multibuffer). Spec: `notes/vscode-search-parity.md`.

use std::{ops::Range, path::Path, pin::pin, sync::Arc, time::Duration};

use anyhow::Result;
use collections::HashSet;
use editor::{Editor, EditorEvent, SelectionEffects, scroll::Autoscroll};
use file_icons::FileIcons;
use futures::StreamExt as _;
use gpui::{
    Action, App, AsyncWindowContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle,
    Focusable, HighlightStyle, Hsla, KeyContext, Pixels, Render, ScrollStrategy,
    StrikethroughStyle, StyledText, Subscription, Task, UniformListScrollHandle, WeakEntity,
    Window, actions, px, uniform_list,
};
use language::{Anchor, Buffer, BufferEvent, Point, ToPoint as _};
use project::{
    Project,
    search::{SearchQuery, SearchResult},
};
use theme::ActiveTheme as _;
use ui::{
    Color, Icon, IconButton, IconButtonShape, IconName, IconSize, Label, LabelSize, ListItem,
    ListItemSpacing, Tooltip, prelude::*,
};
use util::{ResultExt as _, paths::PathMatcher};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    searchable::SearchableItem as _,
};

const SEARCH_PANEL_KEY: &str = "BoltSearchPanel";
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(600);
const PREVIEW_CONTEXT_CHARS: usize = 24;

actions!(
    bolt_search,
    [
        /// Toggles focus on the search panel.
        ToggleFocus,
        /// Opens the search panel and focuses its query input, seeded with the current selection.
        FocusSearch,
        /// Opens the search panel with the replace input visible.
        ToggleReplace,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<SearchPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &FocusSearch, window, cx| {
            deploy(workspace, false, window, cx);
        });
        workspace.register_action(|workspace, _: &ToggleReplace, window, cx| {
            deploy(workspace, true, window, cx);
        });
    })
    .detach();
}

fn deploy(
    workspace: &mut Workspace,
    show_replace: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let seed = workspace
        .active_item_as::<Editor>(cx)
        .map(|editor| editor.update(cx, |editor, cx| editor.query_suggestion(None, window, cx)))
        .filter(|seed| !seed.is_empty() && !seed.contains('\n'));
    let Some(panel) = workspace.focus_panel::<SearchPanel>(window, cx) else {
        return;
    };
    panel.update(cx, |panel, cx| {
        if show_replace {
            panel.replace_enabled = true;
        }
        if let Some(seed) = seed {
            panel
                .query_editor
                .update(cx, |editor, cx| editor.set_text(seed, window, cx));
        }
        let focus_handle = if show_replace && !panel.query_editor.read(cx).text(cx).is_empty() {
            panel.replace_editor.focus_handle(cx)
        } else {
            panel.query_editor.focus_handle(cx)
        };
        panel.query_editor.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx)
        });
        window.focus(&focus_handle, cx);
        cx.notify();
    });
}

struct MatchResult {
    range: Range<Anchor>,
    /// Trimmed line preview and the byte range of the match inside it.
    preview: SharedString,
    highlight: Range<usize>,
    /// Text the match will be replaced with, if it needs line context (regex captures).
    line_text: String,
    hit_in_line: Range<usize>,
}

struct FileResult {
    buffer: Entity<Buffer>,
    path: Arc<Path>,
    file_name: SharedString,
    directory: SharedString,
    matches: Vec<MatchResult>,
    collapsed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    File(usize),
    Match(usize, usize),
}

pub struct SearchPanel {
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    focus_handle: FocusHandle,
    query_editor: Entity<Editor>,
    replace_editor: Entity<Editor>,
    include_editor: Entity<Editor>,
    exclude_editor: Entity<Editor>,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    replace_enabled: bool,
    filters_enabled: bool,
    use_ignore_files: bool,
    results: Vec<FileResult>,
    active_query: Option<SearchQuery>,
    searching: bool,
    limit_reached: bool,
    error: Option<SharedString>,
    selected_row: Option<Row>,
    scroll_handle: UniformListScrollHandle,
    pending_search: Option<Task<()>>,
    debounce: Option<Task<()>>,
    /// Files collapsed by the user, kept collapsed across live refreshes.
    collapsed_paths: HashSet<Arc<Path>>,
    buffer_subscriptions: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl SearchPanel {
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
            let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    let mut editor = Editor::single_line(window, cx);
                    editor.set_placeholder_text(placeholder, window, cx);
                    editor
                })
            };
            let query_editor = input("Search", window, cx);
            let replace_editor = input("Replace", window, cx);
            let include_editor = input("e.g. *.ts, src/**/include", window, cx);
            let exclude_editor = input("e.g. *.ts, src/**/exclude", window, cx);

            let mut subscriptions = Vec::new();
            for editor in [&query_editor, &include_editor, &exclude_editor] {
                subscriptions.push(cx.subscribe_in(
                    editor,
                    window,
                    |this: &mut Self, _, event: &EditorEvent, _, cx| {
                        if let EditorEvent::BufferEdited = event {
                            this.schedule_search(cx);
                        }
                    },
                ));
            }
            subscriptions.push(cx.subscribe_in(
                &replace_editor,
                window,
                |_: &mut Self, _, event: &EditorEvent, _, cx| {
                    if let EditorEvent::BufferEdited = event {
                        cx.notify();
                    }
                },
            ));
            // Live update: re-run the search when non-ignored files change on disk.
            subscriptions.push(cx.subscribe(
                &project,
                |this: &mut Self, project, event: &project::Event, cx| {
                    if let project::Event::WorktreeUpdatedEntries(worktree_id, entries) = event
                        && this.active_query.is_some()
                    {
                        let include_ignored = !this.use_ignore_files;
                        let relevant = project
                            .read(cx)
                            .worktree_for_id(*worktree_id, cx)
                            .is_some_and(|worktree| {
                                let worktree = worktree.read(cx);
                                entries.iter().any(|(_, entry_id, _)| {
                                    worktree.entry_for_id(*entry_id).is_none_or(|entry| {
                                        entry.is_file() && (include_ignored || !entry.is_ignored)
                                    })
                                })
                            });
                        if relevant {
                            this.schedule_refresh(cx);
                        }
                    }
                },
            ));

            Self {
                workspace: weak_workspace,
                project,
                focus_handle: cx.focus_handle(),
                query_editor,
                replace_editor,
                include_editor,
                exclude_editor,
                case_sensitive: false,
                whole_word: false,
                regex: false,
                replace_enabled: false,
                filters_enabled: false,
                use_ignore_files: true,
                results: Vec::new(),
                active_query: None,
                searching: false,
                limit_reached: false,
                error: None,
                selected_row: None,
                scroll_handle: UniformListScrollHandle::new(),
                pending_search: None,
                debounce: None,
                collapsed_paths: HashSet::default(),
                buffer_subscriptions: Vec::new(),
                _subscriptions: subscriptions,
            }
        })
    }

    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| this.search(cx)).log_err();
        }));
    }

    /// Re-runs the current search after files changed, keeping collapsed files collapsed.
    fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        if self.debounce.is_some() {
            return;
        }
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            this.update(cx, |this, cx| this.run_search(true, cx))
                .log_err();
        }));
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        self.run_search(false, cx);
    }

    fn build_query(&self, cx: &App) -> Result<Option<SearchQuery>> {
        let text = self.query_editor.read(cx).text(cx);
        if text.is_empty() {
            return Ok(None);
        }
        let path_style = self.project.read(cx).path_style(cx);
        let matcher = |editor: &Entity<Editor>| -> Result<PathMatcher> {
            if !self.filters_enabled {
                return Ok(PathMatcher::default());
            }
            let globs: Vec<String> = split_glob_patterns(&editor.read(cx).text(cx))
                .into_iter()
                .map(str::trim)
                .filter(|glob| !glob.is_empty())
                .map(str::to_owned)
                .collect();
            Ok(PathMatcher::new(&globs, path_style)?)
        };
        let include = matcher(&self.include_editor)?;
        let exclude = matcher(&self.exclude_editor)?;
        let match_full_paths = self.project.read(cx).visible_worktrees(cx).count() > 1;
        let include_ignored = !self.use_ignore_files;
        let query = if self.regex {
            SearchQuery::regex(
                text,
                self.whole_word,
                self.case_sensitive,
                include_ignored,
                false,
                include,
                exclude,
                match_full_paths,
                None,
            )?
        } else {
            SearchQuery::text(
                text,
                self.whole_word,
                self.case_sensitive,
                include_ignored,
                include,
                exclude,
                match_full_paths,
                None,
            )?
        };
        Ok(Some(query))
    }

    fn run_search(&mut self, keep_collapsed: bool, cx: &mut Context<Self>) {
        self.debounce = None;
        self.pending_search = None;
        self.buffer_subscriptions.clear();
        if keep_collapsed {
            self.collapsed_paths = self
                .results
                .iter()
                .filter(|file| file.collapsed)
                .map(|file| file.path.clone())
                .collect();
        } else {
            self.collapsed_paths.clear();
        }
        self.results.clear();
        self.selected_row = None;
        self.limit_reached = false;
        self.error = None;
        let query = match self.build_query(cx) {
            Ok(Some(query)) => query,
            Ok(None) => {
                self.active_query = None;
                self.searching = false;
                cx.notify();
                return;
            }
            Err(error) => {
                self.active_query = None;
                self.searching = false;
                self.error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        self.active_query = Some(query.clone());
        self.searching = true;
        let search = self
            .project
            .update(cx, |project, cx| project.search(query, cx));
        self.pending_search = Some(cx.spawn(async move |this, cx| {
            let task_handle = search.task_handle;
            let mut chunks = pin!(search.rx.ready_chunks(256));
            while let Some(results) = chunks.next().await {
                let keep_going = this
                    .update(cx, |this, cx| {
                        for result in results {
                            match result {
                                SearchResult::Buffer { buffer, ranges } => {
                                    this.add_result(buffer, ranges, cx)
                                }
                                SearchResult::LimitReached => this.limit_reached = true,
                                SearchResult::WaitingForScan | SearchResult::Searching => {}
                            }
                        }
                        cx.notify();
                    })
                    .is_ok();
                if !keep_going {
                    break;
                }
            }
            drop(task_handle);
            this.update(cx, |this, cx| {
                this.searching = false;
                cx.notify();
            })
            .log_err();
        }));
        cx.notify();
    }

    fn add_result(
        &mut self,
        buffer: Entity<Buffer>,
        ranges: Vec<Range<Anchor>>,
        cx: &mut Context<Self>,
    ) {
        if ranges.is_empty() {
            return;
        }
        // Live update: unsaved edits to a file with results re-run the search.
        self.buffer_subscriptions.push(cx.subscribe(
            &buffer,
            |this: &mut Self, _, event: &BufferEvent, cx| {
                if let BufferEvent::Edited { .. } = event {
                    this.schedule_refresh(cx);
                }
            },
        ));
        let buffer_ref = buffer.read(cx);
        let Some(file) = buffer_ref.file() else {
            return;
        };
        let rel_path = file.path().clone();
        let path: Arc<Path> = Arc::from(rel_path.as_std_path());
        let file_name: SharedString = rel_path.file_name().unwrap_or_default().to_string().into();
        let directory: SharedString = rel_path
            .parent()
            .map(|parent| parent.as_unix_str().to_string())
            .unwrap_or_default()
            .into();
        let snapshot = buffer_ref.snapshot();
        let matches = ranges
            .into_iter()
            .map(|range| {
                let start = range.start.to_point(&snapshot);
                let end = range.end.to_point(&snapshot);
                let line_end = Point::new(start.row, snapshot.line_len(start.row));
                let line_text: String = snapshot
                    .text_for_range(Point::new(start.row, 0)..line_end)
                    .collect();
                let hit_start = start.column as usize;
                let hit_end = if end.row == start.row {
                    end.column as usize
                } else {
                    line_text.len()
                };
                let (preview, highlight) = preview_line(&line_text, hit_start..hit_end);
                MatchResult {
                    range,
                    preview: preview.into(),
                    highlight,
                    line_text,
                    hit_in_line: hit_start..hit_end,
                }
            })
            .collect();
        let collapsed = self.collapsed_paths.contains(&path);
        let result = FileResult {
            buffer,
            path,
            file_name,
            directory,
            matches,
            collapsed,
        };
        let position = self
            .results
            .binary_search_by(|existing| existing.path.cmp(&result.path))
            .unwrap_or_else(|position| position);
        self.results.insert(position, result);
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (file_ix, file) in self.results.iter().enumerate() {
            rows.push(Row::File(file_ix));
            if !file.collapsed {
                rows.extend((0..file.matches.len()).map(|match_ix| Row::Match(file_ix, match_ix)));
            }
        }
        rows
    }

    fn match_count(&self) -> usize {
        self.results.iter().map(|file| file.matches.len()).sum()
    }

    fn open_match(
        &mut self,
        file_ix: usize,
        match_ix: usize,
        focus_item: bool,
        allow_preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.results.get(file_ix) else {
            return;
        };
        let Some(search_match) = file.matches.get(match_ix) else {
            return;
        };
        self.selected_row = Some(Row::Match(file_ix, match_ix));
        let buffer = file.buffer.clone();
        let range = search_match.range.clone();
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            let snapshot = buffer.read(cx).snapshot();
            let start = range.start.to_point(&snapshot);
            let end = range.end.to_point(&snapshot);
            let editor = workspace.open_project_item::<Editor>(
                None,
                buffer,
                true,
                focus_item,
                true,
                allow_preview,
                window,
                cx,
            );
            editor.update(cx, |editor, cx| {
                editor.change_selections(
                    SelectionEffects::scroll(Autoscroll::center()),
                    window,
                    cx,
                    |selections| selections.select_ranges([start..end]),
                );
            });
        });
        cx.notify();
    }

    fn replacement_for(&self, search_match: &MatchResult, cx: &App) -> Option<String> {
        let query = self
            .active_query
            .clone()?
            .with_replacement(self.replace_editor.read(cx).text(cx));
        if query.replacement_requires_context() {
            query
                .replacement_for(&search_match.line_text, search_match.hit_in_line.clone())
                .map(|replacement| replacement.into_owned())
        } else {
            query.replacement().map(str::to_owned)
        }
    }

    /// Replaces the given matches of one file (all of them when `match_ix` is `None`), saves it and drops them.
    fn replace_in_file(&mut self, file_ix: usize, match_ix: Option<usize>, cx: &mut Context<Self>) {
        let Some(file) = self.results.get(file_ix) else {
            return;
        };
        let indices: Vec<usize> = match match_ix {
            Some(match_ix) => vec![match_ix],
            None => (0..file.matches.len()).collect(),
        };
        let edits: Vec<(Range<Anchor>, String)> = indices
            .iter()
            .filter_map(|&ix| {
                let search_match = file.matches.get(ix)?;
                Some((
                    search_match.range.clone(),
                    self.replacement_for(search_match, cx)?,
                ))
            })
            .collect();
        if edits.is_empty() {
            return;
        }
        let buffer = file.buffer.clone();
        buffer.update(cx, |buffer, cx| {
            buffer.edit(edits, None, cx);
        });
        self.project
            .update(cx, |project, cx| project.save_buffer(buffer, cx))
            .detach_and_log_err(cx);
        self.dismiss(file_ix, match_ix, cx);
    }

    fn replace_all(&mut self, cx: &mut Context<Self>) {
        for file_ix in (0..self.results.len()).rev() {
            self.replace_in_file(file_ix, None, cx);
        }
    }

    fn dismiss(&mut self, file_ix: usize, match_ix: Option<usize>, cx: &mut Context<Self>) {
        match match_ix {
            Some(match_ix) => {
                if let Some(file) = self.results.get_mut(file_ix) {
                    if match_ix < file.matches.len() {
                        file.matches.remove(match_ix);
                    }
                    if file.matches.is_empty() {
                        self.results.remove(file_ix);
                    }
                }
            }
            None => {
                if file_ix < self.results.len() {
                    self.results.remove(file_ix);
                }
            }
        }
        self.selected_row = None;
        cx.notify();
    }

    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_search = None;
        self.debounce = None;
        self.results.clear();
        self.active_query = None;
        self.searching = false;
        self.limit_reached = false;
        self.error = None;
        self.selected_row = None;
        self.query_editor
            .update(cx, |editor, cx| editor.set_text("", window, cx));
        cx.notify();
    }

    fn set_all_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        for file in &mut self.results {
            file.collapsed = collapsed;
        }
        self.selected_row = None;
        cx.notify();
    }

    fn toggle_option(
        &mut self,
        option: fn(&mut Self) -> &mut bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = option(self);
        *value = !*value;
        self.search(cx);
    }

    fn select_next(&mut self, _: &menu::SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let rows = self.rows();
        let next = match self
            .selected_row
            .and_then(|row| rows.iter().position(|r| *r == row))
        {
            Some(ix) => (ix + 1).min(rows.len().saturating_sub(1)),
            None => 0,
        };
        if let Some(row) = rows.get(next) {
            self.selected_row = Some(*row);
            self.scroll_handle
                .scroll_to_item(next, ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.rows();
        let previous = match self
            .selected_row
            .and_then(|row| rows.iter().position(|r| *r == row))
        {
            Some(ix) => ix.saturating_sub(1),
            None => 0,
        };
        if let Some(row) = rows.get(previous) {
            self.selected_row = Some(*row);
            self.scroll_handle
                .scroll_to_item(previous, ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        match self.selected_row {
            Some(Row::Match(file_ix, match_ix)) => {
                self.open_match(file_ix, match_ix, true, false, window, cx)
            }
            Some(Row::File(file_ix)) => {
                if let Some(file) = self.results.get_mut(file_ix) {
                    file.collapsed = !file.collapsed;
                    cx.notify();
                }
            }
            None => {
                if self.results.is_empty() {
                    self.search(cx);
                } else {
                    self.open_match(0, 0, true, false, window, cx);
                }
            }
        }
    }

    fn render_input(
        &self,
        editor: &Entity<Editor>,
        buttons: Vec<AnyElement>,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors();
        let focused = editor.focus_handle(cx).contains_focused(window, cx);
        h_flex()
            .w_full()
            .min_w_0()
            .h_7()
            .pl_1p5()
            .pr_0p5()
            .gap_1()
            .rounded_sm()
            .border_1()
            .border_color(if focused {
                colors.border_focused
            } else {
                colors.border
            })
            .bg(colors.editor_background)
            .child(div().flex_1().min_w_0().child(editor.clone()))
            .children(buttons)
    }

    fn option_button(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: &'static str,
        active: bool,
        option: fn(&mut Self) -> &mut bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        IconButton::new(id, icon)
            .shape(IconButtonShape::Square)
            .icon_size(IconSize::Small)
            .toggle_state(active)
            .tooltip(Tooltip::text(tooltip))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.toggle_option(option, window, cx)
            }))
            .into_any_element()
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let has_results = !self.results.is_empty();
        let all_collapsed = has_results && self.results.iter().all(|file| file.collapsed);
        h_flex()
            .w_full()
            .px_2()
            .h_8()
            .justify_between()
            .child(
                Label::new("SEARCH")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_0p5()
                    .child(
                        IconButton::new("bolt-search-refresh", IconName::RotateCw)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Refresh"))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.search(cx))),
                    )
                    .child(
                        IconButton::new("bolt-search-clear", IconName::Close)
                            .icon_size(IconSize::Small)
                            .disabled(!has_results && self.active_query.is_none())
                            .tooltip(Tooltip::text("Clear Search Results"))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.clear(window, cx)
                            })),
                    )
                    .child(
                        IconButton::new(
                            "bolt-search-collapse",
                            if all_collapsed {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronUp
                            },
                        )
                        .icon_size(IconSize::Small)
                        .disabled(!has_results)
                        .tooltip(Tooltip::text(if all_collapsed {
                            "Expand All"
                        } else {
                            "Collapse All"
                        }))
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.set_all_collapsed(!all_collapsed, cx)
                            },
                        )),
                    ),
            )
    }

    fn render_inputs(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let replace_enabled = self.replace_enabled;
        let query_buttons = vec![
            self.option_button(
                "bolt-search-case",
                IconName::CaseSensitive,
                "Match Case",
                self.case_sensitive,
                |this| &mut this.case_sensitive,
                cx,
            ),
            self.option_button(
                "bolt-search-word",
                IconName::WholeWord,
                "Match Whole Word",
                self.whole_word,
                |this| &mut this.whole_word,
                cx,
            ),
            self.option_button(
                "bolt-search-regex",
                IconName::Regex,
                "Use Regular Expression",
                self.regex,
                |this| &mut this.regex,
                cx,
            ),
        ];
        let replace_buttons = vec![
            IconButton::new("bolt-search-replace-all", IconName::ReplaceAll)
                .shape(IconButtonShape::Square)
                .icon_size(IconSize::Small)
                .disabled(self.results.is_empty())
                .tooltip(Tooltip::text("Replace All"))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.replace_all(cx)))
                .into_any_element(),
        ];

        v_flex()
            .w_full()
            .px_2()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_0p5()
                    .child(
                        IconButton::new(
                            "bolt-search-toggle-replace",
                            if replace_enabled {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            },
                        )
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Toggle Replace"))
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.replace_enabled = !this.replace_enabled;
                                if this.replace_enabled {
                                    window.focus(&this.replace_editor.focus_handle(cx), cx);
                                }
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(self.render_input(&self.query_editor, query_buttons, window, cx))
                            .when(replace_enabled, |this| {
                                this.child(self.render_input(
                                    &self.replace_editor,
                                    replace_buttons,
                                    window,
                                    cx,
                                ))
                            }),
                    ),
            )
            .child(
                // Like VS Code, the "…" toggle floats under the inputs without taking vertical space.
                div().relative().w_full().h_0().child(
                    div().absolute().right_0().top_0().child(
                        IconButton::new("bolt-search-toggle-filters", IconName::Ellipsis)
                            .icon_size(IconSize::XSmall)
                            .toggle_state(self.filters_enabled)
                            .tooltip(Tooltip::text("Toggle Search Details"))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.filters_enabled = !this.filters_enabled;
                                this.search(cx);
                            })),
                    ),
                ),
            )
            .when(self.filters_enabled, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .pl_6()
                        .child(
                            Label::new("files to include")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(self.render_input(&self.include_editor, Vec::new(), window, cx))
                        .child(
                            Label::new("files to exclude")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(self.render_input(
                            &self.exclude_editor,
                            vec![self.option_button(
                                "bolt-search-ignore-files",
                                IconName::FileIgnored,
                                "Use Exclude Settings and Ignore Files",
                                self.use_ignore_files,
                                |this| &mut this.use_ignore_files,
                                cx,
                            )],
                            window,
                            cx,
                        )),
                )
            })
    }

    fn render_summary(&self) -> Option<impl IntoElement> {
        let message: SharedString = if let Some(error) = &self.error {
            error.clone()
        } else if self.active_query.is_none() {
            return None;
        } else if self.searching && self.results.is_empty() {
            "Searching…".into()
        } else if self.results.is_empty() {
            "No results found.".into()
        } else {
            let matches = self.match_count();
            let files = self.results.len();
            let mut message = format!(
                "{matches} result{} in {files} file{}",
                if matches == 1 { "" } else { "s" },
                if files == 1 { "" } else { "s" },
            );
            if self.limit_reached {
                message.push_str(" (search limit reached)");
            }
            message.into()
        };
        Some(
            div()
                .px_3()
                .py_1()
                .child(
                    Label::new(message)
                        .size(LabelSize::Small)
                        .color(if self.error.is_some() {
                            Color::Error
                        } else {
                            Color::Muted
                        }),
                ),
        )
    }

    fn render_row(&self, row: Row, cx: &Context<Self>) -> AnyElement {
        let is_selected = self.selected_row == Some(row);
        match row {
            Row::File(file_ix) => {
                let file = &self.results[file_ix];
                let icon = FileIcons::get_icon(&file.path, cx).map(|icon| {
                    Icon::from_path(icon)
                        .color(Color::Muted)
                        .size(IconSize::Small)
                });
                ListItem::new(("bolt-search-file", file_ix))
                    .spacing(ListItemSpacing::Sparse)
                    .toggle(Some(!file.collapsed))
                    .on_toggle(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(file) = this.results.get_mut(file_ix) {
                            file.collapsed = !file.collapsed;
                            cx.notify();
                        }
                    }))
                    .toggle_state(is_selected)
                    .start_slot::<Icon>(icon)
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_1p5()
                            .child(Label::new(file.file_name.clone()).size(LabelSize::Small))
                            .child(
                                Label::new(file.directory.clone())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .px_1p5()
                                    .rounded_full()
                                    .bg(cx.theme().colors().element_background)
                                    .child(
                                        Label::new(file.matches.len().to_string())
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            ),
                    )
                    .end_slot(
                        h_flex()
                            .gap_0p5()
                            .when(self.replace_enabled, |this| {
                                this.child(
                                    IconButton::new(
                                        ("bolt-search-replace-file", file_ix),
                                        IconName::ReplaceAll,
                                    )
                                    .icon_size(IconSize::XSmall)
                                    .tooltip(Tooltip::text("Replace All in File"))
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _, cx| {
                                            this.replace_in_file(file_ix, None, cx)
                                        },
                                    )),
                                )
                            })
                            .child(
                                IconButton::new(
                                    ("bolt-search-dismiss-file", file_ix),
                                    IconName::Close,
                                )
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Dismiss"))
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| {
                                        this.dismiss(file_ix, None, cx)
                                    },
                                )),
                            ),
                    )
                    .show_end_slot_on_hover()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.selected_row = Some(Row::File(file_ix));
                        if let Some(file) = this.results.get_mut(file_ix) {
                            file.collapsed = !file.collapsed;
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Row::Match(file_ix, match_ix) => {
                let search_match = &self.results[file_ix].matches[match_ix];
                let replacement = self
                    .replace_enabled
                    .then(|| self.replacement_for(search_match, cx))
                    .flatten();
                ListItem::new(("bolt-search-match", file_ix * 100_000 + match_ix))
                    .indent_level(1)
                    .indent_step_size(px(20.))
                    .spacing(ListItemSpacing::Sparse)
                    .toggle_state(is_selected)
                    .child(div().text_ui_sm(cx).child(render_preview(
                        search_match,
                        replacement,
                        cx,
                    )))
                    .end_slot(
                        h_flex()
                            .gap_0p5()
                            .when(self.replace_enabled, |this| {
                                this.child(
                                    IconButton::new(
                                        ("bolt-search-replace-match", file_ix * 100_000 + match_ix),
                                        IconName::Replace,
                                    )
                                    .icon_size(IconSize::XSmall)
                                    .tooltip(Tooltip::text("Replace"))
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _, cx| {
                                            this.replace_in_file(file_ix, Some(match_ix), cx)
                                        },
                                    )),
                                )
                            })
                            .child(
                                IconButton::new(
                                    ("bolt-search-dismiss-match", file_ix * 100_000 + match_ix),
                                    IconName::Close,
                                )
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Dismiss"))
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| {
                                        this.dismiss(file_ix, Some(match_ix), cx)
                                    },
                                )),
                            ),
                    )
                    .show_end_slot_on_hover()
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        let double_click = event.click_count() > 1;
                        this.open_match(file_ix, match_ix, double_click, !double_click, window, cx);
                    }))
                    .into_any_element()
            }
        }
    }
}

fn render_preview(search_match: &MatchResult, replacement: Option<String>, cx: &App) -> StyledText {
    let colors = cx.theme().colors();
    let preview = search_match.preview.to_string();
    let highlight = search_match.highlight.clone();
    match replacement {
        None => StyledText::new(preview).with_highlights([(
            highlight,
            HighlightStyle {
                background_color: Some(colors.search_match_background),
                ..Default::default()
            },
        )]),
        Some(replacement) => {
            // Old text struck through, followed by the replacement, like VS Code's replace preview.
            let mut text = String::with_capacity(preview.len() + replacement.len());
            text.push_str(&preview[..highlight.end]);
            let inserted = text.len()..text.len() + replacement.len();
            text.push_str(&replacement);
            text.push_str(&preview[highlight.end..]);
            StyledText::new(text).with_highlights([
                (
                    highlight,
                    HighlightStyle {
                        background_color: Some(with_alpha(colors.version_control_deleted, 0.25)),
                        strikethrough: Some(StrikethroughStyle {
                            thickness: px(1.),
                            color: None,
                        }),
                        ..Default::default()
                    },
                ),
                (
                    inserted,
                    HighlightStyle {
                        background_color: Some(with_alpha(colors.version_control_added, 0.25)),
                        ..Default::default()
                    },
                ),
            ])
        }
    }
}

fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// Trims the line around the match: leading whitespace removed, long prefixes shortened with an ellipsis.
fn preview_line(line: &str, hit: Range<usize>) -> (String, Range<usize>) {
    let hit_start = floor_char_boundary(line, hit.start.min(line.len()));
    let hit_end = floor_char_boundary(line, hit.end.min(line.len())).max(hit_start);
    let content_start = line.len() - line.trim_start().len();
    let mut start = content_start.min(hit_start);
    let prefix_chars = line[start..hit_start].chars().count();
    let mut ellipsis = false;
    if prefix_chars > PREVIEW_CONTEXT_CHARS {
        let skip = prefix_chars - PREVIEW_CONTEXT_CHARS;
        start = line[start..]
            .char_indices()
            .nth(skip)
            .map_or(hit_start, |(offset, _)| start + offset);
        ellipsis = true;
    }
    let mut preview = String::new();
    if ellipsis {
        preview.push('…');
    }
    let offset = preview.len();
    preview.push_str(line[start..].trim_end());
    let highlight_start = offset + (hit_start - start);
    let highlight_end = (offset + (hit_end - start)).min(preview.len());
    (preview, highlight_start..highlight_end.max(highlight_start))
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Splits comma-separated globs, keeping commas inside `{...}` and honouring `\` escapes.
fn split_glob_patterns(text: &str) -> Vec<&str> {
    let mut patterns = Vec::new();
    let mut depth = 0usize;
    let mut escaped = false;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                patterns.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    patterns.push(&text[start..]);
    patterns
}

impl Render for SearchPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows();
        let row_count = rows.len();
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("BoltSearchPanel");
        v_flex()
            .id("bolt-search-panel")
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm))
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(self.render_header(cx))
            .child(self.render_inputs(window, cx))
            .children(self.render_summary())
            .child(
                uniform_list(
                    "bolt-search-results",
                    row_count,
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
}

impl Focusable for SearchPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for SearchPanel {}

impl Panel for SearchPanel {
    fn persistent_name() -> &'static str {
        "Search Panel"
    }

    fn panel_key() -> &'static str {
        SEARCH_PANEL_KEY
    }

    fn activation_focus_handle(&self, cx: &App) -> FocusHandle {
        self.query_editor.focus_handle(cx)
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

    fn icon(&self, _: &Window, _: &App) -> Option<IconName> {
        Some(IconName::MagnifyingGlass)
    }

    fn icon_tooltip(&self, _: &Window, _: &App) -> Option<&'static str> {
        Some("Search")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        // Between the project panel (1) and the git panel (3), like VS Code's Explorer / Search / Source Control.
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_line() {
        let (preview, highlight) = preview_line("    let value = 42;", 8..13);
        assert_eq!(preview, "let value = 42;");
        assert_eq!(&preview[highlight], "value");

        let long = format!("{}needle tail", "x".repeat(60));
        let (preview, highlight) = preview_line(&long, 60..66);
        assert!(preview.starts_with('…'));
        assert_eq!(&preview[highlight], "needle");
    }

    #[test]
    fn test_split_glob_patterns() {
        assert_eq!(
            split_glob_patterns("*.ts, src/{a,b}/**"),
            vec!["*.ts", " src/{a,b}/**"]
        );
    }
}
