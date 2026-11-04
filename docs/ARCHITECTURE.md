# Hoist — Architecture

Desktop app (Tauri 2, Windows first) that turns a small banner image (≈2000 px, received via KakaoTalk) into a layered, print-resolution document (real size in mm, 100 dpi target) with an AI assistant that does the rebuilding. User is a non-designer in his 50s. Korean UI.

## Processes

One process. No Node sidecar, no cloud server.

- `src-tauri/` — Tauri app: commands, settings, project folders, OpenAI client, agent loop.
- `crates/core/` — pure Rust library: document model, ops/undo, persistence, rendering, export, image analysis. No Tauri, no network.
- `src/` — React 19 + TypeScript + Vite UI. Talks to the backend only through `src/shared/api`: generated `bindings.ts` over Tauri `invoke`, plus a `Channel` per agent turn.

## Contracts (fixed — change one only together with the others)

- `crates/core/src/model.rs` — document model + ops (Rust).
- `src/shared/api/bindings.ts` — generated from the Rust commands and types (see IPC). Never edit it by hand.
- `src/shared/api/types.ts` — the names the UI imports, aliased onto bindings. A value the backend returns uses `X_Serialize`. A value the UI only builds and sends uses `X_Deserialize`, which covers `Op`, `ExportOptions`, `SettingsPatch` and `CreateArgs`. `ChatItem` is the one type defined here by hand, because the backend stores chat as raw JSON.
- `src/shared/api/backend.ts` — the `api` object the UI calls. It is `commands` from bindings plus a `Channel` per `agentSend`/`replaceOriginal` that feeds `api.onAgentEvent(handler) → unsubscribe`. The browser mock implements the same `Backend` type.

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

Layers import only downward: `app > pages > widgets > features > entities > shared`. A slice never imports another slice of its own layer, so code needed by two slices moves down a layer. Outside a slice, import only its `index.ts` (or a `.css` file) through the `@/` alias, e.g. `@/shared/ui`. Inside a slice, use relative paths. `shared` segments reach each other's `index.ts` by relative path, so `shared/` never needs the alias and the installer can compile `shared/ui` without it. Each slice imports its own CSS. `pnpm lint` enforces all of this with `eslint-plugin-boundaries`, and every file under `src/` must belong to a layer. `@tauri-apps/*` packages are imported only under `src/shared/api/`; everything else calls what `shared/api` exports, such as `windowControls`, `onDragDrop` and `errorMessage` (the single way to turn a command error into text). `no-restricted-imports` enforces this. `shared/api/backend.ts` uses the Tauri API inside the app and the in-memory mock in a plain browser (`pnpm dev`).

## Backend layout (Rust)

```
crates/core/src/      hoist-core, flat modules
  model.rs ops.rs     contract types; merge-patch ops and their inverses
  render.rs text.rs   document → SVG → resvg; cosmic-text outlines (Fonts)
  image.rs compare.rs trace.rs analyze.rs
                      raster helpers, original-vs-work compare, vtracer, palette and segments
  export.rs check.rs  PNG with pHYs and SVG; print-resolution warnings
  store.rs            project folder, per-turn undo/redo, versions, pins

src-tauri/src/
  main.rs             calls lib::run()
  lib.rs              plugins, setup (manages AppState), tauri-specta builder and invoke handler
  error.rs            Error (thiserror), serialized as ApiError { kind, message }
  state.rs            AppState: settings, open projects, running agent turns
  settings.rs         settings in tauri-plugin-store (settings.json); API key in the Windows credential manager (keyring), file fallback, OPENAI_API_KEY
  openai.rs           Responses API stream, compact, images, cost
  agent/              turn loop, tools, prompt, image helpers
  commands/           project, document, render, export, agent, settings, dialogs, update
```

This follows the Tauri docs: commands are thin functions that take `State<AppState>` or `AppHandle`, call `hoist-core` or `agent/`, and return `Result<T, Error>`. `hoist-core` never sees Tauri or the network. No traits stand in for Tauri or OpenAI; `agent/` uses `AppHandle` directly. Plugins are registered in this order:

1. single-instance, which focuses the existing window on a second launch.
2. devtools (CrabNebula) in debug builds, or log in release builds. Both install a global logger, so only one runs. devtools is constructed before the Tauri builder, so it captures from the start even though single-instance registers first.
3. window-state, store, updater, decorum, opener, dialog.
4. prevent-default with all flags, in release builds only.

decorum is used only for `plugin:decorum|show_snap_overlay`, which opens the Windows snap layouts. The overlay titlebar is never created, because the React titlebar owns the window chrome.

## IPC (tauri-specta)

Every command carries `#[tauri::command]` and `#[specta::specta]`, and every type that crosses the boundary derives `specta::Type`, the core model included. `lib.rs` lists the commands once in `collect_commands!`. The same builder supplies the invoke handler and writes `src/shared/api/bindings.ts`. The file is regenerated on every debug run and by `cargo test -p hoist export_bindings`, which opens no window. Commit it so the frontend builds without cargo.

- Accessors are camelCase (`commands.listProjects`); invoke names stay snake_case.
- Errors are thrown. `ErrorHandlingMode::Throw` is set explicitly because rc.26 defaults to returning a result object. The thrown value is `ApiError`, `{ kind, message }`, with kind `core`, `io`, `opener`, `updater`, `other`, `no_api_key`, `lock`, or `invalid`. `agent_send` and `replace_original` return `no_api_key` before doing anything when no OpenAI key is set.
- Where serde reads and writes a type differently, it is split by direction: `Layer_Serialize` is what commands return, with every field present, and `Layer_Deserialize` is what they accept, with `#[serde(default)]` fields optional.
- 64-bit integers and floats both export as `number`. This assumes integers stay below 2^53 and floats stay finite.
- `serde_json::Value` (merge patches) exports as `Value`.
- Chat history is typed: `ChatItem` is a `role`-tagged union of `UserChat` and `AgentChat`, the same shape as `chat.jsonl`. Every write to an agent item reads the stored item, changes its own fields, and writes it back, so `question` and `answer` survive later updates.
- Fixed choices are string unions (`Pane`, `ExportFormat`). `ExportScale` stays a JSON number and exports as `1 | 10` through a semantic rule in `lib.rs`.
- An agent turn streams over a `tauri::ipc::Channel<AgentEvent>` passed to `agent_send` and `replace_original`. `AgentEvent` is `{ project_id, turn }` plus a `type`-tagged variant: `turn_start`, `thinking_start { round }`, `thinking_delta`, `compacted { reason }`, `text_delta`, `text`, `action`, `document_changed`, `report`, `question`, `turn_end`, `error`. An `action` carries an optional `detail`, a one-line Korean summary of the tool arguments. There is no global event.
- `UpdateAvailable { version, notes }` is the one global event (`events.updateAvailable`). `update_install` streams `UpdateProgress` (`started`, `progress`, `finished`) over a Channel.
- On Windows, `src-tauri/build.rs` embeds `windows-app-manifest.xml` through a linker flag, because tauri-build gives the manifest only to bin targets and test binaries need Common Controls v6 to load.

## Updates

`scripts/build-installer.ps1` builds the app and the installer as `Hoist-Setup.exe`. It signs the installer with `tauri signer sign` and the key in `~/.tauri/banner-studio.key`, then writes `dist-installer/latest.json` with the version, notes, publish date, download URL, and signature. Both files go on the GitHub Release `v<version>`. Once the main window's page has loaded, the app fetches `releases/latest/download/latest.json` once and emits `UpdateAvailable` if the version is newer; failures are only logged. `update_install` downloads the installer, checks the signature against the pubkey in `tauri.conf.json`, and starts it with `/P /UPDATE /R /ARGS`, then the app exits. With `/S` or `/P` the installer runs without a window. It installs into the registered `InstallLocation`, or the default folder. It waits up to 15 s for `hoist.exe` to unlock, keeps the existing shortcuts, and with `/R` relaunches the app.

## Project folder (`<projects_dir>/<project id>/`)

```
project.json       Project
document.json      Document (current)
ops.jsonl          OpEntry per line (append-only; undo/redo pointer stored in project state file)
versions/<seq>.json   Document snapshot after each agent turn and each batch of user ops
originals/v1.png, v2.png …
assets/            crops, generated images, rendered thumbnails (png)
analysis/v<n>.json analyze_original result per original version (palette, regions)
chat.jsonl         ChatItem per line
agent_items.json   Responses API input items carried across turns
memory.md          durable facts the agent must keep (never compacted)
thumbnail.png      composite preview for the home grid
```

## Rendering rule

One renderer for everything: the screen, the agent's eyes, and the export all come from `hoist-core` rendering the same document. The UI never draws document content itself; it composites per-layer PNG rasters returned by `render_layers` and draws only chrome (selection boxes, rulers, handles) on top.

## Agent rules (enforced in code, not prompts)

1. Every edit tool returns a render of the touched region plus a compare-with-original score in the same tool result.
2. A turn can only end after a full `compare` + `export_check` pass (or the agent explicitly reports why not).
3. Properties listed in `layer.pinned` were edited by the user; edit tools reject changes to them unless the user asked in this turn.
4. When the original is replaced the system injects a mandatory `ask_user` before any edit.
5. Canvas is locked (UI) while a turn runs.
6. Every tool call appends an `OpEntry`; one agent turn = one undo step.

## Agent context (`agent/mod.rs`, `agent/tools.rs`)

Context is kept, not summarized: the turn is a one-off job and quality beats cost. Text in the input items is never removed. Each response's `usage.input_tokens` is recorded. When it passes 300K, every image except the latest 4 is replaced by a text placeholder naming the tool, its arguments, and the round it was seen in. If the API still reports a context overflow (`context_length_exceeded`, or "maximum context length" in the error), images are cut to the latest 2 and the request is retried. Only if that also fails does the loop call `/responses/compact`. The compact instructions name what must survive: the layer plan and strategies, measured colors, widths and coordinates, decisions, remaining steps, and questions with their answers. The UI then gets a `compacted` event. Every request sends `prompt_cache_key = project_id` so the shared prefix stays cached. A turn stops after 80 rounds.

Read tools (`get_document`, `view_original`, `view_work`, `compare`, `analyze_original`, `measure`, `crop_original`) are memoized per turn. The key is the tool, its arguments with nulls dropped, and the document `rev` or original version. A repeat returns text only, such as "변경 없음 (rev N, 라운드 k와 동일)" or "라운드 k 결과와 동일: …". The action's `detail` then ends in "이미 본 결과". When images are demoted, memo entries that returned images are forgotten so the agent can look again. `analyze_original` also caches to `analysis/v<n>.json`. The prompt tells the agent not to ask again for what it has already seen.

Each round asks for a detailed reasoning summary (`reasoning.summary = "detailed"`). Every round starts with `thinking_start { round }`, and `response.reasoning_summary_text.delta` streams as `thinking_delta`. Thinking is never written to `chat.jsonl`. If the API rejects that level, the turn retries once with `"auto"`. If it rejects summaries altogether (an unverified organization, for example), the rest of the turn runs without them.

## Models (OpenAI only)

- agent default: `gpt-6.1-terra`
- heavy steps (decompose, choose strategies, retry after 2 guard failures): `gpt-6.1-sol`
- naming / summaries: `gpt-6-luna`; compaction, when it happens at all, uses the turn's own model
- images: `gpt-image-2.5-sunburst` (final), `gpt-image-2.5-flare` (drafts)

## Code rules

- No comments in code except for an external fact or a workaround reason.
- Rust: typed errors in core and at the command boundary (`error.rs`), `anyhow` inside `agent/` and `openai.rs`. No `unwrap` outside tests.
- TS: strict, no `any`, no default exports except pages.
- Don't add dependencies for what a few lines do.
