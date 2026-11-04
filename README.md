# Banner Studio

작은 현수막 이미지를 실물 크기로 출력할 수 있는 레이어 문서로 다시 만드는 Windows 데스크톱 앱. Tauri 2 + Rust + React.

## 개발

```
pnpm install
pnpm tauri dev        # 앱 실행 (Rust 첫 빌드는 몇 분 걸림)
pnpm dev              # UI만 브라우저에서 (mock 백엔드)
cargo test -p banner-core
```

설정 → OpenAI API 키를 넣어야 도우미가 동작한다. 프로젝트는 `문서\BannerStudio\<id>\` 폴더 하나에 전부 저장된다.

## 프론트 구조

`src/`는 Feature-Sliced Design을 따른다. 레이어는 `app → pages → widgets → features → entities → shared` 순이고, 아래 레이어만 import할 수 있다. 같은 레이어의 다른 슬라이스는 import하지 않는다. 슬라이스 밖에서는 `@/widgets/canvas`처럼 그 슬라이스의 `index.ts`로만 접근한다. `pnpm lint`가 이 규칙을 검사한다. 디자인 시스템은 `src/shared/ui`에 있고, 설치 프로그램(`installer/`)도 같이 쓴다.

## 백엔드 구조

Rust는 포트·어댑터 구조다. `crates/core`(banner-core)는 도메인(`domain/`)과 렌더·래스터·내보내기·프로젝트 저장 서비스(`render/`, `raster/`, `export/`, `project/`)이고, Tauri와 네트워크를 모른다. `src-tauri/src`는 유스케이스와 에이전트 루프(`application/`), LLM·이벤트·API 키 trait(`ports/`), OpenAI·keyring·Tauri 구현(`adapters/`)으로 나뉜다. 의존은 `adapters → application → ports`와 `application → banner-core` 방향만 있고, 어떤 구현을 쓸지는 `src-tauri/src/lib.rs`에서만 정한다.

문서: `docs/ARCHITECTURE.md`(구조와 계약), `docs/DESIGN.md`(UI 스펙), `docs/design/*.png`(Figma 렌더).
