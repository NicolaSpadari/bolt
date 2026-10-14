//! BOLT: VS Code-style project name and branch in the status bar (bottom left) instead of the title bar.
//!
//! The project-items computation mirrors `TitleBar::render`; keep it in sync on upstream syncs.

use super::*;
use workspace::{HideStatusItem, StatusItemView, item::ItemHandle};

impl TitleBar {
    fn bolt_project_items(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let title_bar_settings = *TitleBarSettings::get_global(cx);
        let is_git_enabled = ProjectSettings::get_global(cx).git.enabled.status;

        let mut project_name = None;
        let mut repository = None;
        let mut linked_worktree_name = None;
        if let Some(worktree) = self.effective_active_worktree(cx) {
            repository = self.get_repository_for_worktree(&worktree, cx);
            let worktree_abs_path = worktree.read(cx).abs_path();
            project_name = worktree
                .read(cx)
                .root_name()
                .file_name()
                .map(|name| SharedString::from(name.to_string()));
            if let Some(repo) = &repository {
                let repo = repo.read(cx);
                let identity = repo_identity_path(&repo.common_dir_abs_path, repo.path_style);
                let identity_fallback =
                    repo_identity_path_if_local(&repo.common_dir_abs_path, repo.path_style);
                linked_worktree_name = linked_worktree_name_anchor(
                    repo.main_worktree_abs_path(),
                    identity_fallback,
                    repo.is_linked_worktree(),
                )
                .and_then(|name_anchor_path| {
                    linked_worktree_short_name(
                        name_anchor_path,
                        repo.work_directory_abs_path.as_ref(),
                    )
                })
                .or_else(|| {
                    repo.is_linked_worktree()
                        .then_some(project_name.clone())
                        .flatten()
                });

                let display_name = if identity.extension() == Some(std::ffi::OsStr::new("git")) {
                    identity.file_stem().and_then(|n| n.to_str())
                } else {
                    repo.path_style.file_name(identity)
                };

                if let Some(repo_name) = display_name {
                    let visible_worktrees_in_repo = self.visible_worktrees_in_repository(repo, cx);
                    let name = if visible_worktrees_in_repo == 1 {
                        if let Ok(relative) =
                            worktree_abs_path.strip_prefix(&*repo.work_directory_abs_path)
                        {
                            if relative.as_os_str().is_empty() {
                                repo_name.to_string()
                            } else {
                                format!("{}/{}", repo_name, relative.display())
                            }
                        } else {
                            repo_name.to_string()
                        }
                    } else {
                        repo_name.to_string()
                    };
                    project_name = Some(SharedString::from(name));
                }
            }
        }

        let web_url = repository.as_ref().and_then(|repository| {
            let repository = repository.read(cx);
            repository
                .remote_origin_url
                .as_deref()
                .or(repository.remote_upstream_url.as_deref())
                .and_then(remote_web_url)
        });

        h_flex()
            .gap_0p5()
            .when(title_bar_settings.show_project_items, |this| {
                this.child(
                    h_flex()
                        .gap_1()
                        .child(
                            Icon::new(IconName::Folder)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .child(self.render_project_name(project_name, window, cx)),
                )
            })
            .when_some(repository.filter(|_| is_git_enabled), |this, repository| {
                this.children(self.render_worktree_and_branch(repository, linked_worktree_name, cx))
            })
            .when_some(web_url, |this, web_url| {
                let icon = hosting_icon(&web_url);
                this.child(
                    IconButton::new("bolt-open-repository", icon)
                        .icon_size(IconSize::Small)
                        .icon_color(Color::Muted)
                        .tooltip(Tooltip::text("Open Repository in Browser"))
                        .on_click(move |_, _, cx| cx.open_url(&web_url)),
                )
            })
            .into_any_element()
    }
}

/// Converts a git remote (`git@host:owner/repo.git`, `ssh://git@host/owner/repo`, `https://host/owner/repo.git`) to
/// its web page URL.
fn remote_web_url(remote: &str) -> Option<String> {
    let remote = remote.trim();
    let (host, path) = if let Some(rest) = remote
        .strip_prefix("https://")
        .or_else(|| remote.strip_prefix("http://"))
        .or_else(|| remote.strip_prefix("ssh://"))
        .or_else(|| remote.strip_prefix("git://"))
    {
        let rest = rest
            .rsplit_once('@')
            .map_or(rest, |(_, host_and_path)| host_and_path);
        let (host, path) = rest.split_once('/')?;
        // Drop an explicit port (`ssh://git@host:22/owner/repo`).
        (host.split(':').next()?, path)
    } else {
        let rest = remote
            .rsplit_once('@')
            .map_or(remote, |(_, host_and_path)| host_and_path);
        rest.split_once(':')?
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("https://{host}/{path}"))
}

fn hosting_icon(web_url: &str) -> IconName {
    if web_url.contains("gitlab") {
        IconName::Gitlab
    } else if web_url.contains("bitbucket") {
        IconName::Bitbucket
    } else {
        IconName::Github
    }
}

#[cfg(test)]
mod bolt_tests {
    use super::remote_web_url;

    #[test]
    fn test_remote_web_url() {
        let expected = Some("https://github.com/owner/repo".to_string());
        assert_eq!(remote_web_url("git@github.com:owner/repo.git"), expected);
        assert_eq!(
            remote_web_url("https://github.com/owner/repo.git"),
            expected
        );
        assert_eq!(
            remote_web_url("https://user@github.com/owner/repo"),
            expected
        );
        assert_eq!(
            remote_web_url("ssh://git@github.com:22/owner/repo.git"),
            expected
        );
        assert_eq!(remote_web_url("/local/path"), None);
    }
}

/// Status bar item rendering the title bar's project name and branch buttons.
pub struct BoltProjectStatus {
    title_bar: Entity<TitleBar>,
    _observe_title_bar: Subscription,
}

impl BoltProjectStatus {
    pub fn new(title_bar: Entity<TitleBar>, cx: &mut Context<Self>) -> Self {
        let observe_title_bar = cx.observe(&title_bar, |_, _, cx| cx.notify());
        Self {
            title_bar,
            _observe_title_bar: observe_title_bar,
        }
    }
}

impl Render for BoltProjectStatus {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.title_bar
            .update(cx, |title_bar, cx| title_bar.bolt_project_items(window, cx))
    }
}

impl StatusItemView for BoltProjectStatus {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn hide_setting(&self, _: &App) -> Option<HideStatusItem> {
        // Hidden through `title_bar.show_project_items` / `title_bar.show_branch_name`.
        None
    }
}

pub(crate) fn add_to_status_bar(
    workspace: &mut Workspace,
    title_bar: &Entity<TitleBar>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let item = cx.new(|cx| BoltProjectStatus::new(title_bar.clone(), cx));
    workspace.status_bar().update(cx, |status_bar, cx| {
        status_bar.add_left_item(item, window, cx);
    });
}
