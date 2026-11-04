$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "1/3 앱 릴리스 빌드"
pnpm tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "app build failed" }

$exe = Join-Path $root "src-tauri\target\release\banner-studio.exe"
$fonts = Join-Path $root "src-tauri\fonts"
$stage = Join-Path $root "installer\src-tauri\payload\stage"
$zip = Join-Path $root "installer\src-tauri\payload\app.zip"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage "fonts") | Out-Null
Copy-Item $exe $stage
Copy-Item (Join-Path $fonts "*") (Join-Path $stage "fonts")
if (Test-Path $zip) { Remove-Item -Force $zip }
Write-Host "2/3 payload 묶기"
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
Remove-Item -Recurse -Force $stage

Write-Host "3/3 설치 프로그램 빌드"
Set-Location (Join-Path $root "installer")
pnpm tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "installer build failed" }
Set-Location $root
$out = Join-Path $root "dist-installer"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item (Join-Path $root "installer\src-tauri\target\release\banner-studio-setup.exe") (Join-Path $out "Banner Studio Setup.exe") -Force
Write-Host "완료: $out\Banner Studio Setup.exe"
