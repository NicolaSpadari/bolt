# bolt_i18n — translated labels in place of i18n keys

Like x18n's VS Code in-place mode (`packages/vscode/src/model/inplace.ts`, `annotations.ts`), drawn the way
`bolt_iconify` draws icons (type-tagged folds).

## Behavior (x18n rules)
- The key text inside the quotes is folded and replaced by its value in the display locale (`t("Calendar")`), drawn in
  the inlay hint color. Hover on the value: tooltip with the raw key.
- Rows holding a cursor or a selection show the raw key (x18n `revealed`).
- Missing key: raw key in the warning color, never folded.
- Value longer than `max_length` (x18n `annotationMaxLength`, 40): not folded; after the closing quote a hint
  ` <value truncated to max_length - 1>…` (x18n `inlayHints` + `truncate`). Newlines in values → one space.
- Interpolation (Bolt addition, user request 2026-11-05): placeholders show the expression passed for them,
  `{name}` → `{user.name}`, from `t(key, { name: user.name })`, `t(key, [a, b])` (`{0}`), Laravel
  `__(key, ['name' => $x])` (`:name`) and `<i18n-t>` slots (`<template #from>{{ from }}</template>`). `{n}` / `{count}`
  default to the plural count.
- Plurals (`a | b | c`, Laravel `{0} …|[1,*] …`) only when a count is passed (`t(key, n)`, `:plural="…"`,
  `trans_choice`): a number literal picks the vue-i18n form (2 forms: 1 → first; 3 forms: 0 / 1 / many), any other
  expression shows the last (plural) form. `max_length` applies to the interpolated text. Hover shows the key only.
- Dynamic keys (template literals with `${`) are skipped. A key that names a subtree (namespace) is neither shown nor
  flagged as missing.

## Calls scanned (x18n presets)
- vue-i18n / nuxt-i18n: `$t t tc $tc te $te tm $tm i18n.t i18n.global.t $i18n.t i18n.tc i18n.te`; `keypath="…"`.
- Laravel: `__ trans trans_choice @lang Lang::get Lang::choice`.
- Plus setting `functions` (e.g. `$lang` for cb-frontend). A call may be preceded by `this.` / `obj.`.

## Label sources (intool model, pms-internal-dashboard `Services/Translation`)
- Directories: project candidates (`i18n/locales`, `locales`, `lang`, `src/locales`, `src/i18n`, `src/lang`,
  `resources/lang`, from the nearest dir with `package.json`/`composer.json`) + setting `paths` (absolute).
- Structure per directory, for the display locale `L`:
  - FolderPerLanguage: `dir/L/*.json|*.php` → keys `<file stem>.<path>` (Laravel `main.close` → `en/main.php`).
  - SingleFilePerLanguage: `dir/L.json`, `dir/translations_L.json` (intool/guesthub output).
- StorageFormat flat or nested: nested objects/arrays are flattened with `.`; flat keys are kept as they are.
- PHP: `return [ 'k' => 'v', 'n' => [ … ] ];` (also `array(…)`); non-literal values are skipped.
- Earlier sources win on duplicate keys (project dirs before `paths`). Cached per file, reloaded when mtime changes.

## Settings (`i18n`, user or project `.zed/settings.json`)
`enabled` (true), `locale` ("en"), `paths` ([] — absolute), `functions` ([]), `max_length` (40).
