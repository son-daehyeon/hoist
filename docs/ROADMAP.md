# ROADMAP — 일러스트레이터 대체

2026-11-05 결정. 아버지가 현수막 작업에 일러스트레이터로 하던 일을 Hoist 안에서 전부 할 수 있게 한다. 무거워져도 된다. 특히 자주 쓰는 것: **보기 → 윤곽선 보기(Ctrl+Y)**.

## 아키텍처 결정: 화면은 SVG DOM, 출력은 resvg

- 지금: 백엔드가 레이어별 PNG를 만들어 `<img>`로 보여줌. 줌할 때마다 전부 다시 그리고, 매 편집마다 base64를 실어 나른다.
- 바꿈: 백엔드(`hoist-core`)가 레이어별 **SVG 조각**(`<g>` 하나, mm 단위, 글자는 cosmic-text로 윤곽선화)을 내려주고, 프론트는 `<svg viewBox>` 하나에 조각을 꽂아 보여준다. 줌·팬은 transform뿐이라 다시 그릴 게 없다. 조각 키는 레이어 내용 해시라 바뀐 레이어만 다시 받는다.
- 출력·비교·분석은 지금처럼 같은 SVG를 resvg로 래스터화한다. 화면과 출력이 같은 SVG에서 나오므로 어긋나지 않는다.
- 윤곽선 보기 = CSS 한 줄(`fill:none; stroke:#000; vector-effect:non-scaling-stroke`). 그림은 대각선 박스.
- 히트 테스트는 DOM(`elementFromPoint`, `isPointInFill`)로 일러처럼 정확히. 윤곽선 모드에서는 선 근처 클릭으로 선택.

## 웨이브

### 0. 3차 피드백 (진행 중)

병렬 도구 실행, 생각 요약 제거, 액션 묶기, 질문 접기, 비용 표시, 썸네일 디바운스, 드래그 렉.

### 1. 기반 (Rust: rs-hex / core-render, 폰트: fonts)

- 모델: 글자 `scale_x`(장평)·`skew_deg`·`vertical`·`arc_bend`, 도형 `polygon{sides}`·`star{points, inner_ratio}`·`line{arrow_start, arrow_end}`, 그림 `crop_px`·`adjust{brightness, contrast, saturation}`. 기존 Paint(단색·선형 그라디언트)·Stroke 여러 개·DropShadow·LayerBlur는 그대로.
- `layer_svgs(id, have)` 명령: 루트 레이어별 `{id, key, svg, bbox_mm}`. 그림은 `asset:이름` 자리표시 → 프론트가 `convertFileSrc`.
- `list_fonts`: family·weight·파일 경로 (프론트 글꼴 미리보기·인라인 편집용 @font-face).
- 에이전트 도구 스키마·스킬에 새 속성 반영 (글꼴이 있으면 따지 말고 글자 레이어 + 장평·자간으로).
- 출력: PDF(svg2pdf, 글자 윤곽선), JPG. 패스 합치기·빼기·교차(`path_boolean`), 글자→윤곽선(`outline_text`), 그림 가져오기(`import_image`), 프로젝트 복제.
- 번들 글꼴 50종 + 라이선스 목록 (`scripts/fetch-fonts.ps1`, `src-tauri/fonts/manifest.json`).

### 2. 편집 도구 (프론트, 캔버스가 먼저)

- 캔버스(fe-fsd): SVG DOM 캔버스, 윤곽선 보기(Ctrl+Y), 정확한 히트 테스트, 회전 핸들, 다중 선택 변형, 스냅 유지.
- 속성 패널(frontend): 글자(글꼴 미리보기·크기·굵기·자간·행간·장평·기울임·세로·정렬·아치), 채우기(색·그라디언트), 획 여러 개, 효과(그림자·흐림), 위치·크기·회전, 그림(자르기·보정). 색 선택기·스포이트·팔레트.
- 펜 도구: 새로 그리기, 점·핸들 편집, 점 추가/삭제, 닫기, 모서리↔곡선.
- 선택·배치: 정렬·균등 분배, 복제(Alt 드래그, Ctrl+D), 복사·붙여넣기, 순서 단축키, 도형 도구 5종, 인라인 글자 편집(더블클릭).

### 3. 그림·마무리

- 누끼(로컬 모델, 무거워도 됨), 템플릿(복제 + "문구만 바꿔줘"), 단축키 도움말, DESIGN.md·Figma 반영.

## 성능 예산

- 드래그·회전 60fps, 편집 → 화면 반영 50ms 안, 줌은 재렌더 없음, 레이어 100개 문서에서 유지.
- 확인은 dev 앱을 CDP로 띄워 pointer 이벤트를 쏘고 프레임 시간을 잰다 (`~/hoist-e2e/drive.mjs`).
