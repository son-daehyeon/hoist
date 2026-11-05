# 설치본(com.hoist.app)이 실행 중이어도 같이 띄울 수 있는 dev 앱 + CDP 포트.
# single-instance 플러그인이 identifier로 잠그므로 dev는 com.hoist.dev로 띄운다
# (앱 데이터 폴더만 갈라지고, 프로젝트 폴더와 키링은 공유).
# 사용: pwsh scripts/dev-cdp.ps1  →  ~/hoist-e2e/drive.mjs 로 조작 (CDP_PORT 기본 9222)
param([int]$Port = 9222)
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Set-Location (Join-Path $PSScriptRoot "..")
# --no-watch: 다른 작업자가 Rust 파일을 고치는 동안 앱이 재시작되지 않게
pnpm tauri dev --no-watch --config '{"identifier":"com.hoist.dev"}'
