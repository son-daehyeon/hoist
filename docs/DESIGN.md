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

## Icons

`src/shared/ui/icons.ts` has 68 stroke icons (24 viewBox). Render with `<Icon name size={16} />`: stroke currentColor, width 1.75 at 20 px (scale proportionally), round caps/joins, no fill.

## Components (sizes are exact)

- Button: kinds primary(black bg, white text) / secondary(white, 1px gray-300 border) / ghost / danger(red bg). Sizes M 28 px (padding 0 10, label-m) and L 36 px (padding 0 14, label-l). Optional 16 px leading icon. Disabled: muted bg, disabled text.
- IconButton 28×28, radius 4; hover gray-100; active = blue-500 bg + white icon (active tool / toggle). Always has a tooltip.
- ToolButton (left rail) 48×48 vertical: icon 20 + micro label; active = gray-150 bg.
- Toolbar field (context toolbar) 24 px high, gray-100 bg, radius 3, 62–76 px wide: micro prefix (X/Y/W/H) + label-s value; focus = white bg + 1px blue border.
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
- Menu: white, 1px border, radius 6, padding 4, shadow md, 200 wide; items 28 px (icon 16, body-m, shortcut micro tertiary); dividers.
- Dialog: white, radius 8, shadow lg; title bar 44 px (title-m + close, bottom border); body padding 20; footer 52 px (top border, buttons right). Widths 480 / 560 / 640 / 720. Dim backdrop rgba(24,27,27,.4).
- Toast: black bg, white label-m, 36 px, radius 6, icon 16; bottom-center of the canvas, auto-hide 3 s. Error tone = red bg.
- Progress line 2 px blue at the top of the canvas while the agent works.
- Tag: micro text only (tertiary / link / danger), no background.
- Assistant panel pieces: Message user = gray-100 block radius 6 padding 8 10; Message agent = 18 px black rounded avatar with sparkles + body-m text; Action list = 1px border box, header "작업 2/5", rows 22 px with 14 px status icon (check / spinner blue / circle gray / alert red) + caption; Compare report = two 146×46 thumbnails labelled 원본/작업본 + caption stats; Question = 1px gray-300 box with body-m question and full-width secondary/primary M buttons; Edit summary = gray-50 box with label-s title + caption lines; Context tag 20 px gray-100 radius 3 (12 px icon + micro + 10 px ×); Composer = 1px gray-300 box radius 6 padding 8: tags row, body-m placeholder, bottom row [paperclip][layers] … [28 px black send button]. Locked state: placeholder "작업이 끝나면 입력할 수 있어요" and a secondary "중단" button instead of send.

## Screens (1440×900 reference; must be fully responsive to the window)

Title bar 40 px, white, bottom border: [≡ menu][← back (editor only)] | title label-m + caption size … right: editor actions … [window controls]. The whole app window is frameless; title bar is the drag region.

### Home (`docs/design/home-grid.png`, `home-list.png`, `home-menu.png`)

Left nav 220 px, gray-50, right border: 최근 항목 / 모든 프로젝트 / 휴지통 … 설정 at bottom. Content: header row (title-l "최근 항목", search 240, grid/list toggle icon buttons, [+ 새 프로젝트] primary L), sub row (sort dropdown "최근 수정 순", caption count), then grid (240 tiles, gap 24/28) or list (header row + 40 px rows). Empty state centered. ≡ opens the menu (새 프로젝트, 폴더에서 열기, 설정, 도움말, 정보). Tile ⋯ menu: 이름 바꾸기, 복제, 폴더에서 보기, 삭제.

### New project dialog 720 (`new-project-uploaded.png`)

Tabs under the title: 원본 이미지로 시작 | 빈 현수막으로 시작. Body two columns: left 320 = dropzone (dashed, upload icon, "현수막 이미지를 여기에 놓으세요", [파일 선택], caption) → after upload a 320×180 preview on gray canvas with file name caption and [바꾸기]; right = 이름 (L field), 실물 가로 / 실물 세로 (L fields with `mm` suffix, always typed by the user — no presets), 미리보기 box (gray-50, proportional rectangle + "6.7 : 1" + "100dpi 기준 23622 × 3543 px"), and in upload mode a caption about ratio mismatch. Footer: 취소 | 만들고 분석 시작 (primary, disabled until image + name + size are valid).

### Editor (`editor-*.png`)

Title bar: [≡][←] | project name + size … [undo][redo] | [기록 ghost][원본 교체 ghost] | [내보내기 primary M] | window controls.
Below: tool rail 56 px (선택 cursor, 이동 hand, 글자 type, 도형 square, 캡처 crop, 비교 compare) · layer panel 240 px (panel header "레이어" + "+", rows) · canvas column · assistant panel 340 px.
Canvas column: context toolbar 40 px (nothing selected: "선택된 레이어 없음" | 재단선·안전 영역 ▾ | 원본 겹쳐 보기 ▾ … ⋯; text layer selected: 트레이싱 글자 ▾ | swatch | 외곽선 6mm ▾ | 그림자 ▾ | X Y W H mm … lock ⋯; agent working: lock icon + "도우미가 작업하는 동안 편집이 잠겨요"), then rulers (20 px, gray-50, mm ticks: minor 100 mm, major 500 mm with micro labels) on top and left, gray canvas with two artboards stacked: 원본 (label above, micro) and 작업본 (label turns blue when something is selected). Selected layer: 1px blue outline, eight 8×8 white handles with blue border, and a blue size label "1602 × 247 mm" under it. Capture tool: dashed blue rectangle with 8% blue fill and a tooltip "캡처를 채팅에 붙였어요". Status bar 24 px gray-50 top border: left micro status, right "6000 × 900 mm · 100dpi 기준 23622 × 3543 px" and zoom [−] 12% [+] [fit]. While the agent works: 2px blue progress line under the toolbar and a 50% white overlay over rail+layers+canvas.
Assistant panel: header 36 px with tabs 도우미 | 기록 and ⋯; body scrolls; composer pinned at the bottom with 12 px padding. 기록 tab lists version rows and a footer [이 버전으로 되돌리기] + caption.

### Dialogs

원본 교체 560: dragover dropzone (blue-50 bg, blue dashed border) + caption "지금 원본(v1)은 지우지 않고 기록에 남겨요"; 취소 | 올리기. After upload the assistant asks a Question (3 options) and the composer is locked until answered.
내보내기 640 (`export-png.png`, `export-svg.png`): left 280 = preview on gray canvas + "파일 크기 600 × 90 mm · 1:10" + folder path; right = 형식 segmented PNG|SVG, 축척 segmented 1:1 실물|1:10 with caption note, PNG: 해상도 select "100dpi · 권장" + caption px size; SVG: checkbox 글자를 윤곽선으로 바꾸기 + caption about raster layers; warning box (red-50, alert icon, "실물 기준 해상도가 모자란 레이어 2개" + items) when `export_check` returns warnings, with footer 고치러 가기 | 그대로 내보내기. Success → toast "바탕화면에 저장했어요".
설정 560: Field L OpenAI API 키 (password, helper), 프로젝트 저장 폴더 + [찾아보기], usage row; 취소 | 저장.

## Interaction notes

- Keyboard: V select, H hand, T text, R rect, C capture, Ctrl+Z/Y, Delete, Esc, Ctrl+E export, Ctrl+N new.
- Zoom: wheel+Ctrl, buttons, "fit". Pan: space+drag or hand tool. Zoom shows percent where 100% = 1 mm per px.
- Dragging a layer moves a cached raster immediately (CSS transform) and commits an `update_layer` op on drop, then re-renders.
- Everything the user edits directly (move, resize, text props, fills, strokes, effects) goes through `applyOps` with actor `user`; the backend records pinned properties and the edit summary.
- One agent turn = one undo step: undo while the agent panel shows a finished turn reverts all of that turn's ops.
