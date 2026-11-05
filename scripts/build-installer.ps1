param([string]$Notes = "")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
$signingKey = Join-Path $env:USERPROFILE ".tauri\hoist.key"
if (-not (Test-Path $signingKey)) { throw "signing key not found: $signingKey" }

Write-Host "1/5 앱 릴리스 빌드 (v$version)"
pnpm tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "app build failed" }

$exe = Join-Path $root "target\release\hoist.exe"
$payload = Join-Path $root "installer\src-tauri\payload"
$stage = Join-Path $payload "stage"
$zip = Join-Path $payload "app.zip"
$out = Join-Path $root "dist-installer"
$models = Join-Path $root "src-tauri\models"
if (-not (Test-Path (Join-Path $models "isnet-general-use.onnx"))) { throw "누끼 모델이 없어요: scripts\fetch-models.ps1 을 먼저 실행하세요" }
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe $stage
if (Test-Path $zip) { Remove-Item -Force $zip }
Write-Host "2/5 payload 묶기 (앱은 설치기 안에, 글꼴·모델은 따로)"
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
Remove-Item -Recurse -Force $stage
$resources = & (Join-Path $PSScriptRoot "pack-resources.ps1") -OutDir $out -ManifestPath (Join-Path $payload "resources.json") -Version $version

Write-Host "3/5 설치 프로그램 빌드"
Set-Location (Join-Path $root "installer")
pnpm tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "installer build failed" }
Set-Location $root
$setupName = "Hoist-Setup.exe"
$setup = Join-Path $out $setupName
Copy-Item (Join-Path $root "target\release\hoist-setup.exe") $setup -Force

Write-Host "4/5 업데이트 서명"
pnpm exec tauri signer sign --private-key-path $signingKey --password= --app-version $version $setup
if ($LASTEXITCODE -ne 0) { throw "signing failed" }
$signature = (Get-Content "$setup.sig" -Raw).Trim()

Write-Host "5/5 latest.json"
$manifest = [ordered]@{
  version   = $version
  notes     = $Notes
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      url       = "https://github.com/son-daehyeon/hoist-releases/releases/download/v$version/$setupName"
      signature = $signature
    }
  }
}
$json = $manifest | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText((Join-Path $out "latest.json"), $json, (New-Object System.Text.UTF8Encoding $false))

Write-Host "완료: $setup, $setup.sig, $out\latest.json, $out\$resources"
Write-Host "GitHub Release v$version 에 $setupName, latest.json, $resources 를 올리세요. 글꼴·모델 zip은 바뀌지 않아도 매 릴리스에 올려요."
