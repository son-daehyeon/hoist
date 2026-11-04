# Banner Studio — Architecture

Desktop app (Tauri 2, Windows first) that turns a small banner image (≈2000 px, received via KakaoTalk) into a layered, print-resolution document (real size in mm, 100 dpi target) with an AI assistant that does the rebuilding. User is a non-designer in his 50s. Korean UI.

## Processes
One process. No Node sidecar, no cloud server.
- `src-tauri/` — Tauri app: commands, settings, project folders, OpenAI client, agent loop.
- `crates/core/` — pure Rust library: document model, ops/undo, persistence, rendering, export, image analysis. No Tauri, no network.
- `src/` — React 19 + TypeScript + Vite UI. Talks to the backend only through `src/api.ts` (Tauri `invoke`) and the `agent-event` Tauri event.

## Contracts (fixed — do not change without updating all three)
- `crates/core/src/model.rs` — document model + ops (Rust).
- `src/types.ts` — the same model in TypeScript. Field names and enum strings are identical (`serde` snake_case, tagged enums with `type`/`shape`/`op`).
- `src/api.ts` — every Tauri command the UI may call, with argument and return types.

Units: everything geometric is **mm** (`x_mm`, `w_mm`, `font_size_mm`, …). Colors are `#RRGGBB` strings plus a separate `opacity` 0–1. Angles in degrees. Pixel sizes only appear for originals/assets (`width_px`).

## Project folder (`<projects_dir>/<project id>/`)
```
project.json       Project
document.json      Document (current)
ops.jsonl          OpEntry per line (append-only; undo/redo pointer stored in project state file)
versions/<seq>.json   Document snapshot after each agent turn and each batch of user ops
originals/v1.png, v2.png …
assets/            crops, generated images, rendered thumbnails (png)
chat.jsonl         ChatItem per line
memory.md          durable facts the agent must keep (never compacted)
thumbnail.png      composite preview for the home grid
```

## Rendering rule
One renderer for everything: the screen, the agent's eyes, and the export all come from `banner-core` rendering the same document. The UI never draws document content itself; it composites per-layer PNG rasters returned by `render_layers` and draws only chrome (selection boxes, rulers, handles) on top.

## Agent rules (enforced in code, not prompts)
1. Every edit tool returns a render of the touched region plus a compare-with-original score in the same tool result.
2. A turn can only end after a full `compare` + `export_check` pass (or the agent explicitly reports why not).
3. Properties listed in `layer.pinned` were edited by the user; edit tools reject changes to them unless the user asked in this turn.
4. When the original is replaced the system injects a mandatory `ask_user` before any edit.
5. Canvas is locked (UI) while a turn runs.
6. Every tool call appends an `OpEntry`; one agent turn = one undo step.

## Models (OpenAI only)
- agent default: `gpt-6.1-terra`
- heavy steps (decompose, choose strategies, retry after 2 guard failures): `gpt-6.1-sol`
- compaction / naming / summaries: `gpt-6-luna`
- images: `gpt-image-2.5-sunburst` (final), `gpt-image-2.5-flare` (drafts)

## Code rules
- No comments in code except for an external fact or a workaround reason.
- Rust: `anyhow` at the app edge, typed errors inside core. No `unwrap` outside tests.
- TS: strict, no `any`, no default exports except pages.
- Don't add dependencies for what a few lines do.
