# Hoist — UI spec (from the Figma file "Hoist — App Design")

Reference renders are in `docs/design/*.png`. Match them. Style: light desktop program (Figma / Sketch), not a web app. No cards, no pills, no speech bubbles. Panels sit edge to edge with 1 px dividers.

## Tokens (CSS variables, light only)

Colors

```
--gray-0 #FFFFFF  --gray-50 #FAFAFA  --gray-100 #F4F4F5  --gray-150 #ECECEE  --gray-175 #E7E7EA
--gray-200 #E2E2E5 --gray-300 #CFCFD4 --gray-400 #A1A1A8 --gray-500 #6B6B72 --gray-600 #52525A
--gray-700 #3F3F46 --gray-800 #27272A --gray-900 #18181B
--blue-50 #EAF3FF --blue-500 #0A84FF --blue-600 #0066D6   --red-50 #FEF2F2 --red-600 #D92D20

bg: canvas=gray-175, surface=gray-0, chrome=gray-50, subtle=gray-50, muted=gray-100, hover=gray-100,
    pressed=gray-150, selected=blue-50, primary=gray-900, primary-hover=gray-800, accent=blue-500,
    inverse=gray-900, danger=red-600, danger-subtle=red-50
text: primary=gray-900, secondary=gray-600, tertiary=gray-500, disabled=gray-400, on-primary=gray-0,
      on-inverse=gray-0, link=blue-600, danger=red-600
border: default=gray-200, strong=gray-300, focus=blue-500, selection=blue-500, danger=red-600
icon: primary=gray-700, secondary=gray-500, disabled=gray-400, accent=blue-500, on-primary=gray-0
```

Blue is used ONLY for selection, focus rings, active tool, selected rows, progress. Never for buttons or text. Red only for errors.

Spacing 2 4 6 8 12 16 20 24 32. Radius sm 3, md 4, lg 6, xl 8. Shadows: none on cards; `md` 0 4 12 rgba(0,0,0,.12) for menus/tooltips; `lg` 0 12 32 rgba(0,0,0,.18) for dialogs.

Type: Pretendard (fallback system-ui). Load from `public/fonts` if available, else `font-family: Pretendard, "Pretendard Variable", system-ui, sans-serif`.

```
display 22/30 600 · title-l 18/26 600 · title-m 15/22 600 · title-s 13/18 600
body-l 14/22 400 · body-m 13/20 400 · body-m-strong 13/20 500
label-l 14/20 500 · label-m 13/18 500 · label-s 12/16 500 · caption 12/16 400 · micro 11/14 400
```

Motion: `--dur-fast` 120ms (hover, menu) · `--dur-base` 200ms (dialog, toast, tab content) · `--dur-slow` 320ms (progress, layer-added flash) · `--ease-out` cubic-bezier(.2,.8,.2,1) · `--ease-in-out` cubic-bezier(.4,0,.2,1); all durations and animations drop to 0 under `prefers-reduced-motion: reduce`.

## Icons

`src/shared/ui/icons.ts` has 71 stroke icons (24 viewBox). Render with `<Icon name size={16} />`: stroke currentColor, width 1.75 at 20 px (scale proportionally), round caps/joins, no fill.

## Components (sizes are exact)

- Button: kinds primary(black bg, white text) / secondary(white, 1px gray-300 border) / ghost / danger(red bg). Sizes M 28 px (padding 0 10, label-m) and L 36 px (padding 0 14, label-l). Optional 16 px leading icon. Disabled: muted bg, disabled text.
- IconButton 28×28, radius 4; hover gray-100; active = blue-500 bg + white icon (active tool / toggle). Always has a tooltip.
- ToolButton (left rail) 48×48 vertical: icon 20 + micro label; active = gray-150 bg.
- Toolbar field 24 px high, gray-100 bg, radius 3, 62–76 px wide: micro prefix (X/Y/W/H) + label-s value; focus = white bg + 1px blue border.
- Toolbar dropdown 24 px: optional 14 px icon, label-s, 12 px chevron-down. Toolbar swatch: 14 px color square + hex. Toolbar divider 1×16.
- Field: label (label-s secondary) above; input M 32 / L 36 px, radius 4, 1px gray-300 border, focus 2px blue; optional suffix (`mm`) in caption tertiary; optional helper (caption; red on error).
- Select: like Field with chevron. Search field 32 px with search icon. Checkbox 16 px (checked = black bg white check).
- Tab (underline): label-m, 2px black underline when active. Segmented control: gray-100 track radius 4 padding 2, segments 24 px radius 3, active = white + 1px gray-200 border.
- Panel header 36 px (title-s + right icon button, bottom border). Section header 28 px (chevron 12 + label-s secondary). Layer row 28 px: indent 6/24, chevron for groups, 14 px type icon, body-m name, micro tertiary tag (처리 방식), 12 px lock when pinned, 14 px eye. Selected = blue-50 bg, hover = gray-100.
- Project tile 240 wide: thumbnail 240×135 (white, 1px border, radius 4, hover border gray-300 + shadow sm + "⋯" button) then label-m title + caption meta. Project list row 40 px.
- Version row 44 px: 56×32 thumbnail, label-s time, caption summary; current = blue-50.
- Nav item 32 px, radius 4, icon 16 + label-m; active gray-150.
- Window controls: three 46×32 areas with 10 px minimize/maximize/close icons (Tauri custom titlebar; close hover #C42B1C bg + white icon).
- Tooltip: black bg, white label-s, optional shortcut in gray-400, radius 4, shadow md.
- Menu: white, 1px border, radius 6, padding 4, shadow md, 200 wide; items 28 px (icon 16, body-m, shortcut micro tertiary); dividers; section headings 28 px label-m tertiary; taller than the window it scrolls.
- Dialog: white, radius 8, shadow lg; title bar 44 px (title-m + close, bottom border); body padding 20; footer 52 px (top border, buttons right). Widths 480 / 560 / 640 / 720. Dim backdrop rgba(24,27,27,.4).
- Toast: black bg, white label-m, 36 px, radius 6, icon 16; bottom-center of the canvas, auto-hide 3 s. Error tone = red bg.
- Progress line 2 px blue at the top of the canvas while the agent works.
- Tag: micro text only (tertiary / link / danger), no background.
- Popover: white, 1px border, radius 6, shadow md, anchored below its trigger, or to the right of the nearest `[data-popover-edge]` panel (else the trigger); closes on outside press or Esc; fades in at `--dur-fast`.
- ScrubField 28 px, gray-100 bg, radius 4: body-m tertiary prefix that scrubs on left/right drag (ew-resize cursor, Shift ×10) + body-m value + optional suffix; arrows ±step (Shift ×10); commits once on Enter, blur or drag end, Esc reverts; empty with "혼합" placeholder when values differ; focus = white bg + 1px blue border.
- SliderField: one row of body-m secondary label (44 px) + native range (accent gray-900) + 68 px ScrubField without a prefix; commits once on release, key-up or Enter.
- Color swatch 24×24, radius 4, 1px gray-300 border over a checkerboard, shows the paint (solid or gradient); minus icon when mixed; tooltip names what it colors.
- Color picker (popover 248 wide, right of the swatch): 단색|그라디언트 segmented (label-m), eyedropper icon button, saturation/brightness square 148 px, hue and opacity sliders 12 px with white ring thumbs, `#` hex field + 불투명 % ScrubField, 최근 쓴 색 (12, localStorage) and 이 현수막에 쓴 색 (colors used in the document) as 8-column chips. Drags preview in the picker only and commit once on release.
- Gradient editor (inside the color picker): 24 px bar over a checkerboard, press to add a stop; 14×18 stop handles drag along the bar, selected stop ringed blue; 각도 ScrubField + 28 px dial (drag to turn, Shift 15° steps) + minus button to remove the selected stop (at least two stay).
- Eyedropper: while active the whole window shows a crosshair cursor and the picker shows "원본이나 작업본을 누르면 그 색을 집어요. Esc는 취소"; one press picks the on-screen pixel of the original or work artboard and nothing else reacts to that press.
- Properties panel: panel header with the selected layer name (or "레이어 N개", or the project name when nothing is selected), then collapsible sections (36 px header: 12 px chevron + label-m, optional + button; body padding 0 12 12; open state kept in localStorage): 위치와 크기, 글자 (font button showing the display name in its own face; it opens a 300 wide popover to the right of the panel with a 글꼴 찾기 search field and the list grouped by category (고딕, 명조, 둥근, 손글씨, 디스플레이, 그 외) then 내 컴퓨터 글꼴, sticky label-m tertiary group titles, 36 px rows with each display name drawn in its own font (bundled fonts load through FontFace when the row scrolls into view); the layer stores the family name, and 굵기 lists only that font’s weights, 크기, 굵기, 자간, 행간, 정렬, 장평 / 기울임 / 아치 sliders, 세로쓰기 checkbox; a non-zero 아치 also centers the text), 채우기 / 외곽선 / 효과 lists with − per row, 도형 (모양 select 사각형·원·선·다각형·별 that swaps the whole kind, then 둥근 모서리 / 변 / 꼭짓점·안쪽 / 시작·끝 화살표), 그림 (맞추기·채우기·늘이기, [배경 지우기] which spins a loader icon and reads "배경 지우는 중…" while it runs, [자르기 되돌리기] only when cropped, 밝기·대비·채도 sliders), and 크기 (mm) (가로, 세로, 재단, 배경) when nothing is selected. Pictures come in only through ≡ › 파일 › 그림 넣기… or by dropping files on the window. With any selection a 정렬 section follows 위치와 크기: title "정렬 (현수막 기준)" for one layer or "정렬 (고른 것끼리 기준)" for several, a 3-column row of 왼쪽 · 가운데 · 오른쪽 · 위 · 중간 · 아래 and 가로 간격 같게 · 세로 간격 같게 (3+ layers; the two outermost stay put); rotated layers align by their bounding box, locked layers do not move, and one press is one undo step. With any selection a 작업 section ends the panel: 합치기 · 빼기 · 교차 · 제외 (2+ path, shape or text layers), 윤곽선 만들기 (one text layer), 복제 (+10 mm) · 삭제 as secondary M buttons. All text 13 px or larger.
- 정보 dialog: app name and version, one caption line, then under a divider [글꼴 라이선스 보기] which opens the bundled fonts’ LICENSES.md with the system viewer, and a body-m tertiary line explaining it.
- Layer tag for a picture the user placed (strategy `user_image`): 사진.
- Agent question dialog (480, or 560 with a picture): opens in the middle of the editor whenever the agent waits on a question, with no close button and Esc ignored. Title "에이전트 질문", "질문 1 / 3" (label-m tertiary) when several questions came together, the question in body-m, the related original crop (max 480 wide) on a gray canvas when given, options as full-width L buttons (first primary), a "직접 적어 주세요" field with [답하기] when free text is allowed, and a ghost [에이전트가 알아서] in the footer that answers `__auto__`. Each answer goes through the same path as the chat card and the next question follows.
- Editor ≡ menu: 파일 (새 프로젝트, 폴더에서 열기, 그림 넣기…) · 보기 (윤곽선 보기 ✓ Ctrl+Y, 재단선 · 안전 영역 보기 ✓, 원본 겹쳐 보기 끄기 / 50% / 100% as three checked items) · 오브젝트 (복사, 붙여넣기, 복제, 삭제, 윤곽선 만들기, 패스 합치기·빼기·교차·제외, 그림 자르기 for one image, 맨 앞으로 · 앞으로 · 뒤로 · 맨 뒤로; disabled when they do not apply) · 정렬 (the same six align items and two distribute items) · 설정 · 도움말 · 정보. Dropping image files on the editor window places them as image layers: one third of the artboard width at most, aspect kept, centered, inside the 그래픽 group when it exists.
- Assistant panel pieces: Message user = gray-100 block radius 6 padding 8 10; Message agent = 18 px black rounded avatar with sparkles + body-m text, and when the turn did work a one-line caption summary on top ("› 작업 93개 · 레이어 46 · 그림 8 · 누끼 6 · 생각 4분 · 비용 ₩1,049" with red "실패 N" and gray "중단 N" tags; top three kinds by first word, viewing actions not counted, thinking summed over rounds, live while running). While running only the current round thinking and actions show under it; when done only the summary and the final reply stay, and clicking the summary unfolds the whole timeline (while working the avatar fades between full and 35% opacity, 1.2 s ease-in-out alternate, still under reduced motion); Action list = 1px border box, header "작업 2/5", rows 22 px with 14 px status icon (check / spinner blue / circle gray / alert red) + caption; Compare report = two 146×46 thumbnails labelled 원본/작업본 + caption stats; Question = 1px gray-300 box with body-m question and full-width secondary/primary M buttons; Edit summary = gray-50 box with label-s title + caption lines; Context tag 20 px gray-100 radius 3 (12 px icon + micro + 10 px ×); Composer = 1px gray-300 box radius 6 padding 8: tags row, body-m placeholder, bottom row [paperclip][layers] … [28 px black send button]. Locked state: placeholder "작업이 끝나면 입력할 수 있어요" and a secondary "중단" button instead of send.

## Screens (1440×900 reference; must be fully responsive to the window)

Title bar 40 px, white, bottom border: [≡ menu][← back (editor only)] | title label-m + caption size … right: editor actions … [window controls]. The whole app window is frameless; title bar is the drag region.

### Home (`docs/design/home-grid.png`, `home-list.png`, `home-menu.png`)

Left nav 220 px, gray-50, right border: 최근 항목 / 모든 프로젝트 / 휴지통 … 설정 at bottom. Content: header row (title-l "최근 항목", search 240, grid/list toggle icon buttons, [+ 새 프로젝트] primary L), sub row (sort dropdown "최근 수정 순", caption count), then grid (240 tiles, gap 24/28) or list (header row + 40 px rows). Empty state centered. ≡ opens the menu (새 프로젝트, 폴더에서 열기, 설정, 도움말, 정보). Tile ⋯ menu: 이름 바꾸기, 복제, 폴더에서 보기, 삭제.

### New project dialog 720 (`new-project-uploaded.png`)

Tabs under the title: 원본 이미지로 시작 | 빈 현수막으로 시작. Body two columns: left 320 = dropzone (dashed, upload icon, "현수막 이미지를 여기에 놓으세요", [파일 선택], caption) → after upload a 320×180 preview on gray canvas with file name caption and [바꾸기]; right = 이름 (L field), 실물 가로 / 실물 세로 (L fields with `mm` suffix, always typed by the user — no presets), 미리보기 box (gray-50, proportional rectangle + "6.7 : 1" + "100dpi 기준 23622 × 3543 px"), and in upload mode a caption about ratio mismatch. Footer: 취소 | 만들고 분석 시작 (primary, disabled until image + name + size are valid).

### Editor (`editor-*.png`)

Title bar: [≡][←] | project name + size … [undo][redo] | [기록 ghost][원본 교체 ghost] | [내보내기 primary M] | window controls.
Below: tool rail 56 px (선택 cursor, 이동 hand, 글자 type, 도형 square, 펜 pen, 캡처 crop, 비교 compare) · left column 240 px = properties panel on top (height dragged on the divider, kept in localStorage) + layer panel (panel header "레이어" + "+", rows) · canvas column · assistant panel 340 px.
Canvas column: no header bar, so the canvas starts right under the title bar (position and size live in the properties panel, guides and the original overlay in the ≡ 보기 menu); rulers (20 px, gray-50, mm ticks: minor 100 mm, major 500 mm with micro labels) on top and left, gray canvas with two artboards stacked: 원본 (label above, micro) and 작업본 (label turns blue when something is selected). Selected layer: 1px blue outline, eight 8×8 white handles with blue border, and a blue size label "1602 × 247 mm" under it.
Rotation handle: a 10 px white circle with a blue border, 22 px above the top-center handle on a 1 px blue stem; dragging it rotates about the center (Shift = 15° steps); resizing a turned layer snaps the dragged corner or edge point to the same guides, moving only along the layer axes, the outline, handles and label turn with the layer, and the label adds the angle ("3360 × 90 mm · 30°"). Several layers: each gets its own 1 px outline and the union gets a dashed outline with the handles.
Duplicate: Ctrl+D copies the selected layers 10 mm right and down and selects the copies; Alt+drag leaves the original in place and drags a copy (snapping stays on while Alt is held for the copy).
Shape tool (rail 도형, R): the rail button opens a popover with 사각형 · 둥근 사각형 · 원 · 선 · 다각형 · 별 (선 adds 끝에 화살표, 다각형 and 별 add a 변·꼭짓점 count); drag to draw with a 1 px blue live outline, Shift keeps equal sides or 45° lines, a click alone drops a default size (20% of the shorter artboard side). Fill is the last color picked in the color picker with no stroke (a line takes that color as its stroke); the tool returns to 선택 after drawing.
Inline text edit: double-click a text layer (or click with the 글자 tool to make one) and type on the canvas. The layer glyphs hide and a borderless textarea sits on the layer box under a 1 px blue outline, turned with the layer, in the layer font, weight, size, letter and line spacing, alignment and 장평. Enter adds a line, Ctrl+Enter or a click outside saves, Esc cancels, and saving empty text deletes the layer. In outline view the typed glyphs show as 1 px black outlines. Vertical text edits top to bottom in right-to-left columns, one column per line; skew and arch are not shown while editing.
Crop: double-click an image layer or choose ≡ 오브젝트 › 그림 자르기 with one image selected. The whole picture shows in place, dimmed 55% outside the crop window, with eight handles on the window, dragging inside moves it within the picture, and a label "자르기 419 × 434 px". Enter or a click outside saves the crop in source pixels, Esc cancels; the layer box keeps its size and the crop is fitted into it with the layer fit. It works the same in outline view.
Pen tool (rail 펜, P): click places a corner point, click-and-drag places a curve point with mirrored handles, a dashed blue line follows the pointer from the last point, clicking the first point closes the path, and Enter or Esc ends an open path. A closed path is filled with the last color picked, an open path gets a stroke in that color; the pen stays active for the next path.
Path edit: double-click a path layer. Its points show as 8 px white squares (curve points round, the selected one blue) with handles for the selected point; drag a point or a handle (curve handles stay mirrored), Alt+click a point to switch corner and curve, click the path line to add a point, Delete removes the selected point. Enter or a click outside saves, Esc cancels; the layer box is refitted to the new shape without moving the rest of the drawing. Works the same in outline view. Resizing a path layer with the handles scales its shape with the box.
Clicking inside a selected layer box where nothing is drawn keeps the selection and drags it (needed in outline view, where shapes are hit only on their lines).
Outline view (Ctrl+Y, or the shape button left of the zoom controls): artboard white, every shape and glyph as a 1 px black line at any zoom, no fills or effects, images shown as a box with both diagonals; selection chrome and guides unchanged. Capture tool: dashed blue rectangle with 8% blue fill and a tooltip "캡처를 채팅에 붙였어요". Status bar 24 px gray-50 top border: left micro status, right "6000 × 900 mm · 100dpi 기준 23622 × 3543 px" and zoom [−] 12% [+] [fit]. While the agent works: an indeterminate 2px blue progress line sliding along the top of the canvas and a 50% white overlay over rail+layers+canvas.
Assistant panel: header 36 px with tabs 도우미 | 기록, the project cost in micro tertiary ("₩2,480", hover tooltip "대화 ₩2,160 · 이미지 ₩320"), "이미지 ₩320" next to it once pictures cost anything, and ⋯; body scrolls; composer pinned at the bottom with 12 px padding. 기록 tab lists version rows and a footer [이 버전으로 되돌리기] + caption.

### Dialogs

원본 교체 560: dragover dropzone (blue-50 bg, blue dashed border) + caption "지금 원본(v1)은 지우지 않고 기록에 남겨요"; 취소 | 올리기. After upload the assistant asks a Question (3 options) and the composer is locked until answered.
내보내기 640 (`export-png.png`, `export-svg.png`): left 280 = preview on gray canvas + "파일 크기 600 × 90 mm · 1:10" + folder path; right = 형식 segmented PNG|JPG|PDF|SVG (JPG adds "JPG는 투명한 곳 없이 흰 배경으로 저장돼요."; PDF always outlines text, shows the dpi select as "그림 해상도 · 실물 기준", and when a side passes 5080 mm offers only 1:10 with "5080mm를 넘는 PDF는 1:10으로 내보냅니다(Acrobat 한계)."), 축척 segmented 1:1 실물|1:10 with caption note, PNG: 해상도 select "100dpi · 권장" + caption px size; SVG: checkbox 글자를 윤곽선으로 바꾸기 + caption about raster layers; warning box (red-50, alert icon, "실물 기준 해상도가 모자란 레이어 2개" + items) when `export_check` returns warnings, with footer 고치러 가기 | 그대로 내보내기. Success → toast "바탕화면에 저장했어요".
설정 560: Field L OpenAI API 키 (password, helper), 프로젝트 저장 폴더 + [찾아보기], usage row; 취소 | 저장.

## Interaction notes

- Keyboard: V select, H hand, T text, R shape, P pen, C capture, Ctrl+Z undo, Ctrl+Shift+Z redo, Ctrl+Y outline view (윤곽선 보기, also a checked item in the ≡ menu), Ctrl+D duplicate (canvas), Ctrl+C / Ctrl+V copy and paste layers inside the app (each paste lands another 10 mm right and down, into the same group when it still exists; pasting into another project copies the pictures into it), Ctrl+] / Ctrl+[ one step forward / back among siblings and Ctrl+Shift+] / Ctrl+Shift+[ to the very front / back, Delete, Esc, Ctrl+E export, Ctrl+N new.
- Zoom: wheel+Ctrl, buttons, "fit". Pan: space+drag or hand tool. Zoom shows percent where 100% = 1 mm per px.
- Dragging a layer moves its SVG fragment immediately (transform attribute only) and commits an `update_layer` op on drop, then re-renders.
- Everything the user edits directly (move, resize, text props, fills, strokes, effects) goes through `applyOps` with actor `user`; the backend records pinned properties and the edit summary.
- One agent turn = one undo step: undo while the agent panel shows a finished turn reverts all of that turn's ops.
