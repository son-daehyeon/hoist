# TODO

사용자 테스트(2026-11-05, 첫 실제 에이전트 실행)에서 나온 항목. 앱이 실행 중일 때는 손대지 않기로 함.

## 1. 이름: 도우미 → 에이전트

- [x] Rust: `prompt.rs` "당신은 Hoist의 에이전트입니다", 사용자에게 보이는 에러·버전 요약 문구의 "도우미" → "에이전트" (2026-11-05)
- [x] UI 문구 (프론트, 2026-11-05)

- UI 문구 전부(AssistantPanel 탭 "도우미", 빈 상태 안내, 토스트, 설치기 설명 등)와 `src-tauri/src/agent/prompt.rs`의 "당신은 Hoist의 도우미입니다" → "에이전트".
- 코드 식별자(assistant-panel 등)는 그대로.

## 2. 생성 중 애니메이션 + 전반적인 모션

- [x] 모션 토큰·작업 중 표시·다이얼로그/토스트/메뉴/hover/레이어 하이라이트 (프론트, 2026-11-05). Figma DS 기록은 미반영.

- 현재: 에이전트 작업 중 반짝이 아이콘(검은 타일)이 정지 상태라 멈춘 것처럼 보임.
- 작업 중 표시: 아이콘 pulse/회전, 액션 목록의 진행 중 항목에 shimmer, 스트리밍 텍스트 끝에 커서 깜빡임, 캔버스 잠금 오버레이에 흐르는 진행 띠.
- 전반: 다이얼로그 fade+scale(120ms), 토스트 slide-in, 레이어 추가 시 하이라이트 1회, 버튼/행 hover transition, 패널 전환. 전부 `prefers-reduced-motion` 존중.
- 디자인 토큰에 motion 값(duration/easing) 추가하고 Figma DS에도 기록.

## 3. 생각 과정 보여주기 (Claude Code처럼)

- [x] 백엔드: `reasoning.summary = "detailed"`(거부되면 auto로 1회 재시도, 그것도 거부되면 생략), `thinking_start { round }`·`thinking_delta` 이벤트, 액션 `detail`(인자 요약). chat.jsonl에는 저장 안 함 (2026-11-05)
- [x] UI: 접히는 "생각하는 중…" 블록, 액션 옆 detail 표시 (프론트, 2026-11-05)

- Responses API 요청에 `"reasoning": {"effort": ..., "summary": "auto"}` 추가 → 스트림의 `response.reasoning_summary_text.delta`를 `AgentEvent::thinking_delta`로 보내기.
- UI: 액션 목록 위에 접을 수 있는 "생각하는 중…" 블록(스트리밍, 끝나면 접힘). 도구 호출은 인자 요약도 같이("원본 보기 · 왼쪽 위 1/4 영역").
- 채팅 저장(`chat.jsonl`)에는 생각 요약을 넣지 않음(길이·비용).

## 4. 같은 작업 반복(문서 읽기 → 원본 보기 → … 반복)

- [x] 백엔드: 400KB compact 제거 → input_tokens 300K 넘으면 이미지만 강등(최근 4장 유지), 컨텍스트 초과 시 2장으로 줄인 뒤 마지막 수단으로 compact + `compacted` 이벤트, `prompt_cache_key`, 읽기 도구 턴 메모, `analysis/v<n>.json` 캐시, 프롬프트 규칙 8 (2026-11-05)
- [x] UI: 연속된 같은 액션 합치고 "×3", `compacted` 한 줄 표시 (프론트, 2026-11-05)

원인 후보, 코드로 확인한 것:

- `agent/mod.rs`: 히스토리 직렬화 크기가 400KB를 넘으면 `/responses/compact`로 압축. 이미지 출력(view_original 1536px ≈ 수백 KB base64)이 1~2개만 쌓여도 넘어서 **거의 매 라운드 압축** → 압축본에 이미지가 없으니 모델이 다시 읽음.
- 읽기 전용 도구(get_document / view_original / analyze_original)에 캐시가 없어 같은 인자로 또 호출해도 전부 다시 실행.

고칠 것:

- 압축 기준을 바이트가 아니라 "이미지 제외 텍스트 토큰"으로. 이미지는 압축 대신 오래된 것부터 텍스트 placeholder로 교체("(원본 v1 전체 1536px — 이미 봤음)")하고 최근 2~3장만 유지.
- `get_document`: 마지막 읽기 이후 ops가 없으면 "변경 없음 (seq N)"만 반환. `view_original`/`analyze_original`: 같은 인자 재호출이면 "이미 본 이미지(라운드 k)" 짧은 텍스트 반환.
- 시스템 프롬프트에 "이미 본 이미지·문서는 다시 요청하지 말 것" 한 줄.
- UI: 연속된 같은 액션은 하나로 합치고 "×3" 표시.

## 5. 기타

- 제거 프로그램(`uninstall.exe --uninstall`) 실제 실행 미검증.
- 스냅 오버레이(최대화 호버) 실기 확인 필요.
- Cloudflare R2 릴리스 채널: `wrangler login` 후 버킷 `hoist-releases`, 엔드포인트·빌드 스크립트 업로드 단계.

## 6. 두 번째 실사용 테스트 피드백 (2026-11-05)

프론트 (fe-fsd)

- [x] 타임라인: 생각·작업·응답을 **도착 순서대로 한 흐름**에 (지금은 생각이 위, 작업이 아래). 생각 블록은 항상 펼침, 빈 블록은 숨김, 왼쪽 글자 잘림 버그.
- [x] 새 행·블록·메시지 fade-in. 아바타 pulse가 끝에서 툭 끊김 → alternate/ease-in-out로 끊김 없이.
- [x] 버튼·클릭 가능 요소 전부 `cursor: pointer` (중단 버튼 포함).
- [x] 새 프로젝트 이름 placeholder가 "청전동…" → 일반 예시.
- [x] 원본과 현수막 비율이 다르면 새 프로젝트 대화상자에서 **배치 방식 선택** (배경만 늘리기 / 현수막 비율에 맞춰 재배치 / 에이전트가 먼저 묻기).
- [x] 레이어 드래그 지연: 드래그 중엔 로컬 transform만, 놓을 때 한 번만 op.
- [x] 에이전트 패널 키우기·가독성: 기본 폭 440px, 왼쪽 경계 드래그로 360~720px 조절(기억), 본문 15px·줄간격 1.6, 좌우 여백 16px.
- [x] Figma식 이동 스냅: Shift 축 고정, 아트보드·다른 레이어 가장자리·중앙 스마트 스냅(화면 6px, 빨간 가이드), Alt로 스냅 해제, Shift+화살표 10mm, 리사이즈 Shift 비율 유지.

백엔드 (rs-hex)

- [x] add_layer 인자 오류("invalid type: string \"grou…", "missing field x_mm"): 스키마 strict + 관대한 파싱(group 문자열, 배경 기본 좌표) + 틀렸을 때 기대 모양을 돌려주는 에러.
- [x] 생각 요약 한국어 지시, 원본 노이즈·먼지 무시 규칙, fit_mode(배치 방식) 프로젝트 속성과 프롬프트 반영, 액션 이벤트에 round.
- [x] 글자 트레이싱 자글자글: 확대·블러·spline·단순화로 외곽선 매끈하게. 실제 샘플로 before/after 렌더 확인 (`docs/samples/trace-before.png`, `trace-after.png`, 눈 확인 대기).
- [x] 렌더 지연: 레이어별 래스터 캐시(바뀐 레이어만 다시 그림).

## 7. 세 번째 실사용 테스트 피드백 (2026-11-05)

- [x] 도구 호출 병렬 실행(`parallel_tool_calls`, 라운드 안 호출 동시 실행, trace는 락 밖) — 커밋 대기
- [x] 도구 호출 한도 80 → 200 — 커밋 대기
- [x] 생각 내용 숨기고 "생각하는 중 · n초"만 (`ca3e7d5`)
- [x] 새 프로젝트 대화상자의 배치 방식 라디오 제거, 채팅에서만 묻기 (`ca3e7d5`)
- [x] 재배치 선택 시 배치 자유, 정보 위계·가독성 우선 (프롬프트) — 커밋 대기
- [x] 답한 질문 카드 → 한 줄 (`ca3e7d5`)
- [x] 레이어 이름 언더바 → 띄어쓰기 통일 (`tidy_name` + 프롬프트) — 커밋 대기
- [x] 프로젝트별 누적 비용 표시 (`Project.usage_krw`, `usage` 이벤트, 패널 헤더) — 커밋 대기
- [x] 도구 호출 목록 종류별 접기 (`ca3e7d5`)
- [x] 드래그 렉: op마다 썸네일 전체 렌더 제거(1.5초 디바운스), openProject 재조회 제거, PNG는 바뀐 키만 — 커밋 대기
- [ ] 실기 확인: 병렬 실행 로그, 디바운스 썸네일, 드래그 체감 (새 빌드 후)

## 8. 일러스트레이터 대체 (`docs/ROADMAP.md`)

웨이브 1 — 기반

- [ ] 글자 속성: 장평 `scale_x`, 기울임 `skew_deg`, 세로쓰기 `vertical`, 아치 `arc_bend` (rs-hex)
- [ ] 도형: 다각형, 별, 화살표 선 (rs-hex)
- [ ] 그림: 자르기 `crop_px`, 보정 `adjust` (rs-hex)
- [ ] 에이전트 도구 스키마·스킬에 새 속성 반영, "글꼴 있으면 따지 말고 글자 레이어" 규칙 (rs-hex)
- [ ] `layer_svgs` 명령(루트 레이어별 SVG 조각, 키 캐시, 글자 윤곽선화, asset 자리표시) (rs-hex)
- [ ] assetProtocol 활성화 + projects_dir 허용 (rs-hex)
- [ ] `list_fonts` 명령 (rs-hex)
- [ ] 출력 PDF(svg2pdf)·JPG (core-render)
- [ ] 패스 합치기·빼기·교차·제외 `path_boolean` (core-render)
- [ ] 글자 → 윤곽선 `outline_text` (core-render)
- [ ] 그림 가져오기 `import_image` (core-render)
- [ ] 프로젝트 복제 `duplicate_project` (core-render)
- [ ] 번들 글꼴 50종 + `manifest.json` + `LICENSES.md` (fonts)
- [ ] SVG DOM 캔버스(줌 재렌더 없음, 바뀐 조각만 수신) (fe-fsd)
- [ ] 윤곽선 보기 Ctrl+Y + 툴바 버튼 + 앱 메뉴 "보기" (fe-fsd / frontend)
- [ ] DOM 히트 테스트(일러처럼 정확히, 윤곽선 모드는 선 근처) (fe-fsd)
- [ ] 회전 핸들(Shift 15°) (fe-fsd)
- [ ] 다중 선택 이동·크기 조절 (fe-fsd)
- [ ] 다시 하기 단축키 Ctrl+Shift+Z로 (frontend)
- [ ] 속성 패널: 위치·크기·회전·불투명도 / 글자 / 채우기 / 획 여러 개 / 효과 / 그림 / 도형 (frontend)
- [ ] 색 선택기 + 최근 색 + 프로젝트 팔레트 + 스포이트 + 그라디언트 편집기 (frontend)

웨이브 2 — 편집 도구

- [ ] 펜 도구: 새로 그리기, 점·핸들 편집, 점 추가/삭제, 닫기, 모서리↔곡선
- [ ] 도형 도구 5종(사각·둥근 사각·원·선/화살표·다각형/별)
- [ ] 정렬·균등 분배 툴바
- [ ] 복제(Alt 드래그, Ctrl+D), 복사·붙여넣기, 순서 단축키(Ctrl+[ ])
- [ ] 더블클릭 인라인 글자 편집(글꼴 @font-face 미리보기)
- [ ] 글꼴 고르기 드롭다운(미리보기, 50종)
- [ ] 그림 넣기(파일·드래그 앤 드롭), 자르기 핸들
- [ ] 출력 대화상자에 PDF·JPG 선택지
- [ ] 패스 연산·글자 윤곽선화·프로젝트 복제 메뉴 연결
- [ ] 단축키 도움말 갱신, DESIGN.md·Figma DS 반영

웨이브 3 — 그림·마무리

- [ ] 누끼(로컬 모델)
- [ ] 템플릿: 복제 + "문구만 바꿔줘"
- [ ] 성능 측정(dev 앱 CDP): 드래그·회전 60fps, 편집 반영 50ms, 레이어 100개
- [ ] 새 설치본 빌드·재설치·실사용 확인

## 9. 운영

- [ ] Cloudflare R2 릴리스 채널(`wrangler login` 대기) → 엔드포인트·업로드 스크립트 → 첫 릴리스로 "새 버전" 토스트 확인
- [ ] 제거 프로그램 실행 확인, 스냅 오버레이 실기 확인
- [ ] 리포 폴더 `~/repos/hoist`로, 서명 키 이름, GitHub private 리포 push
