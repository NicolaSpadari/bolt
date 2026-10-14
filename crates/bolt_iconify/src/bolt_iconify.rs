//! BOLT: inline Iconify icon previews, like antfu's Iconify IntelliSense for VS Code.
//!
//! Icon names such as `ph:bed` or `i-lucide-check` are replaced in the editor by the icon itself, drawn in the text
//! color. The line holding a cursor shows the original text so it stays editable. Icons are read from the project's
//! `node_modules` (`@iconify-json/<prefix>` or `@iconify/json`), so only installed collections match and nothing is
//! fetched from the network. Setting: `iconify` (default true).

use std::{
    any::TypeId,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use collections::HashMap;
use editor::{
    Anchor, Editor, EditorEvent, FoldPlaceholder, HighlightKey, Inlay, display_map::Crease,
};
use gpui::{
    App, AppContext as _, Context, Entity, HighlightStyle, Hsla, Image, ImageFormat, Subscription, Task,
    WeakEntity, Window, img,
};
use multi_buffer::ToPoint as _;
use regex::Regex;
use settings::{RegisterSetting, Settings, SettingsStore};
use theme::ActiveTheme as _;
use theme_settings::ThemeSettings;
use ui::prelude::*;

/// Files larger than this are not scanned.
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;
const SCAN_DEBOUNCE: Duration = Duration::from_millis(150);
const DEFAULT_ICON_SIZE: f64 = 16.;
/// Rasterization size of the icons: drawing a 256px SVG at text size thins its strokes.
const RASTER_SIZE: u32 = 32;
/// Opacity reduction of the icon name shown next to its icon on the cursor row.
const REVEALED_TEXT_FADE: f32 = 0.4;
/// Inlay ids share the color inlay id space; start far above the LSP document color counter.
static NEXT_INLAY_ID: AtomicUsize = AtomicUsize::new(1 << 40);

#[derive(Debug, Clone, Copy, RegisterSetting)]
pub struct IconifySettings {
    pub enabled: bool,
}

impl Settings for IconifySettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self {
            enabled: content.editor.iconify.unwrap_or(true),
        }
    }
}

/// Fold type tag, so only Bolt's icon folds are ever removed.
enum IconifyFold {}

pub fn init(cx: &mut App) {
    cx.observe_new(|editor: &mut Editor, window: Option<&mut Window>, cx| {
        let Some(window) = window else {
            return;
        };
        if !editor.mode().is_full() || editor.buffer().read(cx).as_singleton().is_none() {
            return;
        }
        let editor_entity = cx.entity();
        let iconify = cx.new(|cx| Iconify::new(editor_entity, window, cx));
        editor.register_addon(IconifyAddon { _iconify: iconify });
    })
    .detach();
}

struct IconifyAddon {
    _iconify: Entity<Iconify>,
}

impl editor::Addon for IconifyAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct IconMatch {
    range: Range<Anchor>,
    image: Arc<Image>,
}

struct Iconify {
    editor: WeakEntity<Editor>,
    icons: Vec<IconMatch>,
    /// Rows holding a cursor the last time folds were applied; their icons show as text.
    revealed_rows: Vec<u32>,
    /// Icon inlays shown before the icon names on the cursor rows.
    inlay_ids: Vec<usize>,
    scan: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Iconify {
    fn new(editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor_subscription =
            cx.subscribe_in(&editor, window, |this, _, event: &EditorEvent, window, cx| {
                match event {
                    EditorEvent::BufferEdited | EditorEvent::Reparsed(_) => {
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
            icons: Vec::new(),
            revealed_rows: Vec::new(),
            inlay_ids: Vec::new(),
            scan: Task::ready(()),
            _subscriptions: vec![editor_subscription, settings_subscription],
        }
    }

    fn schedule_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        if !IconifySettings::get_global(cx).enabled {
            self.icons.clear();
            self.apply_folds(true, window, cx);
            return;
        }
        let Some(buffer) = editor.read(cx).buffer().read(cx).as_singleton() else {
            return;
        };
        let buffer = buffer.read(cx);
        let Some(node_modules_dir) = buffer
            .file()
            .and_then(|file| file.as_local())
            .and_then(|file| file.abs_path(cx).parent().and_then(find_node_modules))
        else {
            return;
        };
        let text = buffer.text();
        if text.len() > MAX_SCAN_BYTES {
            return;
        }
        let color = cx.theme().colors().editor_foreground;
        let background = cx.background_executor().clone();
        self.scan = cx.spawn_in(window, async move |this, cx| {
            background.timer(SCAN_DEBOUNCE).await;
            let found = background
                .spawn(async move { find_icons(&text, &node_modules_dir, color) })
                .await;
            this.update_in(cx, |this, window, cx| {
                let Some(editor) = this.editor.upgrade() else {
                    return;
                };
                let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
                this.icons = found
                    .into_iter()
                    .map(|(range, image)| IconMatch {
                        range: snapshot.anchor_after(editor::MultiBufferOffset(range.start))
                            ..snapshot.anchor_before(editor::MultiBufferOffset(range.end)),
                        image,
                    })
                    .collect();
                this.apply_folds(true, window, cx);
            })
            .ok();
        });
    }

    /// Folds every icon range except those on a row holding a cursor, which show the icon before the dimmed name
    /// instead. Without `force`, nothing happens unless the set of cursor rows changed.
    fn apply_folds(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let icons = &self.icons;
        let revealed_rows = &mut self.revealed_rows;
        let inlay_ids = &mut self.inlay_ids;
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
            editor.remove_folds_with_type(
                &[full_range],
                TypeId::of::<IconifyFold>(),
                false,
                cx,
            );
            let (revealed, folded): (Vec<&IconMatch>, Vec<&IconMatch>) =
                icons.iter().partition(|icon| {
                    let row = icon.range.start.to_point(&snapshot).row;
                    revealed_rows.binary_search(&row).is_ok()
                });
            let creases = folded
                .iter()
                .map(|icon| Crease::simple(icon.range.clone(), icon_placeholder(icon.image.clone())))
                .collect::<Vec<_>>();
            editor.fold_creases(creases, false, window, cx);

            let old_inlays = inlay_ids
                .drain(..)
                .map(project::InlayId::Color)
                .collect::<Vec<_>>();
            let new_inlays = revealed
                .iter()
                .map(|icon| {
                    let id = NEXT_INLAY_ID.fetch_add(1, Ordering::Relaxed);
                    inlay_ids.push(id);
                    Inlay::image(id, icon.range.start, icon.image.clone())
                })
                .collect::<Vec<_>>();
            editor.splice_inlays(&old_inlays, new_inlays, cx);
            editor.highlight_text(
                HighlightKey::BoltIconify,
                revealed.iter().map(|icon| icon.range.clone()).collect(),
                HighlightStyle {
                    fade_out: Some(REVEALED_TEXT_FADE),
                    ..HighlightStyle::default()
                },
                cx,
            );
        });
    }
}

fn icon_placeholder(image: Arc<Image>) -> FoldPlaceholder {
    FoldPlaceholder {
        render: Arc::new(move |_, _, cx: &mut App| {
            let size = ThemeSettings::get_global(cx).buffer_font_size(cx);
            div()
                .h_full()
                .flex()
                .items_center()
                .child(img(image.clone()).size(size))
                .into_any_element()
        }),
        constrain_width: false,
        merge_adjacent: false,
        type_tag: Some(TypeId::of::<IconifyFold>()),
        collapsed_text: None,
    }
}

/// The nearest `node_modules` holding Iconify collections, walking up from `start`.
fn find_node_modules(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|dir| {
        let node_modules = dir.join("node_modules");
        (node_modules.join("@iconify-json").is_dir() || node_modules.join("@iconify/json/json").is_dir())
            .then_some(node_modules)
    })
}

struct Collection {
    icons: serde_json::Map<String, serde_json::Value>,
    aliases: serde_json::Map<String, serde_json::Value>,
    width: f64,
    height: f64,
}

impl Collection {
    fn load(node_modules: &Path, prefix: &str) -> Option<Self> {
        let paths = [
            node_modules.join("@iconify-json").join(prefix).join("icons.json"),
            node_modules.join("@iconify/json/json").join(format!("{prefix}.json")),
        ];
        let text = paths.iter().find_map(|path| std::fs::read_to_string(path).ok())?;
        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(json) => json,
            Err(error) => {
                log::error!("iconify: invalid collection {prefix}: {error}");
                return None;
            }
        };
        let object = |key: &str| {
            json.get(key)
                .and_then(|value| value.as_object())
                .cloned()
                .unwrap_or_default()
        };
        Some(Self {
            icons: object("icons"),
            aliases: object("aliases"),
            width: json.get("width").and_then(|value| value.as_f64()).unwrap_or(DEFAULT_ICON_SIZE),
            height: json.get("height").and_then(|value| value.as_f64()).unwrap_or(DEFAULT_ICON_SIZE),
        })
    }

    /// SVG markup for `name` (following aliases), with `currentColor` replaced by `color`.
    fn svg(&self, name: &str, color: &str) -> Option<String> {
        let mut name = name;
        let mut alias_depth = 0;
        let icon = loop {
            if let Some(icon) = self.icons.get(name) {
                break icon;
            }
            let parent = self.aliases.get(name)?.get("parent")?.as_str()?;
            alias_depth += 1;
            if alias_depth > 8 {
                return None;
            }
            name = parent;
        };
        let body = icon.get("body")?.as_str()?;
        let number = |key: &str, default: f64| icon.get(key).and_then(|value| value.as_f64()).unwrap_or(default);
        let width = number("width", self.width);
        let height = number("height", self.height);
        let left = number("left", 0.);
        let top = number("top", 0.);
        let (raster_width, raster_height) = if width >= height {
            (RASTER_SIZE as f64, RASTER_SIZE as f64 * height / width)
        } else {
            (RASTER_SIZE as f64 * width / height, RASTER_SIZE as f64)
        };
        Some(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{raster_width}" height="{raster_height}" viewBox="{left} {top} {width} {height}">{}</svg>"#,
            body.replace("currentColor", color)
        ))
    }
}

/// Loaded collections per `node_modules` directory and prefix (`None` = not installed).
static COLLECTIONS: LazyLock<Mutex<HashMap<(PathBuf, String), Option<Arc<Collection>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::default()));

fn collection(node_modules: &Path, prefix: &str) -> Option<Arc<Collection>> {
    let key = (node_modules.to_path_buf(), prefix.to_string());
    if let Ok(collections) = COLLECTIONS.lock()
        && let Some(cached) = collections.get(&key)
    {
        return cached.clone();
    }
    let loaded = Collection::load(node_modules, prefix).map(Arc::new);
    if let Ok(mut collections) = COLLECTIONS.lock() {
        collections.insert(key, loaded.clone());
    }
    loaded
}

static COLON_FORM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:i-)?([a-z][a-z0-9]*(?:-[a-z0-9]+)*):([a-z0-9]+(?:-[a-z0-9]+)*)")
        .expect("Failed to create COLON_FORM regex")
});
static DASH_FORM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"i-([a-z0-9]+(?:-[a-z0-9]+)+)").expect("Failed to create DASH_FORM regex")
});

fn is_name_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':' | '/')
}

/// Icon ranges (byte offsets) and their rendered image.
fn find_icons(text: &str, node_modules: &Path, color: Hsla) -> Vec<(Range<usize>, Arc<Image>)> {
    let color = color_hex(color);
    let bounded = |range: &Range<usize>| {
        let before = text[..range.start].chars().next_back();
        let after = text[range.end..].chars().next();
        !before.is_some_and(is_name_char) && !after.is_some_and(is_name_char)
    };
    let mut images: HashMap<String, Option<Arc<Image>>> = HashMap::default();
    let mut image_for = |prefix: &str, name: &str| -> Option<Arc<Image>> {
        let key = format!("{prefix}:{name}");
        images
            .entry(key)
            .or_insert_with(|| {
                let svg = collection(node_modules, prefix)?.svg(name, &color)?;
                Some(Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())))
            })
            .clone()
    };

    let mut found = Vec::new();
    for captures in COLON_FORM.captures_iter(text) {
        let (Some(whole), Some(prefix), Some(name)) = (captures.get(0), captures.get(1), captures.get(2)) else {
            continue;
        };
        let range = whole.range();
        if !bounded(&range) {
            continue;
        }
        if let Some(image) = image_for(prefix.as_str(), name.as_str()) {
            found.push((range, image));
        }
    }
    for captures in DASH_FORM.captures_iter(text) {
        let (Some(whole), Some(rest)) = (captures.get(0), captures.get(1)) else {
            continue;
        };
        let range = whole.range();
        if !bounded(&range) || found.iter().any(|(existing, _)| existing.start < range.end && range.start < existing.end) {
            continue;
        }
        // Collection prefixes can contain dashes (`material-symbols`): prefer the longest installed prefix.
        let parts: Vec<&str> = rest.as_str().split('-').collect();
        let image = (1..parts.len()).rev().find_map(|split| {
            image_for(&parts[..split].join("-"), &parts[split..].join("-"))
        });
        if let Some(image) = image {
            found.push((range, image));
        }
    }
    found.sort_by_key(|(range, _)| range.start);
    found
}

fn color_hex(color: Hsla) -> String {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", channel(rgba.r), channel(rgba.g), channel(rgba.b))
}
