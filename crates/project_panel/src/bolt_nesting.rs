//! BOLT: VS Code-style file nesting in the project panel, always on.
//!
//! Patterns come from antfu's vscode-file-nesting-config: `bolt_nesting_patterns.jsonc` is the README's settings
//! block verbatim (paste a newer one over it, or run `script/bolt-update-file-nesting`). Semantics follow VS Code's `explorer.fileNesting.patterns`: a key matches a
//! parent file name (one optional `*`, whose match is `$(capture)`), each comma-separated value is a glob for sibling
//! files nested under it. Only files nest, only within the same directory, and nesting is one level deep.

use std::sync::LazyLock;

use collections::{HashMap, HashSet};
use project::{GitEntry, ProjectEntryId};
use serde::Deserialize;

#[derive(Deserialize)]
struct PatternsFile {
    #[serde(rename = "explorer.fileNesting.patterns")]
    patterns: HashMap<String, String>,
}

/// Turns the README snippet (object members with `//` comments and trailing commas) into a JSON object.
fn snippet_to_json(snippet: &str) -> String {
    let body: Vec<&str> = snippet
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect();
    let mut json = String::from("{");
    json.push_str(&body.join("\n"));
    json.push('}');
    // Drop trailing commas before a closing brace (outside of strings).
    let mut cleaned = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_comma = false;
    for character in json.chars() {
        if in_string {
            cleaned.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if pending_comma && !character.is_whitespace() {
            if character != '}' {
                cleaned.push(',');
            }
            pending_comma = false;
        }
        match character {
            ',' => pending_comma = true,
            '"' => {
                in_string = true;
                cleaned.push(character);
            }
            _ => cleaned.push(character),
        }
    }
    cleaned
}

struct NestingRule {
    parent: String,
    has_wildcard: bool,
    children: Vec<String>,
}

static RULES: LazyLock<Vec<NestingRule>> = LazyLock::new(|| {
    let file: PatternsFile = serde_json::from_str(&snippet_to_json(include_str!(
        "bolt_nesting_patterns.jsonc"
    )))
    .expect("bundled file nesting patterns are valid");
    let mut rules: Vec<NestingRule> = file
        .patterns
        .into_iter()
        .map(|(parent, children)| NestingRule {
            has_wildcard: parent.contains('*'),
            parent: parent.to_lowercase(),
            children: children
                .split(',')
                .map(|child| child.trim().to_lowercase())
                .filter(|child| !child.is_empty())
                .collect(),
        })
        .collect();
    // Exact parent names win over wildcard ones (`package.json` before `*.json`), then longer keys first.
    rules.sort_by(|a, b| {
        a.has_wildcard
            .cmp(&b.has_wildcard)
            .then(b.parent.len().cmp(&a.parent.len()))
            .then(a.parent.cmp(&b.parent))
    });
    rules
});

/// Nesting computed for the visible entries of the project panel.
#[derive(Default, Clone)]
pub(crate) struct NestingState {
    /// Files that have nested children.
    parents: HashSet<ProjectEntryId>,
    /// Files shown nested under a parent (rendered one level deeper).
    nested: HashSet<ProjectEntryId>,
}

impl NestingState {
    pub(crate) fn is_parent(&self, entry_id: ProjectEntryId) -> bool {
        self.parents.contains(&entry_id)
    }

    pub(crate) fn is_nested(&self, entry_id: ProjectEntryId) -> bool {
        self.nested.contains(&entry_id)
    }
}

/// Moves nested files right after their parent, dropping them when the parent is collapsed.
/// `entries` must already be sorted in tree order; `expanded_ids` is the sorted expanded-entries list.
pub(crate) fn apply(
    entries: &mut Vec<GitEntry>,
    expanded_ids: &[ProjectEntryId],
    excluded_id: ProjectEntryId,
    state: &mut NestingState,
) {
    let mut files_by_dir: HashMap<String, Vec<usize>> = HashMap::default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.is_file()
            && entry.id != excluded_id
            && let Some(parent_dir) = entry.path.parent()
        {
            files_by_dir
                .entry(parent_dir.as_unix_str().to_string())
                .or_default()
                .push(index);
        }
    }

    let mut children_of: HashMap<usize, Vec<usize>> = HashMap::default();
    let mut nested_indices: HashSet<usize> = HashSet::default();
    for indices in files_by_dir.values() {
        if indices.len() < 2 {
            continue;
        }
        let names: Vec<String> = indices
            .iter()
            .map(|&index| {
                entries[index]
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_lowercase()
            })
            .collect();
        for (position, parent_position) in nest_names(&names).into_iter().enumerate() {
            if let Some(parent_position) = parent_position {
                children_of
                    .entry(indices[parent_position])
                    .or_default()
                    .push(indices[position]);
                nested_indices.insert(indices[position]);
            }
        }
    }
    if children_of.is_empty() {
        return;
    }

    let mut old_entries: Vec<Option<GitEntry>> =
        std::mem::take(entries).into_iter().map(Some).collect();
    for index in 0..old_entries.len() {
        if nested_indices.contains(&index) {
            continue;
        }
        let Some(entry) = old_entries[index].take() else {
            continue;
        };
        let entry_id = entry.id;
        entries.push(entry);
        if let Some(children) = children_of.get(&index) {
            state.parents.insert(entry_id);
            let is_expanded = expanded_ids.binary_search(&entry_id).is_ok();
            for &child_index in children {
                if let Some(child) = old_entries[child_index].take()
                    && is_expanded
                {
                    state.nested.insert(child.id);
                    entries.push(child);
                }
            }
        }
    }
}

/// For each lowercase file name of one directory, returns the position of the file it nests under.
fn nest_names(names: &[String]) -> Vec<Option<usize>> {
    let positions_by_name: HashMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(position, name)| (name.as_str(), position))
        .collect();
    let mut parent_of: Vec<Option<usize>> = vec![None; names.len()];

    for rule in RULES.iter() {
        for (parent_position, parent_name) in names.iter().enumerate() {
            let Some(capture) = match_capture(&rule.parent, parent_name) else {
                continue;
            };
            for child_pattern in &rule.children {
                let child_pattern = child_pattern.replace("$(capture)", capture);
                if child_pattern.contains('*') {
                    for (child_position, child_name) in names.iter().enumerate() {
                        if child_position != parent_position
                            && parent_of[child_position].is_none()
                            && glob_match(&child_pattern, child_name)
                        {
                            parent_of[child_position] = Some(parent_position);
                        }
                    }
                } else if let Some(&child_position) = positions_by_name.get(child_pattern.as_str())
                    && child_position != parent_position
                    && parent_of[child_position].is_none()
                {
                    parent_of[child_position] = Some(parent_position);
                }
            }
        }
    }

    // One level only: a file that is itself nested hands its children to its own top-level parent.
    let mut resolved = parent_of.clone();
    for position in 0..names.len() {
        let mut current = parent_of[position];
        let mut steps = 0;
        while let Some(parent) = current
            && let Some(grandparent) = parent_of[parent]
        {
            current = Some(grandparent);
            steps += 1;
            if steps > names.len() {
                current = None;
                break;
            }
        }
        resolved[position] = current.filter(|&parent| parent != position);
    }
    resolved
}

/// Matches a parent pattern (at most one `*`) and returns what the `*` matched.
fn match_capture<'a>(pattern: &str, name: &'a str) -> Option<&'a str> {
    match pattern.split_once('*') {
        None => (pattern == name).then_some(""),
        Some((prefix, suffix)) => {
            if name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
            {
                Some(&name[prefix.len()..name.len() - suffix.len()])
            } else {
                None
            }
        }
    }
}

/// Glob match where `*` matches any run of characters.
fn glob_match(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(position) => rest = &rest[position + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nest(names: &[&str]) -> Vec<(String, Option<String>)> {
        let names: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
        nest_names(&names)
            .into_iter()
            .enumerate()
            .map(|(position, parent)| {
                (
                    names[position].clone(),
                    parent.map(|parent| names[parent].clone()),
                )
            })
            .collect()
    }

    #[test]
    fn test_bundled_patterns_parse() {
        assert!(RULES.len() > 50);
        assert!(RULES.iter().any(|rule| rule.parent == "package.json"));
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.env", "prod.env"));
        assert!(glob_match(".env.*", ".env.local"));
        assert!(!glob_match(".env.*", ".env"));
        assert!(glob_match("foo.*.ts", "foo.spec.ts"));
        assert!(!glob_match("foo.*.ts", "foo.ts"));
        assert!(glob_match("karma*", "karma.conf.js"));
        assert!(glob_match("exact", "exact"));
    }

    #[test]
    fn test_nesting() {
        let result = nest(&[
            "package.json",
            "pnpm-lock.yaml",
            "foo.ts",
            "foo.js",
            "foo.spec.ts",
            "README.md",
            "bar.ts",
        ]);
        let parent_of = |name: &str| {
            result
                .iter()
                .find(|(file, _)| file == name)
                .and_then(|(_, parent)| parent.clone())
        };
        assert_eq!(parent_of("pnpm-lock.yaml").as_deref(), Some("package.json"));
        assert_eq!(parent_of("foo.js").as_deref(), Some("foo.ts"));
        assert_eq!(parent_of("foo.spec.ts").as_deref(), Some("foo.ts"));
        assert_eq!(parent_of("package.json"), None);
        assert_eq!(parent_of("bar.ts"), None);
        assert_eq!(parent_of("foo.ts"), None);
    }
}

impl crate::ProjectPanel {
    /// Expands or collapses the nested files of a nesting parent.
    pub(crate) fn bolt_set_nested_expanded(
        &mut self,
        entry_id: ProjectEntryId,
        expanded: bool,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(worktree_id) = self.project.read(cx).worktree_id_for_entry(entry_id, cx) else {
            return;
        };
        let Some(expanded_ids) = self.state.expanded_dir_ids.get_mut(&worktree_id) else {
            return;
        };
        match (expanded_ids.binary_search(&entry_id), expanded) {
            (Ok(index), false) => {
                expanded_ids.remove(index);
            }
            (Err(index), true) => expanded_ids.insert(index, entry_id),
            _ => return,
        }
        self.update_visible_entries(Some((worktree_id, entry_id)), false, false, window, cx);
        cx.notify();
    }

    pub(crate) fn bolt_toggle_nested(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let is_expanded = self
            .project
            .read(cx)
            .worktree_id_for_entry(entry_id, cx)
            .and_then(|worktree_id| self.state.expanded_dir_ids.get(&worktree_id))
            .is_some_and(|expanded_ids| expanded_ids.binary_search(&entry_id).is_ok());
        self.bolt_set_nested_expanded(entry_id, !is_expanded, window, cx);
    }
}
