$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root "src-tauri\models"
New-Item -ItemType Directory -Force $dest | Out-Null

function Get-Verified([string]$Url, [string]$Path, [string]$Sha256) {
  if ((Test-Path $Path) -and (Get-FileHash $Path -Algorithm SHA256).Hash -eq $Sha256) {
    Write-Host "  이미 있음: $(Split-Path -Leaf $Path)"
    return
  }
  Write-Host "  받는 중: $Url"
  Invoke-WebRequest $Url -OutFile $Path
  $actual = (Get-FileHash $Path -Algorithm SHA256).Hash
  if ($actual -ne $Sha256) {
    Remove-Item -Force $Path
    throw "SHA-256 불일치: $Url (받은 값 $actual)"
  }
}

Write-Host "1/3 누끼 모델 IS-Net general-use (Apache-2.0)"
Get-Verified "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx" `
  (Join-Path $dest "isnet-general-use.onnx") `
  "60920E99C45464F2BA57BEE2AD08C919A52BBF852739E96947FBB4358C0D964A"
Invoke-WebRequest "https://raw.githubusercontent.com/xuebinqin/DIS/main/LICENSE.md" -OutFile (Join-Path $dest "LICENSE-isnet.txt")

$ortVersion = "1.31.1"
Write-Host "2/3 ONNX Runtime $ortVersion CPU (MIT)"
$zip = Join-Path $env:TEMP "onnxruntime-win-x64-$ortVersion.zip"
Get-Verified "https://github.com/microsoft/onnxruntime/releases/download/v$ortVersion/onnxruntime-win-x64-$ortVersion.zip" `
  $zip "5869E26BB25314375F2D5EF1877E3EA06B1543D6C710E6E4A1EC2F7177AB4B2C"
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($zip)
try {
  foreach ($pair in @(
      @("lib/onnxruntime.dll", "onnxruntime.dll"),
      @("LICENSE", "LICENSE-onnxruntime.txt"),
      @("ThirdPartyNotices.txt", "ThirdPartyNotices-onnxruntime.txt"))) {
    $entry = $archive.GetEntry("onnxruntime-win-x64-$ortVersion/$($pair[0])")
    [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $dest $pair[1]), $true)
  }
} finally {
  $archive.Dispose()
}

Write-Host "3/3 출처 기록"
@"
isnet-general-use.onnx
  IS-Net (Highly Accurate Dichotomous Image Segmentation), general-use weights by Xuebin Qin et al.
  https://github.com/xuebinqin/DIS  License: Apache-2.0 (LICENSE-isnet.txt)
  ONNX export published by rembg: https://github.com/danielgatis/rembg/releases/tag/v0.0.0

onnxruntime.dll
  ONNX Runtime $ortVersion (CPU), Microsoft. https://github.com/microsoft/onnxruntime
  License: MIT (LICENSE-onnxruntime.txt, ThirdPartyNotices-onnxruntime.txt)
"@ | Set-Content -Encoding utf8 (Join-Path $dest "NOTICE.txt")

Write-Host "완료: $dest"
