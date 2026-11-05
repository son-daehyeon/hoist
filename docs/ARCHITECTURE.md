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
  export.rs check.rs  PNG with pHYs, JPG, SVG, PDF (svg2pdf); print-resolution warnings
  boolean.rs          path union/subtract/intersect/exclude (i_overlay), text → outlines
  matting.rs          background removal: IS-Net on ONNX Runtime (ort, CPU), session kept after first use
  store.rs            project folder, per-turn undo/redo, versions, pins

src-tauri/src/
  main.rs             calls lib::run()
  lib.rs              plugins, setup (manages AppState), tauri-specta builder and invoke handler
  error.rs            Error (thiserror), serialized as ApiError { kind, message }
  state.rs            AppState: settings, open projects, running agent turns
  settings.rs         settings in tauri-plugin-store (settings.json); API key in the Windows credential manager (keyring), file fallback, OPENAI_API_KEY
  openai.rs           Responses API stream, compact, images, cost
  agent/              turn loop, tools, prompt, image helpers
  commands/           project, document, pathops, assets, render, fonts, export, agent, settings, dialogs, update
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
- An agent turn streams over a `tauri::ipc::Channel<AgentEvent>` passed to `agent_send` and `replace_original`. `AgentEvent` is `{ project_id, turn }` plus a `type`-tagged variant: `turn_start`, `thinking_start { round }`, `compacted { reason }`, `usage { turn_krw, project_krw }`, `text_delta`, `text`, `action`, `document_changed`, `report`, `question`, `turn_end`, `error`. An `action` carries an optional `detail`, a one-line Korean summary of the tool arguments, and the `round` it ran in. There is no global event.
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

One renderer for everything: the screen, the agent's eyes, and the export all come from `hoist-core` rendering the same document. The UI never computes document geometry itself. It shows the SVG fragments returned by `layer_svgs` and draws only chrome (selection boxes, rulers, handles) on top.

`layer_svgs(id, have)` returns `{ width_mm, height_mm, assets_dir, layers: [{ id, key, svg, bbox_mm }] }`, one entry per root layer in z-order, preceded by `__background` when the artboard has a background. Each `svg` is one `<g data-layer="ID">` in artboard mm, with no scale applied and children nested as their own `<g data-layer>`. Text is already outlined with cosmic-text. Hidden layers are included with `data-hidden="1" display="none"`, so CSS can show them. Each fragment carries its own `<defs>`, with gradient, filter and clipPath ids of the form `{root id}-{kind}{n}`, so fragments can be placed side by side in one `<svg viewBox>`. Images use `xlink:href="asset:NAME"`, which the UI resolves with `convertFileSrc(assets_dir + NAME)`. `bbox_mm` is `[x, y, w, h]`, including text ink, strokes and effects. `key` is 16 hex digits hashed from the subtree JSON and does not depend on zoom. Keys listed in `have` come back with `svg: null`. Fragments live in the same `RenderCache` (32 MB, least-recently-used), so an edit regenerates only its root. The command runs off the main thread (`command(async)`) and logs its counts and milliseconds at debug level. `cargo run -p hoist-core --example render_bench` measures a 100-layer document: about 19 ms in a debug build after one text layer moves. `build_svg`, behind raster, export, `path_boolean` and `outline_text`, joins the same fragments in `svg_document`, with images as file URLs or data URLs instead of `asset:`, so the screen and the output share one markup.

The canvas (`src/widgets/canvas`) puts the work pane in one `<svg viewBox="0 0 W H">` whose CSS size is W·zoom × H·zoom. Each fragment is a memoized `<g>` keyed by `id:key`, and the canvas asks again only when `doc.rev` changes, sending the keys it holds in `have`. Zoom and pan only change CSS, so they make no `layer_svgs` call. While dragging, `preview.ts` writes one `transform` per affected `[data-layer]` element; the matrix is computed in artboard mm and mapped through the parent's CTM, so nesting depth does not matter. Pointer-up sends every changed layer as one `apply_ops`. Hit testing uses `document.elementsFromPoint` and the nearest `[data-layer]`, skipping locked and hidden layers; in outline view it also tries points up to 3 px away so a click near a line selects it. The bounding-box test is used only before fragments arrive. Outline view (Ctrl+Y, kept in localStorage) is CSS on the `<svg>`: no fill, 1 px black non-scaling stroke, no filters, images hidden and replaced by a box with diagonals.

The asset protocol is on (`protocol-asset`). `tauri.conf.json` allows `$DOCUMENT/Hoist/**`. At startup, and whenever `set_settings` changes `projects_dir`, the app also allows that folder recursively. `list_fonts` returns `[{ family, display_name, category, weights, files: [{ weight, style, path }] }]`, grouped by each face's first family name. `Fonts::new` reads `manifest.json` in the bundled font folder and matches its entries to faces by file name. That gives `display_name` (Korean) and `category` (고딕, 명조, 둥근, 손글씨, 디스플레이), and both are `null` for fonts outside the manifest. Each manifest weight name (Regular, Bold, …) replaces the face's weight in fontdb, because some bundled files report usWeightClass 400 for every style (Suncheon, MuseumClassic). This fixes both the list and which file a weight renders with. Families sort by category in that order, then by display name, with fonts outside the manifest last. The agent's `list_fonts` tool returns the same catalog without file paths. A variable font lists every hundred weight its `wght` axis covers, with one file entry per weight pointing at the same file. The call also allows each font folder (not recursively), so the UI can use `@font-face` with `convertFileSrc(path)`.

Layout rules shared by screen and output:

- `scale_x` shapes the text in a box `w / scale_x` wide and then scales x, so wrapping and alignment hold.
- `skew_deg` shears each glyph about its baseline, with positive values leaning right.
- `vertical` stacks one glyph per `font_size × line_height + letter_spacing` and places lines from right to left.
- `arc_bend` maps each glyph's center along the box width, the chord, onto a circular arc and rotates the glyph to the tangent. At ±1 the arc is a half circle, and positive values bow upward.
- A line runs from the box's left-middle to right-middle. Its arrowheads are triangles four stroke widths long, filled with the stroke paint.
- `crop_px` shows a pixel window of the image through a nested `<svg viewBox>`.
- `adjust` applies `feComponentTransfer` for brightness and contrast, then `feColorMatrix saturate`.
- A layer is `<g data-layer>` (opacity, hidden) around `<g transform>`. Effect filters sit between the two, so shadow offsets stay in parent space. `drop_shadow` is a `<use>` of the body under it, with a filter that outputs only the shadows. `layer_blur` wraps the body in its own filter. The body itself is never inside the shadow filter, so svg2pdf keeps it vector.

`render_layers` goes through `ProjectHandle.render_cache` (`RenderCache` in `render.rs`). Each root layer's PNG is keyed by a hash of the scale, the artboard, and the JSON of the layer's whole subtree, and the background has its own key. Moving one layer therefore re-rasterizes only its root. Entries are evicted least-recently-used past 64 MB. Every layer in the result carries its cache `key`; the caller passes the keys it already holds in `have`, and those layers come back without `png_base64`. Each call logs its layer count, hits, layers sent, and milliseconds at debug level. Direct edits (`apply_ops`, undo, redo, restore) never render a thumbnail while holding the project lock: they schedule one render 1.5 s after the last edit (a per-project generation counter drops superseded requests) that writes `thumbnail.png` and the newest version's thumbnail if it has none.

Editing commands outside `apply_ops` still produce ops and go through the same `apply_and_record` path, so they are one undo step:

- `path_boolean(id, layer_ids, op)` (`union`, `subtract`, `intersect`, `exclude`) renders each Path, Shape or Text layer alone through `to_svg`, reads the filled geometry back with usvg in artboard mm, flattens curves to 0.05 mm, and runs i_overlay. The first layer becomes an evenodd Path in its own parent's space and keeps its paints, effects and z-order; the others are deleted. `subtract` is the first minus all the others.
- `outline_text(id, layer_id)` turns a Text layer into a Path layer with the same box, taking the outlines from the same rendered SVG, so width scale, skew and arcs carry over. `strategy_reason` becomes "글자 윤곽선화".
- `import_image(id, path)` copies PNG, and JPG without an EXIF rotation, into `assets/` unchanged. A rotated JPG is turned upright and saved at quality 92; WebP, BMP, GIF (first frame) and TIFF are turned upright and saved as PNG. It returns `{ asset, width_px, height_px }` and adds no layer.
- `remove_background(id, asset)` runs on a blocking thread without holding the project lock and writes `assets/nobg_NNN.png` with the predicted alpha (multiplied into any existing alpha), returning the same shape as `import_image`. The model is IS-Net general-use (Apache-2.0, 1024² input, about 1.5 s for a 2000 px photo on a 12-thread CPU, under 1 GB RAM). `onnxruntime.dll` 1.31.1 is loaded by absolute path from the models folder, never from `System32`. The folder is `<install dir>/models`, or `src-tauri/models` in development; `scripts/fetch-models.ps1` downloads and hash-checks both files with their licenses, and `build-installer.ps1` copies the folder into the payload. Without it the command answers "누끼 모델이 없어요".
- `duplicate_project(id, name)` copies `document.json`, `assets/`, `originals/`, `analysis/`, `memory.md` and `thumbnail.png` into a new project with a new id, name and timestamps and `usage_krw` 0, so it works as a template. Chat, versions and ops are not copied.
- `export_project` formats: `png` (pHYs = dpi × scale), `jpg` (quality 92, flattened on white, JFIF density = dpi × scale), `svg`, and `pdf` (svg2pdf 0.13, vector with outlined text, page size = artboard mm ÷ scale; filters are rasterized at dpi × scale).

Tracing (`trace.rs`, used by `trace_region`) builds a soft color-coverage mask, upsamples it 8× with Lanczos3, blurs it (σ = 0.5 source px), and thresholds it again before vtracer runs in spline mode. The corner, splice, length and speckle thresholds are scaled to the upsampling (`TraceOptions::upsampled`). The upsampled area is capped at 16M px, so large regions get a smaller factor. `cargo run -p hoist-core --example trace_samples` writes `docs/samples/trace-before.png` and `trace-after.png` from the sample banner.

## Agent rules (enforced in code, not prompts)

1. Every edit tool returns a render of the touched region plus a compare-with-original score in the same tool result.
2. A turn can only end after a full `compare` + `export_check` pass (or the agent explicitly reports why not).
3. Properties listed in `layer.pinned` were edited by the user; edit tools reject changes to them unless the user asked in this turn.
4. When the original is replaced the system injects a mandatory `ask_user` before any edit.
5. Canvas is locked (UI) while a turn runs.
6. Every tool call appends an `OpEntry`; one agent turn = one undo step.

## Agent context (`agent/mod.rs`, `agent/tools.rs`)

Context is kept, not summarized: the turn is a one-off job and quality beats cost. Text in the input items is never removed. Each response's `usage.input_tokens` is recorded. When it passes 300K, every image except the latest 4 is replaced by a text placeholder naming the tool, its arguments, and the round it was seen in. If the API still reports a context overflow (`context_length_exceeded`, or "maximum context length" in the error), images are cut to the latest 2 and the request is retried. Only if that also fails does the loop call `/responses/compact`. The compact instructions name what must survive: the layer plan and strategies, measured colors, widths and coordinates, decisions, remaining steps, and questions with their answers. The UI then gets a `compacted` event. Every request sends `prompt_cache_key = project_id` so the shared prefix stays cached. A turn stops after 200 rounds. Requests set `parallel_tool_calls: true`; every call in a round is marked running at once and runs as its own task (synchronous tools on `spawn_blocking`), actions update as each call finishes, and the outputs go back to the model in the original call order. Stopping aborts the remaining tasks and answers their calls with an error output. Each round's text cost plus the image cost accrued in that round is added to `Project.usage_krw` and sent as a `usage` event; the monthly total in settings is still kept.

Read tools (`get_document`, `view_original`, `view_work`, `compare`, `analyze_original`, `measure`, `crop_original`) are memoized per turn. The key is the tool, its arguments with nulls dropped, and the document `rev` or original version. A repeat returns text only, such as "변경 없음 (rev N, 라운드 k와 동일)" or "라운드 k 결과와 동일: …". The action's `detail` then ends in "이미 본 결과". When images are demoted, memo entries that returned images are forgotten so the agent can look again. `analyze_original` also caches to `analysis/v<n>.json`. The prompt tells the agent not to ask again for what it has already seen.

`add_layer` and `update_layer` use OpenAI strict schemas (`agent/layer_args.rs`): every field is required, optional ones are nullable, and the description carries example calls. If the API rejects a strict schema, the turn retries with `strict` removed. Parsing is still lenient: `kind: "group"` as a string, a `group` name as the parent, and image, group or background layers without coordinates (they fill the artboard) are accepted, and a missing `strategy` is inferred from the kind. When a layer still does not parse, the error contains the minimal JSON shape for that kind. A `update_layer` patch only applies its non-null fields.

`Project.fit_mode` (`extend_background`, `relayout`, `ask`; default `ask`) says how to place the original when its aspect ratio differs from the banner by more than 5%. With `ask`, the agent asks before its first edit and saves the answer with the `set_fit_mode` tool, and edit tools are refused until it does; the UI can set it through `create_project` or the `set_fit_mode` command. `relayout` is the exception to "reproduce the original exactly": groups may be rearranged freely for information hierarchy and legibility at a distance, and text may grow while keeping its relative sizes; colors, fonts, wording and element kinds stay. The prompt also says to ignore noise, dust and compression artifacts in the original, to send independent tool calls together, and to name layers in short spaced Korean (`tidy_name` turns `_` into spaces). Image generation prompts end in "clean, no noise, no dust, no artifacts".

Requests ask for no reasoning summary (`reasoning: {"effort": …}` only). Every round starts with `thinking_start { round }`, which the UI uses to time how long the model thinks.

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
