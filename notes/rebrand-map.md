# Rebrand map — Zed branding still visible in Bolt (mapped 2026-10-15, not done yet)

Approach: replace hard-coded "Zed" in user-visible text with `ReleaseChannel::global(cx).display_name()` (already
"Bolt") or a literal "Bolt"; mark every edit `BOLT:` and list it in PROGRESS "Upstream touch points". Only items in the
default build (features `ai`, `collab`, `repl`, `debugger` off) matter.

## 1. Empty window / welcome page (highest visibility)
`crates/workspace/src/pane.rs:4634` puts `welcome::WelcomePage` in empty panes (also "Show Welcome", after onboarding).
| Where | What the user sees | Bolt |
|---|---|---|
| `crates/workspace/src/welcome.rs:480` | Zed logo (`VectorName::ZedLogo` → `assets/images/zed_logo.svg`) | Bolt logo (new vector or replaced SVG, from `crates/zed/resources/bolt-icon.svg`) |
| `welcome.rs:451`, `:453` | "Welcome back to Zed" / "Welcome to Zed" | display name |
| `welcome.rs:482` | "The editor for what's next" (Zed slogan) | Bolt tagline or nothing |
| `crates/onboarding/src/onboarding.rs:350`, `:354`, `:357` | logo + "Welcome to Zed" + slogan | same as above |

## 2. Other visible strings (default build)
| Where | Text |
|---|---|
| `crates/zed/src/zed.rs:1760` | "About Zed" window title |
| `crates/settings_ui/src/settings_ui.rs:898` | "Zed — Settings" window title |
| `crates/zed/src/zed/app_menus.rs:303-316` | Help: "Zed Repository", "Zed Twitter", zed.dev docs / jobs links |
| `crates/zed/src/zed/move_to_applications.rs:76,78,106,181,199,201` | "Move Zed to Applications?", "Installing Zed…" |
| `crates/workspace/src/notifications.rs:1213` | "A new version of Zed is available…" |
| `crates/workspace/src/workspace.rs:10754` | "…unsupported version of Zed." |
| `crates/workspace/src/pane_group.rs:424,428` | collaboration labels |
| `crates/title_bar/src/title_bar.rs:1177,1183,1327`, `title_bar/src/collab.rs:90` | restart/update Zed, screen sharing |
| `crates/ui/src/components/collab/update_button.rs:103,109,115` | "Checking for / Downloading / Installing Zed Update…" |
| `crates/auto_update/src/auto_update.rs:317` | "Zed was installed via a package manager." |
| `crates/install_cli/src/install_cli_binary.rs:115,144` | "Couldn't install the Zed CLI" |
| `crates/onboarding/src/basics_page.rs:442,447` | trust-projects tooltip/description |
| `crates/settings/src/base_keymap_setting.rs:61,77,89` | "Zed (Default)" label only (keep `BaseKeymap::Zed`) |
| `crates/settings_ui/src/page_data.rs` (~20 descriptions: 150, 336, 386, 407, 464, 728, 3949, 3972, 5206, 7040, 7077, 8017, 8630, 9017, 9306, 9886, 9906, 10454, 11213, 11221), `pages/sandbox_settings.rs:66` | setting descriptions |
| `crates/extensions_ui/src/extensions_ui.rs:1094-1196` (18 "X support is built-in to Zed!"), `components/extension_card.rs:362` | extensions UI |
| `crates/language_tools/src/key_context_view.rs:196,204` | "…key bindings in Zed" |
| `crates/zed/src/zed.rs:121` `DOCS_URL`, `recent_projects/src/remote_servers.rs:2540` | zed.dev docs links |

## 3. macOS bundle
- `crates/zed/resources/info/Permissions.plist` (~12 "An application in Zed wants to use …" permission prompts).
- `crates/zed/resources/info/DocumentTypes.plist:22` "Zed Text Document" (Finder kind); `Document.icns` (file icon).
- `script/bundle-mac`: `-volname Zed`, `Zed-${arch}.dmg`, `IDENTITY="Zed Industries, Inc."`.
- `crates/zed/Cargo.toml`: nightly/preview bundle sections still "Zed Nightly/Preview", `dev.zed.*`; `authors`.

## 4. Do NOT rename (protocol / identifiers)
`zed_actions` / `zed::` action namespaces (incl. `OpenZedRepo`, `EmailZed`), `.zed/` project folder, `ZED_*` env vars,
settings keys, `BaseKeymap::Zed`, `Plan::Zed*`, "Zed Keybind Context" language, extension API names,
`api.zed.dev` / `cloud.zed.dev`, `Zed/{version}` user agent, Windows `app_identifier`s.
To check separately: `open_listener.rs:179-184` parses `zed://` links while the bundle registers the `bolt` scheme.

## Ignore
Comments (~580 lines), tests, logs, Linux/Windows-only text, collab server, docs preprocessor, AI-only crates (off by
default).
