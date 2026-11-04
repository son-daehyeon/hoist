# Banner Studio — Architecture

Desktop app (Tauri 2, Windows first) that turns a small banner image (≈2000 px, received via KakaoTalk) into a layered, print-resolution document (real size in mm, 100 dpi target) with an AI assistant that does the rebuilding. User is a non-designer in his 50s. Korean UI.

## Processes

One process. No Node sidecar, no cloud server.

- `src-tauri/` — Tauri app: use cases and the agent loop, behind ports for the model, UI events, and the API key store.
- `crates/core/` — pure Rust library: document model, ops/undo, persistence, rendering, export, image analysis. No Tauri, no network.
- `src/` — React 19 + TypeScript + Vite UI. Talks to the backend only through `src/shared/api` (Tauri `invoke`) and the `agent-event` Tauri event.

## Contracts (fixed — do not change without updating all three)

- `crates/core/src/domain/model.rs` — document model + ops (Rust).
- `src/shared/api/types.ts` — the same model in TypeScript. Field names and enum strings are identical (`serde` snake_case, tagged enums with `type`/`shape`/`op`).
- `src/shared/api/tauri.ts` — every Tauri command the UI may call, with argument and return types.

Units: everything geometric is **mm** (`x_mm`, `w_mm`, `font_size_mm`, …). Colors are `#RRGGBB` strings plus a separate `opacity` 0–1. Angles in degrees. Pixel sizes only appear for originals/assets (`width_px`).

## Frontend layout (`src/`, Feature-Sliced Design)

```
app/        entry (main.tsx), App shell with error boundary
pages/      home, editor
widgets/    title-bar, app-menu, layer-panel, canvas, assistant-panel
features/   project-create, project-manage, project-open, export, replace-original, settings
entities/   editor (editor session state and context), layer (layer factory, strategy labels)
shared/     ui (design system, tokens.css), lib (formatting, layer tree), api (Tauri commands, contract types, mock backend)
```

Layers import only downward: `app > pages > widgets > features > entities > shared`. A slice never imports another slice of its own layer, so code needed by two slices moves down a layer. Outside a slice, import only its `index.ts` (or a `.css` file) through the `@/` alias, e.g. `@/shared/ui`. Inside a slice, use relative paths. `shared` segments reach each other's `index.ts` by relative path, so `shared/` never needs the alias and the installer can compile `shared/ui` without it. Each slice imports its own CSS. `pnpm lint` enforces all of this with `eslint-plugin-boundaries`, and every file under `src/` must belong to a layer. `shared/api/backend.ts` uses the Tauri API inside the app and the in-memory mock in a plain browser (`pnpm dev`).

## Backend layout (Rust, ports and adapters)

```
crates/core/src/   banner-core
  domain/          model.rs (contract types), ops.rs (merge-patch ops and inverses)
  render/          mod.rs (document → SVG → resvg), text.rs (cosmic-text outlines, Fonts)
  raster/          image.rs, compare.rs, trace.rs (vtracer), analyze.rs (palette, segments)
  export/          mod.rs (PNG with pHYs, SVG), check.rs (print-resolution warnings)
  project/         store.rs (project folder, per-turn undo/redo, versions, pins)

src-tauri/src/
  lib.rs           composition root: builds AppState, chooses adapters, registers commands
  state.rs         AppState: settings, open projects, running agents, port objects
  application/     use cases: project, document, render, export, settings, agent/ (turn loop, tools, prompt, img)
  ports/           llm.rs (Llm), events.rs (AgentEvents), secrets.rs (ApiKeyStore)
  adapters/        openai.rs, keyring.rs, tauri/events.rs, tauri/commands/*
```

Dependencies point inward: `adapters → application → ports`, and `application → banner-core`. `banner-core` knows nothing of the app. `application` never names an adapter, `tauri`, `reqwest`, or `keyring`; it reaches the model, the UI event stream, and the key store only through the three port traits, and `lib.rs` is the one place that picks their implementations. Those three seams are the only traits; everything else is a plain module. A Tauri command parses its arguments, calls one application function, and maps the error to a string. An agent turn outlives its command, so the application returns it as a future and the command spawns it. The `Llm` port takes an OpenAI Responses-style request body, so another provider would need its own request mapping.

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
