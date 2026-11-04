# Hoist

작은 현수막 이미지를 실물 크기로 출력할 수 있는 레이어 문서로 다시 만드는 Windows 데스크톱 앱. Tauri 2 + Rust + React.

## 개발

```
pnpm install
pnpm tauri dev        # 앱 실행 (Rust 첫 빌드는 몇 분 걸림)
pnpm dev              # UI만 브라우저에서 (mock 백엔드)
cargo test -p hoist-core
```

설정 → OpenAI API 키를 넣어야 도우미가 동작한다. 프로젝트는 `문서\Hoist\<id>\` 폴더 하나에 전부 저장된다.

## 프론트 구조

`src/`는 Feature-Sliced Design을 따른다. 레이어는 `app → pages → widgets → features → entities → shared` 순이고, 아래 레이어만 import할 수 있다. 같은 레이어의 다른 슬라이스는 import하지 않는다. 슬라이스 밖에서는 `@/widgets/canvas`처럼 그 슬라이스의 `index.ts`로만 접근한다. Tauri 패키지(`@tauri-apps/*`)는 `src/shared/api`에서만 import한다. `pnpm lint`가 이 규칙들을 검사한다. 디자인 시스템은 `src/shared/ui`에 있고, 설치 프로그램(`installer/`)도 같이 쓴다.

## 백엔드 구조

Rust는 Tauri 공식 문서의 구조를 따른다. `crates/core`(hoist-core)는 문서 모델·렌더·내보내기·프로젝트 저장을 맡는 flat 모듈 라이브러리이고, Tauri와 네트워크를 모른다. `src-tauri/src`에는 `commands/`(영역별 커맨드), `agent/`(도우미 턴 루프와 도구), `openai.rs`, `settings.rs`, `state.rs`, `error.rs`가 있다. 프론트가 쓰는 타입과 커맨드는 tauri-specta가 `src/shared/api/bindings.ts`로 만든다. 커맨드나 타입을 바꿨다면 `cargo test -p hoist export_bindings`로 다시 생성해서 같이 커밋한다.

배포는 `scripts/build-installer.ps1`이 설치 파일·서명·`latest.json`을 만들고, 둘을 GitHub Release에 올리면 설치된 앱이 시작할 때 새 버전을 알린다.

문서: `docs/ARCHITECTURE.md`(구조와 계약), `docs/DESIGN.md`(UI 스펙), `docs/design/*.png`(Figma 렌더).
