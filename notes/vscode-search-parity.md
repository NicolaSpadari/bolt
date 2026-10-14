# VS Code Search view → Bolt sidebar search (P2)

Target: VS Code's Search view (`src/vs/workbench/contrib/search`), docked in the left sidebar, replacing Zed's
multibuffer project search for everyday use. Status legend as in PROGRESS.

| # | Area | VS Code behavior | Bolt |
|---|---|---|---|
| 1 | Entry | Activity bar icon (magnifier); `cmd-shift-f` focuses the search input (selected text pre-filled) | ⏳ |
| 2 | Query input | Single line, search as you type (debounced); Enter = search now; up/down = history | ⏳ |
| 3 | Toggles | Match Case, Match Whole Word, Use Regular Expression (inside the input, right side) | ⏳ |
| 4 | Replace | Chevron left of the inputs toggles the replace row; Replace All button next to it; `cmd-shift-h` opens it | ⏳ |
| 5 | Include / exclude | "…" toggles "files to include" / "files to exclude" glob inputs (comma separated); "use exclude settings and ignore files" toggle | ⏳ |
| 6 | Summary | "N results in M files" message under the inputs; "No results found" | ⏳ |
| 7 | Results | Grouped by file: file row = icon, name, dimmed relative dir, match count badge; collapsible | ⏳ |
| 8 | Match row | Line preview with the match highlighted, trimmed around the match; replace preview (strikethrough old + new) | ⏳ |
| 9 | Click | Single click opens the file at the match (preview tab), double click pins; keyboard: Enter opens | ⏳ |
| 10 | Row actions (hover) | Match: Replace, Dismiss; File: Replace All in file, Dismiss | ⏳ |
| 11 | Title actions | Refresh, Clear Search Results, Collapse/Expand All, View as Tree/List, Open in Editor (→ Zed multibuffer) | ⏳ |
| 12 | Live update | Results refresh when files change | ⏳ |
| 13 | Limits | Stops at a result cap with a message | ⏳ |

Notes
- Zed's multibuffer project search stays reachable ("Open in Editor") but is not the default flow.
- Verify details on a real VS Code install when in doubt.
