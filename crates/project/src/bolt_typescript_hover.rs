//! BOLT: TypeScript hovers always expanded one level, like VS Code's expandable hover after one "+"
//! (`interface User` → the whole `interface User { … }` declaration).
//!
//! VS Code asks tsserver (TS ≥ 5.9) for `quickinfo` with a `verbosityLevel`; vtsls ships the same hover provider but
//! never passes a level through LSP, so the expanded signature is requested with vtsls' `typescript.tsserverRequest`
//! command and replaces the signature code block of the regular hover.

use anyhow::{Context as _, Result};
use gpui::{AsyncApp, Entity};
use language::{Buffer, PointUtf16};
use lsp::{LanguageServerId, LanguageServerName};
use serde_json::{Value, json};
use settings::Settings as _;

use crate::{Hover, HoverBlockKind, lsp_store::LspStore, project_settings::ProjectSettings};

const VTSLS: LanguageServerName = LanguageServerName::new_static("vtsls");
/// The first level VS Code reaches with "+": the hovered declaration expanded, nested types still by name.
const VERBOSITY_LEVEL: u32 = 1;

pub(crate) async fn expand(
    hover: &mut Hover,
    lsp_store: &Entity<LspStore>,
    server_id: LanguageServerId,
    buffer: &Entity<Buffer>,
    position: PointUtf16,
    cx: &AsyncApp,
) -> Result<()> {
    let Some((server, path, request_timeout)) = cx.update(|cx| {
        let settings = &ProjectSettings::get_global(cx).global_lsp_settings;
        if !settings.expand_typescript_hovers {
            return None;
        }
        let server = lsp_store
            .read(cx)
            .language_server_for_id(server_id)
            .filter(|server| server.name() == VTSLS)?;
        let path = buffer
            .read(cx)
            .file()
            .and_then(|file| file.as_local())
            .map(|file| file.abs_path(cx))?;
        Some((server, path, settings.get_request_timeout()))
    }) else {
        return Ok(());
    };

    // tsserver positions are 1-based, columns in UTF-16 code units like `PointUtf16`.
    let response = server
        .request::<lsp::request::ExecuteCommand>(
            lsp::ExecuteCommandParams {
                command: "typescript.tsserverRequest".to_string(),
                arguments: vec![
                    json!("quickinfo"),
                    json!({
                        "file": path,
                        "line": position.row + 1,
                        "offset": position.column + 1,
                        "verbosityLevel": VERBOSITY_LEVEL,
                    }),
                ],
                ..lsp::ExecuteCommandParams::default()
            },
            request_timeout,
        )
        .await
        .into_response()
        .context("typescript.tsserverRequest quickinfo")?;
    let Some(display_string) = response
        .as_ref()
        .and_then(|response| response.pointer("/body/displayString"))
        .and_then(Value::as_str)
        .filter(|display_string| !display_string.is_empty())
    else {
        return Ok(());
    };

    for block in &mut hover.contents {
        match &block.kind {
            HoverBlockKind::Code { .. } => {
                block.text = display_string.to_string();
                return Ok(());
            }
            HoverBlockKind::Markdown => {
                if let Some(text) = replace_first_code_block(&block.text, display_string) {
                    block.text = text;
                    return Ok(());
                }
            }
            HoverBlockKind::PlainText => {}
        }
    }
    Ok(())
}

/// Replaces the body of the first fenced code block (vtsls puts the signature there, before the documentation).
fn replace_first_code_block(markdown: &str, body: &str) -> Option<String> {
    let fence_start = markdown.find("```")?;
    let body_start = fence_start + markdown[fence_start..].find('\n')? + 1;
    let body_end = body_start + markdown[body_start..].find("```")?;
    let mut result = String::with_capacity(markdown.len() + body.len());
    result.push_str(&markdown[..body_start]);
    result.push_str(body);
    if !body.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&markdown[body_end..]);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::replace_first_code_block;

    #[test]
    fn test_replace_first_code_block() {
        let markdown = "\n```typescript\ninterface User\n```\nUser docs\n\n```ts\nexample\n```";
        assert_eq!(
            replace_first_code_block(markdown, "interface User {\n    id: number;\n}").as_deref(),
            Some(
                "\n```typescript\ninterface User {\n    id: number;\n}\n```\nUser docs\n\n```ts\nexample\n```"
            )
        );
        assert_eq!(replace_first_code_block("no code", "x"), None);
    }
}
