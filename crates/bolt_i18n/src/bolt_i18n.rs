//! BOLT: translated labels in place of i18n keys, like x18n's in-place annotations for VS Code.
//!
//! A key such as `t("main.close")` shows its value in the display locale instead of the key; the row holding a cursor
//! shows the raw key so it stays editable, and hovering a value shows its key. Labels are read from the project's locale
//! files and from the `i18n.paths` directories, structured like intool's projects (one file or one folder per locale,
//! JSON or Laravel PHP, flat or nested keys). Rules and sources: `notes/bolt-i18n.md`. Setting: `i18n`.

use std::{
    any::TypeId,
    collections::BTreeMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, SystemTime},
};

use collections::HashMap;
use editor::{Anchor, Editor, EditorEvent, FoldPlaceholder, HighlightKey, display_map::Crease};
use gpui::{
    App, AppContext as _, Context, Entity, HighlightStyle, Hsla, SharedString, Subscription, Task, WeakEntity, Window,
};
use multi_buffer::ToPoint as _;
use regex::Regex;
use settings::{RegisterSetting, Settings, SettingsLocation, SettingsStore};
use theme::ActiveTheme as _;
use theme_settings::ThemeSettings;
use ui::{Tooltip, prelude::*};

/// Files larger than this are not scanned.
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;
const SCAN_DEBOUNCE: Duration = Duration::from_millis(150);
/// Directories holding locale files, relative to the project root (nuxt-i18n, vue-i18n and Laravel conventions).
const PROJECT_DIRS: &[&str] = &[
    "i18n/locales",
    "locales",
    "lang",
    "src/locales",
    "src/i18n",
    "src/lang",
    "resources/lang",
];
/// x18n's vue-i18n and Laravel presets.
const DEFAULT_FUNCTIONS: &[&str] = &[
    "$t",
    "t",
    "tc",
    "$tc",
    "te",
    "$te",
    "tm",
    "$tm",
    "i18n.t",
    "i18n.global.t",
    "$i18n.t",
    "i18n.tc",
    "i18n.te",
    "__",
    "trans",
    "trans_choice",
    "@lang",
    "Lang::get",
    "Lang::choice",
];

#[derive(Debug, Clone, RegisterSetting)]
pub struct I18nSettings {
    pub enabled: bool,
    pub locale: String,
    pub paths: Vec<PathBuf>,
    pub functions: Vec<String>,
    pub max_length: usize,
    /// Project root → its extra translation directories and functions.
    pub projects: Vec<(PathBuf, Vec<PathBuf>, Vec<String>)>,
}

impl I18nSettings {
    /// The global `paths` / `functions` plus those of every `projects` entry holding `file_path`.
    fn for_file(&self, file_path: &Path) -> Self {
        let mut settings = self.clone();
        for (root, paths, functions) in &self.projects {
            if file_path.starts_with(root) {
                settings.paths.extend(paths.iter().cloned());
                settings.functions.extend(functions.iter().cloned());
            }
        }
        settings.projects.clear();
        settings
    }
}

/// `~/…` → the home directory.
fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

impl Settings for I18nSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let i18n = content.project.i18n.clone().unwrap_or_default();
        Self {
            enabled: i18n.enabled.unwrap_or(true),
            locale: i18n.locale.unwrap_or_else(|| "en".to_string()),
            paths: i18n.paths.unwrap_or_default().iter().map(|path| expand_home(path)).collect(),
            functions: i18n.functions.unwrap_or_default(),
            max_length: i18n.max_length.unwrap_or(40),
            projects: i18n
                .projects
                .unwrap_or_default()
                .into_iter()
                .map(|(root, project)| {
                    (
                        expand_home(&root),
                        project.paths.unwrap_or_default().iter().map(|path| expand_home(path)).collect(),
                        project.functions.unwrap_or_default(),
                    )
                })
                .collect(),
        }
    }
}

/// Fold type tag, so only Bolt's label folds are ever removed.
enum I18nFold {}

pub fn init(cx: &mut App) {
    cx.observe_new(|editor: &mut Editor, window: Option<&mut Window>, cx| {
        let Some(window) = window else {
            return;
        };
        if !editor.mode().is_full() || editor.buffer().read(cx).as_singleton().is_none() {
            return;
        }
        let editor_entity = cx.entity();
        let i18n = cx.new(|cx| I18n::new(editor_entity, window, cx));
        editor.register_addon(I18nAddon { _i18n: i18n });
    })
    .detach();
}

struct I18nAddon {
    _i18n: Entity<I18n>,
}

impl editor::Addon for I18nAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Label {
    /// Replaces the key. `full_value` is set when `value` was truncated to `max_length`.
    InPlace {
        value: SharedString,
        full_value: Option<SharedString>,
    },
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
struct Found {
    key_range: Range<usize>,
    key: SharedString,
    label: Label,
}

struct Annotation {
    range: Range<Anchor>,
    key: SharedString,
    label: Label,
}

struct I18n {
    editor: WeakEntity<Editor>,
    annotations: Vec<Annotation>,
    /// Rows holding a cursor the last time folds were applied; their keys show as text.
    revealed_rows: Vec<u32>,
    scan: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl I18n {
    fn new(editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor_subscription =
            cx.subscribe_in(&editor, window, |this, _, event: &EditorEvent, window, cx| {
                match event {
                    EditorEvent::BufferEdited | EditorEvent::Reparsed(_) | EditorEvent::Saved => {
                        this.schedule_scan(window, cx)
                    }
                    EditorEvent::SelectionsChanged { .. } => this.apply_folds(false, window, cx),
                    _ => {}
                }
            });
        let settings_subscription =
            cx.observe_global_in::<SettingsStore>(window, |this, window, cx| {
                this.schedule_scan(window, cx)
            });
        // The editor is still being created here, so it can't be read yet.
        cx.defer_in(window, |this, window, cx| this.schedule_scan(window, cx));
        Self {
            editor: editor.downgrade(),
            annotations: Vec::new(),
            revealed_rows: Vec::new(),
            scan: Task::ready(()),
            _subscriptions: vec![editor_subscription, settings_subscription],
        }
    }

    fn schedule_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let Some(buffer) = editor.read(cx).buffer().read(cx).as_singleton() else {
            return;
        };
        let buffer = buffer.read(cx);
        let Some(file) = buffer.file() else {
            return;
        };
        let Some(local_file) = file.as_local() else {
            return;
        };
        let settings = I18nSettings::get(
            Some(SettingsLocation {
                worktree_id: file.worktree_id(cx),
                path: file.path().as_ref(),
            }),
            cx,
        )
        .clone();
        let text = buffer.text();
        if !settings.enabled || text.len() > MAX_SCAN_BYTES {
            if !self.annotations.is_empty() {
                self.annotations.clear();
                self.apply_folds(true, window, cx);
            }
            return;
        }
        let file_path = local_file.abs_path(cx);
        let settings = settings.for_file(&file_path);
        let background = cx.background_executor().clone();
        self.scan = cx.spawn_in(window, async move |this, cx| {
            background.timer(SCAN_DEBOUNCE).await;
            let found = background
                .spawn(async move { find_labels(&text, &file_path, &settings) })
                .await;
            this.update_in(cx, |this, window, cx| {
                let Some(editor) = this.editor.upgrade() else {
                    return;
                };
                let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
                let anchor = |offset: usize| editor::MultiBufferOffset(offset);
                this.annotations = found
                    .into_iter()
                    .map(|found| Annotation {
                        range: snapshot.anchor_after(anchor(found.key_range.start))
                            ..snapshot.anchor_before(anchor(found.key_range.end)),
                        key: found.key,
                        label: found.label,
                    })
                    .collect();
                this.apply_folds(true, window, cx);
            })
            .ok();
        });
    }

    /// Folds every label except those on a row holding a cursor. With `force` (new scan results), also refreshes the
    /// missing-key highlights; without it, nothing happens unless the cursor rows changed.
    fn apply_folds(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let annotations = &self.annotations;
        let revealed_rows = &mut self.revealed_rows;
        editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let mut cursor_rows: Vec<u32> = editor
                .selections
                .disjoint_anchors()
                .iter()
                .flat_map(|selection| {
                    let start = selection.start.to_point(&snapshot).row;
                    let end = selection.end.to_point(&snapshot).row;
                    start..=end
                })
                .collect();
            cursor_rows.sort_unstable();
            cursor_rows.dedup();
            if !force && cursor_rows == *revealed_rows {
                return;
            }
            *revealed_rows = cursor_rows;

            let full_range = editor::MultiBufferOffset(0)..snapshot.len();
            editor.remove_folds_with_type(&[full_range], TypeId::of::<I18nFold>(), false, cx);
            let color = editor::make_inlay_hints_style(cx)
                .color
                .unwrap_or_else(|| cx.theme().status().hint);
            let creases = annotations
                .iter()
                .filter_map(|annotation| {
                    let Label::InPlace { value, full_value } = &annotation.label else {
                        return None;
                    };
                    let start = annotation.range.start.to_point(&snapshot).row;
                    let end = annotation.range.end.to_point(&snapshot).row;
                    let revealed = revealed_rows
                        .iter()
                        .any(|row| (start..=end).contains(row));
                    (start == end && !revealed).then(|| {
                        Crease::simple(
                            annotation.range.clone(),
                            label_placeholder(
                                value.clone(),
                                annotation.key.clone(),
                                full_value.clone(),
                                color,
                            ),
                        )
                    })
                })
                .collect::<Vec<_>>();
            editor.fold_creases(creases, false, window, cx);

            if !force {
                return;
            }
            editor.highlight_text(
                HighlightKey::BoltI18nMissing,
                annotations
                    .iter()
                    .filter(|annotation| annotation.label == Label::Missing)
                    .map(|annotation| annotation.range.clone())
                    .collect(),
                HighlightStyle {
                    color: Some(cx.theme().status().warning),
                    ..HighlightStyle::default()
                },
                cx,
            );
        });
    }
}

fn label_placeholder(
    value: SharedString,
    key: SharedString,
    full_value: Option<SharedString>,
    color: Hsla,
) -> FoldPlaceholder {
    FoldPlaceholder {
        render: Arc::new(move |fold_id, _, cx: &mut App| {
            let settings = ThemeSettings::get_global(cx);
            div()
                .id(fold_id)
                .h_full()
                .flex()
                .items_center()
                .font(settings.buffer_font.clone())
                .text_size(settings.buffer_font_size(cx))
                .text_color(color)
                .child(value.clone())
                .tooltip({
                    let key = key.clone();
                    let full_value = full_value.clone();
                    move |_, cx| match &full_value {
                        Some(full_value) => Tooltip::with_meta(key.clone(), None, full_value.clone(), cx),
                        None => Tooltip::simple(key.clone(), cx),
                    }
                })
                .into_any_element()
        }),
        constrain_width: false,
        merge_adjacent: false,
        type_tag: Some(TypeId::of::<I18nFold>()),
        collapsed_text: None,
    }
}

/// Key usages in `text` with their label in the display locale. Nothing is reported when no locale file is found, so
/// projects without i18n don't get every `t('…')` flagged as missing.
fn find_labels(text: &str, file_path: &Path, settings: &I18nSettings) -> Vec<Found> {
    let usages = find_usages(text, &settings.functions);
    if usages.is_empty() {
        return Vec::new();
    }
    let catalog = Catalog::load(file_path, settings);
    if catalog.sources.is_empty() {
        return Vec::new();
    }
    usages
        .into_iter()
        .filter_map(|usage| {
            let key_range = usage.key_range;
            let key = text.get(key_range.clone())?;
            let label = match catalog.lookup(key) {
                Lookup::Value(value) => {
                    let interpolation = if usage.component {
                        component_interpolation(text, key_range.start)
                    } else {
                        call_interpolation(text, key_range.end)
                    };
                    let value = interpolate(&collapse_newlines(value), &interpolation);
                    let truncated = truncate(&value, settings.max_length);
                    let full_value = (truncated != value).then(|| value.into());
                    Label::InPlace {
                        value: truncated.into(),
                        full_value,
                    }
                }
                Lookup::Namespace => return None,
                Lookup::Missing => Label::Missing,
            };
            Some(Found {
                key: SharedString::from(key.to_string()),
                key_range,
                label,
            })
        })
        .collect()
}

fn collapse_newlines(value: &str) -> String {
    static NEWLINES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*\n\s*").expect("Failed to create NEWLINES regex"));
    NEWLINES.replace_all(value, " ").into_owned()
}

/// x18n's `truncate`: at most `max` characters, the last one an ellipsis.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut truncated = text.chars().take(max.saturating_sub(1)).collect::<String>();
    truncated.push('…');
    truncated
}

struct Usage {
    key_range: Range<usize>,
    /// `<i18n-t keypath="key">` rather than a call.
    component: bool,
}

/// Static keys passed to translate functions (`t('key')`, `this.$lang("key")`) and `keypath="key"` attributes.
/// Template literals with `${` are dynamic and skipped.
fn find_usages(text: &str, extra_functions: &[String]) -> Vec<Usage> {
    let mut functions = DEFAULT_FUNCTIONS
        .iter()
        .map(|function| function.to_string())
        .chain(extra_functions.iter().cloned())
        .collect::<Vec<_>>();
    // Longest first, so `i18n.global.t` wins over `t` in the alternation.
    functions.sort_by_key(|function| std::cmp::Reverse(function.len()));
    functions.dedup();
    let alternation = functions
        .iter()
        .map(|function| regex::escape(function))
        .collect::<Vec<_>>()
        .join("|");
    let pattern = format!(
        r#"(?:^|[^\w$@])(?:{alternation})\(\s*(?:'([^'\\\n]+)'|"([^"\\\n]+)"|`([^`\\$\n]+)`)|\bkeypath="([^"\n]+)""#
    );
    let regex = match Regex::new(&pattern) {
        Ok(regex) => regex,
        Err(error) => {
            log::error!("i18n: invalid usage pattern: {error}");
            return Vec::new();
        }
    };
    regex
        .captures_iter(text)
        .filter_map(|captures| {
            if let Some(keypath) = captures.get(4) {
                return Some(Usage {
                    key_range: keypath.range(),
                    component: true,
                });
            }
            let key = captures.iter().skip(1).flatten().next()?;
            Some(Usage {
                key_range: key.range(),
                component: false,
            })
        })
        .collect()
}

/// Values passed to a message: placeholder name → expression, and the plural count expression.
#[derive(Debug, Default, PartialEq)]
struct Interpolation {
    named: Vec<(String, String)>,
    plural: Option<String>,
}

/// The arguments after the key of a call: `t(key, { name: expr })`, `t(key, [a, b])`, `t(key, count)`,
/// `t(key, { … }, count)` (vue-i18n) and `__(key, ['name' => $x])`, `trans_choice(key, $n, [...])` (Laravel).
fn call_interpolation(text: &str, key_end: usize) -> Interpolation {
    let mut interpolation = Interpolation::default();
    let Some(rest) = text.get(key_end..) else {
        return interpolation;
    };
    // Past the key's closing quote.
    let rest = rest.get(1..).unwrap_or_default();
    let Some(end) = top_level_find(rest, ")") else {
        return interpolation;
    };
    for argument in top_level_split(&rest[..end], ",").into_iter().skip(1) {
        let argument = argument.trim();
        if let Some(inner) = argument.strip_prefix('{').and_then(|inner| inner.strip_suffix('}')) {
            for entry in top_level_split(inner, ",") {
                let entry = entry.trim();
                if entry.is_empty() || entry.starts_with("...") {
                    continue;
                }
                match top_level_find(entry, ":") {
                    Some(colon) => interpolation.named.push((
                        unquote(&entry[..colon]).to_string(),
                        entry[colon + 1..].trim().to_string(),
                    )),
                    None => interpolation.named.push((entry.to_string(), entry.to_string())),
                }
            }
        } else if let Some(inner) = argument.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')) {
            for (index, entry) in top_level_split(inner, ",").into_iter().enumerate() {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                match top_level_find(entry, "=>") {
                    Some(arrow) => interpolation.named.push((
                        unquote(&entry[..arrow]).to_string(),
                        entry[arrow + 2..].trim().to_string(),
                    )),
                    None => interpolation.named.push((index.to_string(), entry.to_string())),
                }
            }
        } else if !argument.is_empty() && interpolation.plural.is_none() {
            interpolation.plural = Some(argument.to_string());
        }
    }
    interpolation
}

/// `<i18n-t keypath="key" :plural="expr">` and its `<template #name>{{ expr }}</template>` slots.
fn component_interpolation(text: &str, keypath_start: usize) -> Interpolation {
    static PLURAL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:^|\s)(?::|v-bind:)?plural="([^"]*)""#).expect("Failed to create PLURAL regex")
    });
    static SLOT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)<template\s+(?:#|v-slot:)([\w-]+)[^>]*>(.*?)</template>"#).expect("Failed to create SLOT regex")
    });
    let mut interpolation = Interpolation::default();
    let Some(tag_start) = text[..keypath_start].rfind("<i18n-t") else {
        return interpolation;
    };
    let Some(tag_end) = text[keypath_start..].find('>').map(|end| keypath_start + end) else {
        return interpolation;
    };
    let tag = &text[tag_start..tag_end];
    interpolation.plural = PLURAL
        .captures(tag)
        .and_then(|captures| captures.get(1))
        .map(|plural| plural.as_str().trim().to_string());
    let body_end = text[tag_end..]
        .find("</i18n-t>")
        .map_or(text.len(), |end| tag_end + end);
    for captures in SLOT.captures_iter(&text[tag_end..body_end]) {
        let (Some(name), Some(content)) = (captures.get(1), captures.get(2)) else {
            continue;
        };
        let content = content.as_str().trim();
        let expression = content
            .strip_prefix("{{")
            .and_then(|content| content.strip_suffix("}}"))
            .map(str::trim)
            .filter(|expression| !expression.is_empty() && !expression.contains("{{"))
            .unwrap_or(name.as_str());
        interpolation
            .named
            .push((name.as_str().to_string(), collapse_newlines(expression)));
    }
    interpolation
}

/// Picks the plural form (vue-i18n pipes, Laravel `{0}` / `[1,*]` prefixes) and shows each placeholder (`{name}`,
/// Laravel `:name`) as `{expression passed for it}`. With a count that is not a number literal, the last form (the
/// plural) is shown.
fn interpolate(message: &str, interpolation: &Interpolation) -> String {
    static PLACEHOLDER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\{\s*([\w.]+)\s*\}").expect("Failed to create PLACEHOLDER regex"));
    static COLON_PLACEHOLDER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r":([A-Za-z_]\w*)").expect("Failed to create COLON_PLACEHOLDER regex"));
    static LARAVEL_RANGE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:\{\d+\}|\[\d+\s*,\s*(?:\d+|\*)\])\s*").expect("Failed to create LARAVEL_RANGE regex")
    });
    let mut message = message.to_string();
    if let Some(count) = &interpolation.plural {
        let forms = message.split('|').map(str::trim).collect::<Vec<_>>();
        if forms.len() > 1 {
            let last = forms.len() - 1;
            let index = match count.parse::<u64>() {
                Ok(1) if forms.len() == 2 => 0,
                Ok(_) if forms.len() == 2 => 1,
                Ok(count) => usize::try_from(count).unwrap_or(last).min(2).min(last),
                Err(_) => last,
            };
            message = LARAVEL_RANGE.replace(forms[index], "").into_owned();
        }
    }
    let expression = |name: &str| -> Option<&str> {
        interpolation
            .named
            .iter()
            .find(|(placeholder, _)| placeholder == name)
            .map(|(_, expression)| expression.as_str())
            .or_else(|| {
                matches!(name, "n" | "count")
                    .then_some(interpolation.plural.as_deref())
                    .flatten()
            })
    };
    let message = PLACEHOLDER.replace_all(&message, |captures: &regex::Captures| {
        let whole = captures.get(0).map_or("", |whole| whole.as_str());
        match captures.get(1).and_then(|name| expression(name.as_str())) {
            Some(expression) => format!("{{{expression}}}"),
            None => whole.to_string(),
        }
    });
    if interpolation.named.is_empty() {
        return message.into_owned();
    }
    COLON_PLACEHOLDER
        .replace_all(&message, |captures: &regex::Captures| {
            let whole = captures.get(0).map_or("", |whole| whole.as_str());
            match captures.get(1).and_then(|name| expression(name.as_str())) {
                Some(expression) => format!("{{{expression}}}"),
                None => whole.to_string(),
            }
        })
        .into_owned()
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix(['\'', '"'])
        .and_then(|inner| inner.strip_suffix(['\'', '"']))
        .unwrap_or(text)
}

/// Byte offset of the first `pattern` outside brackets and string literals, at the nesting level `text` starts at.
fn top_level_find(text: &str, pattern: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == b'\\' {
                index += 1;
            } else if byte == open {
                quote = None;
            }
        } else if depth == 0 && text[index..].starts_with(pattern) {
            return Some(index);
        } else {
            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        index += 1;
    }
    None
}

fn top_level_split<'a>(text: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(index) = top_level_find(rest, separator) {
        parts.push(&rest[..index]);
        rest = &rest[index + separator.len()..];
    }
    parts.push(rest);
    parts
}

enum Lookup<'a> {
    Value(&'a str),
    /// The key names a group of keys (`t('main')`), not a label.
    Namespace,
    Missing,
}

type Messages = BTreeMap<String, String>;

struct Source {
    /// File stem prefixed to the keys of a folder-per-locale file (`main` for `en/main.php`).
    namespace: Option<String>,
    messages: Arc<Messages>,
}

struct Catalog {
    sources: Vec<Source>,
}

impl Catalog {
    fn load(file_path: &Path, settings: &I18nSettings) -> Self {
        let project_root = file_path
            .ancestors()
            .skip(1)
            .find(|dir| dir.join("package.json").is_file() || dir.join("composer.json").is_file());
        let project_dirs = project_root
            .into_iter()
            .flat_map(|root| PROJECT_DIRS.iter().map(move |dir| root.join(dir)));
        let setting_dirs = settings.paths.iter().filter(|path| path.is_absolute()).cloned();
        let sources = project_dirs
            .chain(setting_dirs)
            .filter(|dir| dir.is_dir())
            .flat_map(|dir| locale_files(&dir, &settings.locale))
            .filter_map(|(path, namespace)| {
                Some(Source {
                    namespace,
                    messages: load_messages(&path)?,
                })
            })
            .collect();
        Self { sources }
    }

    /// The first source holding `key` wins.
    fn lookup(&self, key: &str) -> Lookup<'_> {
        let mut namespace_found = false;
        for source in &self.sources {
            let local_key = match &source.namespace {
                Some(namespace) => {
                    let Some(rest) = key.strip_prefix(namespace.as_str()) else {
                        continue;
                    };
                    if rest.is_empty() {
                        namespace_found = true;
                        continue;
                    }
                    let Some(local_key) = rest.strip_prefix('.') else {
                        continue;
                    };
                    local_key
                }
                None => key,
            };
            if let Some(value) = source.messages.get(local_key) {
                return Lookup::Value(value);
            }
            let prefix = format!("{local_key}.");
            if source
                .messages
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(candidate, _)| candidate.starts_with(&prefix))
            {
                namespace_found = true;
            }
        }
        if namespace_found {
            Lookup::Namespace
        } else {
            Lookup::Missing
        }
    }
}

/// Locale files of `dir` for `locale`, with the namespace of their keys: intool's folder per language
/// (`en/main.php` → `main.*`) and single file per language (`en.json`, `translations_en.json`).
fn locale_files(dir: &Path, locale: &str) -> Vec<(PathBuf, Option<String>)> {
    let mut files = Vec::new();
    let locale_dir = dir.join(locale);
    if let Ok(entries) = std::fs::read_dir(&locale_dir) {
        let mut namespaced = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "json" || extension == "php")
            })
            .filter_map(|path| {
                let stem = path.file_stem()?.to_str()?.to_string();
                Some((path, Some(stem)))
            })
            .collect::<Vec<_>>();
        namespaced.sort();
        files.extend(namespaced);
    }
    for name in [format!("{locale}.json"), format!("translations_{locale}.json")] {
        let path = dir.join(name);
        if path.is_file() {
            files.push((path, None));
        }
    }
    files
}

/// Parsed messages per file, reused until the file's modification time changes.
static MESSAGES: LazyLock<Mutex<HashMap<PathBuf, (SystemTime, Arc<Messages>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::default()));

fn load_messages(path: &Path) -> Option<Arc<Messages>> {
    let modified = std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok()?;
    if let Ok(cache) = MESSAGES.lock()
        && let Some((cached_modified, messages)) = cache.get(path)
        && *cached_modified == modified
    {
        return Some(messages.clone());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            log::error!("i18n: can't read {}: {error}", path.display());
            return None;
        }
    };
    let mut messages = Messages::new();
    if path.extension().is_some_and(|extension| extension == "php") {
        match parse_php_array(&text) {
            Some(value) => flatten_php(&value, "", &mut messages),
            None => log::warn!("i18n: no `return [...]` array in {}", path.display()),
        }
    } else {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => flatten_json(&value, "", &mut messages),
            Err(error) => log::error!("i18n: invalid JSON in {}: {error}", path.display()),
        }
    }
    let messages = Arc::new(messages);
    if let Ok(mut cache) = MESSAGES.lock() {
        cache.insert(path.to_path_buf(), (modified, messages.clone()));
    }
    Some(messages)
}

fn join_key(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Nested objects and arrays become dotted keys; flat keys (`"header.support.TITLE"`) are kept as they are.
fn flatten_json(value: &serde_json::Value, prefix: &str, messages: &mut Messages) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                flatten_json(value, &join_key(prefix, key), messages);
            }
        }
        serde_json::Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                flatten_json(value, &join_key(prefix, &index.to_string()), messages);
            }
        }
        serde_json::Value::String(text) => {
            messages.insert(prefix.to_string(), text.clone());
        }
        serde_json::Value::Number(number) => {
            messages.insert(prefix.to_string(), number.to_string());
        }
        serde_json::Value::Bool(flag) => {
            messages.insert(prefix.to_string(), flag.to_string());
        }
        serde_json::Value::Null => {}
    }
}

#[derive(Debug, PartialEq)]
enum PhpValue {
    Text(String),
    Array(Vec<(String, PhpValue)>),
}

fn flatten_php(value: &PhpValue, prefix: &str, messages: &mut Messages) {
    match value {
        PhpValue::Text(text) => {
            messages.insert(prefix.to_string(), text.clone());
        }
        PhpValue::Array(entries) => {
            for (key, value) in entries {
                flatten_php(value, &join_key(prefix, key), messages);
            }
        }
    }
}

/// The array returned by a Laravel translation file (`return [...]` or `return array(...)`). Literal strings, numbers
/// and their `.` concatenations are kept; any other expression (constants, calls) is skipped.
fn parse_php_array(text: &str) -> Option<PhpValue> {
    let mut parser = PhpParser {
        bytes: text.as_bytes(),
        position: 0,
    };
    while parser.position < parser.bytes.len() {
        parser.skip_trivia();
        match parser.peek()? {
            b'\'' | b'"' => {
                parser.parse_string()?;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                if parser.identifier() == "return" {
                    return match parser.parse_value()? {
                        array @ PhpValue::Array(_) => Some(array),
                        PhpValue::Text(_) => None,
                    };
                }
            }
            _ => parser.position += 1,
        }
    }
    None
}

struct PhpParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl PhpParser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn starts_with(&self, prefix: &[u8]) -> bool {
        self.bytes
            .get(self.position..)
            .is_some_and(|rest| rest.starts_with(prefix))
    }

    /// Whitespace and comments (`//`, `#`, `/* */`); the `<?php` tag is skipped as an unknown token by the caller.
    fn skip_trivia(&mut self) {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace() {
                self.position += 1;
            } else if self.starts_with(b"//") || (byte == b'#' && !self.starts_with(b"#[")) {
                while self.peek().is_some_and(|byte| byte != b'\n') {
                    self.position += 1;
                }
            } else if self.starts_with(b"/*") {
                self.position += 2;
                while self.position < self.bytes.len() && !self.starts_with(b"*/") {
                    self.position += 1;
                }
                self.position = (self.position + 2).min(self.bytes.len());
            } else {
                break;
            }
        }
    }

    fn identifier(&mut self) -> String {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.position += 1;
        }
        String::from_utf8_lossy(&self.bytes[start..self.position]).into_owned()
    }

    /// A value, or `None` for an expression that is not a literal (skipped up to the next `,` or closing bracket).
    fn parse_value(&mut self) -> Option<PhpValue> {
        self.skip_trivia();
        let value = match self.peek()? {
            b'[' => {
                self.position += 1;
                self.parse_array(b']')
            }
            b'\'' | b'"' => self.parse_concatenation(),
            byte if byte.is_ascii_digit() || byte == b'-' => {
                let start = self.position;
                self.position += 1;
                while self
                    .peek()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_')
                {
                    self.position += 1;
                }
                Some(PhpValue::Text(
                    String::from_utf8_lossy(&self.bytes[start..self.position]).into_owned(),
                ))
            }
            byte if byte.is_ascii_alphabetic() => {
                let start = self.position;
                let identifier = self.identifier();
                self.skip_trivia();
                if identifier.eq_ignore_ascii_case("array") && self.peek() == Some(b'(') {
                    self.position += 1;
                    self.parse_array(b')')
                } else {
                    self.position = start;
                    None
                }
            }
            _ => None,
        };
        self.skip_trivia();
        let at_end = matches!(self.peek(), Some(b',' | b']' | b')' | b';') | None) || self.starts_with(b"=>");
        if value.is_none() || !at_end {
            self.skip_expression();
            return None;
        }
        value
    }

    fn parse_concatenation(&mut self) -> Option<PhpValue> {
        let mut text = self.parse_string()?;
        loop {
            self.skip_trivia();
            if self.peek() != Some(b'.') {
                return Some(PhpValue::Text(text));
            }
            self.position += 1;
            self.skip_trivia();
            if !matches!(self.peek(), Some(b'\'' | b'"')) {
                return None;
            }
            text.push_str(&self.parse_string()?);
        }
    }

    fn parse_string(&mut self) -> Option<String> {
        let quote = self.peek()?;
        self.position += 1;
        let mut bytes = Vec::new();
        loop {
            let byte = self.peek()?;
            self.position += 1;
            if byte == quote {
                break;
            }
            if byte != b'\\' {
                bytes.push(byte);
                continue;
            }
            let escaped = self.peek()?;
            let unescaped = match (quote, escaped) {
                (_, b'\\') => Some(b'\\'),
                (b'\'', b'\'') => Some(b'\''),
                (b'"', b'"') => Some(b'"'),
                (b'"', b'$') => Some(b'$'),
                (b'"', b'n') => Some(b'\n'),
                (b'"', b't') => Some(b'\t'),
                (b'"', b'r') => Some(b'\r'),
                _ => None,
            };
            match unescaped {
                Some(unescaped) => {
                    bytes.push(unescaped);
                    self.position += 1;
                }
                None => bytes.push(b'\\'),
            }
        }
        String::from_utf8(bytes).ok()
    }

    /// Entries up to `close`; `key => value` pairs and list values (keyed by their index, like PHP).
    fn parse_array(&mut self, close: u8) -> Option<PhpValue> {
        let mut entries = Vec::new();
        let mut next_index = 0usize;
        loop {
            self.skip_trivia();
            match self.peek()? {
                byte if byte == close => {
                    self.position += 1;
                    return Some(PhpValue::Array(entries));
                }
                b',' => {
                    self.position += 1;
                    continue;
                }
                _ => {}
            }
            let start = self.position;
            let first = self.parse_value();
            self.skip_trivia();
            if self.starts_with(b"=>") {
                self.position += 2;
                let value = self.parse_value();
                if let (Some(PhpValue::Text(key)), Some(value)) = (first, value) {
                    if let Ok(index) = key.parse::<usize>() {
                        next_index = index + 1;
                    }
                    entries.push((key, value));
                }
            } else if let Some(value) = first {
                entries.push((next_index.to_string(), value));
                next_index += 1;
            }
            self.skip_trivia();
            if self.position == start {
                // Nothing consumed (a stray closing bracket of the other kind): avoid looping forever.
                self.position += 1;
            }
        }
    }

    /// Skips to the next `,` or closing bracket of the enclosing array, over nested brackets and strings.
    fn skip_expression(&mut self) {
        let mut depth = 0usize;
        while let Some(byte) = self.peek() {
            match byte {
                b'\'' | b'"' => {
                    if self.parse_string().is_none() {
                        return;
                    }
                    continue;
                }
                b'[' | b'(' | b'{' => depth += 1,
                b']' | b')' | b'}' => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => return,
                _ => {}
            }
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str, functions: &[&str]) -> Vec<String> {
        let functions = functions.iter().map(|function| function.to_string()).collect::<Vec<_>>();
        find_usages(text, &functions)
            .into_iter()
            .map(|usage| text[usage.key_range].to_string())
            .collect()
    }

    fn label(text: &str, message: &str) -> String {
        let usage = find_usages(text, &[]).into_iter().next().expect("usage");
        let interpolation = if usage.component {
            component_interpolation(text, usage.key_range.start)
        } else {
            call_interpolation(text, usage.key_range.end)
        };
        interpolate(message, &interpolation)
    }

    #[test]
    fn plural_form_and_placeholders_follow_the_call() {
        assert_eq!(label(r#"$t("main.night", nights)"#, "night | nights"), "nights");
        assert_eq!(label(r#"$t("main.night", 1)"#, "night | nights"), "night");
        assert_eq!(label(r#"t('main.x', 0)"#, "none | one | many"), "none");
        assert_eq!(label(r#"t('main.x', 5)"#, "none | one | many"), "many");
        assert_eq!(label(r#"t('main.x')"#, "night | nights"), "night | nights");
        assert_eq!(
            label(r#"t('main.hello', { name: user.name, count })"#, "Hi {name}, {count} new"),
            "Hi {user.name}, {count} new"
        );
        assert_eq!(label(r#"t('main.list', [first, second])"#, "{0} and {1}"), "{first} and {second}");
        assert_eq!(label(r#"t('main.n', items.length)"#, "{n} item | {n} items"), "{items.length} items");
        assert_eq!(
            label(r#"__('auth.throttle', ['seconds' => $seconds])"#, "Retry in :seconds seconds."),
            "Retry in {$seconds} seconds."
        );
        assert_eq!(
            label(r#"trans_choice('main.apples', $count)"#, "{0} None|[1,*] Some"),
            "Some"
        );
        assert_eq!(label(r#"t('main.x', { a: f(1, 2) })"#, "{a}"), "{f(1, 2)}");
    }

    #[test]
    fn component_slots_and_plural() {
        let text = r#"<i18n-t
            keypath="main.pagination.showing"
            :plural="props.total"
            scope="global"
        >
            <template #from>
                {{ from }}
            </template>
            <template #to>{{ to }}</template>
            <template #n>
                {{ props.total }}
            </template>
        </i18n-t>"#;
        assert_eq!(
            label(text, "No results | Showing {n} result | Showing {from} to {to} of {n} results"),
            "Showing {from} to {to} of {props.total} results"
        );
    }

    #[test]
    fn usages_match_vue_laravel_and_custom_functions() {
        let text = r#"t("main.a") this.$t('main.b') i18n.global.t(`main.c`) <i18n-t keypath="main.d"/>
            __('auth.failed') this.$lang("main.close") emit("x") at("no") t(`x.${y}`) format('no')"#;
        assert_eq!(
            keys(text, &["$lang"]),
            ["main.a", "main.b", "main.c", "main.d", "auth.failed", "main.close"]
        );
    }

    #[test]
    fn php_arrays_are_flattened() {
        let text = r#"<?php
            // return comment
            return [
                'close' => 'Close',
                "nested" => ['title' => 'It\'s ' . "a \"title\"", 'list' => ['a', 'b']],
                'constant' => SomeClass::VALUE,
                'call' => __('x'),
                'after' => 'kept', # comment
                'legacy' => array('x' => 'y'),
            ];"#;
        let mut messages = Messages::new();
        flatten_php(&parse_php_array(text).expect("array"), "", &mut messages);
        let expected = [
            ("after", "kept"),
            ("close", "Close"),
            ("legacy.x", "y"),
            ("nested.list.0", "a"),
            ("nested.list.1", "b"),
            ("nested.title", "It's a \"title\""),
        ];
        assert_eq!(
            messages.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn lookup_handles_namespaces_and_flat_keys() {
        let mut nested = Messages::new();
        flatten_json(
            &serde_json::json!({ "titles": { "calendar": "Calendar" }, "header.support.TITLE": "Support" }),
            "",
            &mut nested,
        );
        let catalog = Catalog {
            sources: vec![
                Source {
                    namespace: Some("main".to_string()),
                    messages: Arc::new(nested.clone()),
                },
                Source {
                    namespace: None,
                    messages: Arc::new(nested),
                },
            ],
        };
        assert!(matches!(catalog.lookup("main.titles.calendar"), Lookup::Value("Calendar")));
        assert!(matches!(catalog.lookup("titles.calendar"), Lookup::Value("Calendar")));
        assert!(matches!(catalog.lookup("header.support.TITLE"), Lookup::Value("Support")));
        assert!(matches!(catalog.lookup("main.titles"), Lookup::Namespace));
        assert!(matches!(catalog.lookup("main"), Lookup::Namespace));
        assert!(matches!(catalog.lookup("main.nope"), Lookup::Missing));
    }

    #[test]
    fn project_entries_apply_to_their_files_only() {
        let settings = I18nSettings {
            enabled: true,
            locale: "en".to_string(),
            paths: vec![PathBuf::from("/global")],
            functions: Vec::new(),
            max_length: 40,
            projects: vec![(
                PathBuf::from("/code/frontend"),
                vec![PathBuf::from("/code/backend/lang")],
                vec!["$lang".to_string()],
            )],
        };
        let inside = settings.for_file(Path::new("/code/frontend/src/App.vue"));
        assert_eq!(inside.paths, [PathBuf::from("/global"), PathBuf::from("/code/backend/lang")]);
        assert_eq!(inside.functions, ["$lang"]);
        let outside = settings.for_file(Path::new("/code/frontend-other/App.vue"));
        assert_eq!(outside.paths, [PathBuf::from("/global")]);
        assert!(outside.functions.is_empty());
    }

    #[test]
    fn truncate_keeps_max_characters() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abcd", 4), "abcd");
    }
}
