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

문서: `docs/ARCHITECTURE.md`(구조와 계약), `docs/DESIGN.md`(UI 스펙), `docs/design/*.png`(Figma 렌더).
