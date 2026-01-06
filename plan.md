# Design Plan: Compact Log Frontend

## Goal
Revamp `static/log.html` to be a compact, data-dense, linear log viewer that aligns with the visual style of `static/index.html`. Remove chat-like aesthetics (bubbles, alignment shifts) in favor of a technical log format.

## visual Style
- **Font:** Monospace (Consolas, Monaco, "Courier New", monospace) for everything.
- **Background:** White or very light gray (`#f5f5f5` to match index).
- **Layout:** Single column, left-aligned. No left/right separation for User/Model.
- **Spacing:** Tight. Reduced padding/margins.
- **Colors:** Minimal use of color. Use text color or small indicators for roles, rather than large blocks/backgrounds.

## Structure

### Header
- Simple `h1` with back link, similar to `index.html`.
- Meta info (status, total tokens) displayed inline or immediately below.

### Log Entries
- A flat list of entries.
- Each entry has:
    - **Marker/Role:** A small fixed-width column or gutter marker indicating the source (User, Model, Tool).
    - **Content:** The text content.
- **User:** displayed as plain text, distinct color (e.g., dark blue).
- **Model:** displayed as plain text, distinct color (e.g., black or dark purple).
- **Tool Call:** displayed like code: `function_name(arg1=..., arg2=...)`.
- **Tool Result:** displayed as `-> result`.

### JSON & Details
- **Compact JSON:** 
    - If the JSON object is small or can be flattened to a single line without being excessive (> 120 chars?), show it inline.
    - Remove "Show JSON" link for these cases.
- **LLM Metadata:**
    - Extract fields like `usage`, `finish_reason`, `safety_ratings` from the raw log JSON if present.
    - Display them in a compact footer line for the model entry, e.g., `[Usage: 100/20 | Stop: Stop Sequence]`.

## Implementation Details

### CSS
- Remove `.log-entry.role-model { text-align: right }`.
- Remove `.tool-call` box styles (background, border).
- Use a table or grid layout for the log stream to ensure alignment of role markers and content? Or just flex row.
- `details` and `summary` tags for collapsible large content.

### JavaScript
- `render(data)`:
    - Iterate logs.
    - For each entry, determine type.
    - `formatJson(obj)`: Logic to check string length. If short -> return `<span>{json}</span>`. If long -> return `<details><summary>JSON</summary><pre>{json}</pre></details>`.
    - `formatText(text)`: Keep the truncation logic but styling should be minimal.

## Steps
1.  **Refactor HTML/CSS:** Update `static/log.html` to remove chat-specific styles and adopt the `index.html` aesthetic.
2.  **Update JS Logic:** Improve JSON formatting and tool call presentation.
3.  **Test:** Load a log and verify compactness.
