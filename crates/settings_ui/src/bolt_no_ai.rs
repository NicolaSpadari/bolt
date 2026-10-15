//! BOLT: stand-ins for the AI settings pages when the `ai` feature is off: their crates (agent, edit prediction,
//! copilot, codestral) are left out of the build. The AI pages are hidden in Bolt (`BOLT_HIDDEN_PAGES`), so these
//! only keep `page_data.rs` compiling unchanged.

use gpui::{AnyElement, App, Context, ScrollHandle, Window};
use ui::prelude::*;

use crate::{SettingField, SettingsFieldMetadata, SettingsUiFile, SettingsWindow};

pub(crate) fn render_edit_prediction_setup_page(
    _: &SettingsWindow,
    _: &ScrollHandle,
    _: &mut Window,
    _: &mut Context<SettingsWindow>,
) -> AnyElement {
    gpui::Empty.into_any_element()
}

pub(crate) fn render_tool_permissions_setup_page(
    _: &SettingsWindow,
    _: &ScrollHandle,
    _: &mut Window,
    _: &mut Context<SettingsWindow>,
) -> AnyElement {
    gpui::Empty.into_any_element()
}

macro_rules! empty_tool_config_page_fn {
    ($($fn_name:ident),* $(,)?) => {
        $(
            pub fn $fn_name(
                _: &SettingsWindow,
                _: &ScrollHandle,
                _: &mut Window,
                _: &mut Context<SettingsWindow>,
            ) -> AnyElement {
                gpui::Empty.into_any_element()
            }
        )*
    };
}

empty_tool_config_page_fn!(
    render_terminal_tool_config,
    render_edit_file_tool_config,
    render_write_file_tool_config,
    render_delete_path_tool_config,
    render_copy_path_tool_config,
    render_move_path_tool_config,
    render_create_directory_tool_config,
    render_fetch_tool_config,
    render_web_search_tool_config,
    render_skill_tool_config,
);

pub fn render_ollama_model_picker(
    _: SettingField<settings::OllamaModelName>,
    _: SettingsUiFile,
    _: Option<&SettingsFieldMetadata>,
    _: &'static str,
    _: &'static str,
    _: &mut Window,
    _: &mut App,
) -> AnyElement {
    gpui::Empty.into_any_element()
}
