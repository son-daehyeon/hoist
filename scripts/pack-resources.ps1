param(
  [Parameter(Mandatory)][string]$OutDir,
  [Parameter(Mandatory)][string]$ManifestPath,
  [Parameter(Mandatory)][string]$Version,
  [string[]]$Sources = @("src-tauri\fonts", "src-tauri\models")
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Add-Type -AssemblyName System.IO.Compression
New-Item -ItemType Directory -Force $OutDir | Out-Null

$files = [System.Collections.Generic.List[object]]::new()
foreach ($src in $Sources) {
  $full = if ([System.IO.Path]::IsPathRooted($src)) { $src } else { Join-Path $root $src }
  $dir = (Resolve-Path $full).Path.TrimEnd("\")
  $top = Split-Path -Leaf $dir
  foreach ($f in Get-ChildItem $dir -Recurse -File) {
    $files.Add([pscustomobject]@{
        Name   = "$top/" + $f.FullName.Substring($dir.Length + 1).Replace("\", "/")
        Path   = $f.FullName
        Length = $f.Length
      })
  }
}
$files.Sort([Comparison[object]] { param($a, $b) [string]::CompareOrdinal($a.Name, $b.Name) })

$tmp = Join-Path $OutDir "resources.zip.partial"
$stream = [System.IO.File]::Create($tmp)
$archive = [System.IO.Compression.ZipArchive]::new($stream, [System.IO.Compression.ZipArchiveMode]::Create)
$stamp = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
try {
  foreach ($f in $files) {
    $entry = $archive.CreateEntry($f.Name, [System.IO.Compression.CompressionLevel]::Optimal)
    $entry.LastWriteTime = $stamp
    $writer = $entry.Open()
    $reader = [System.IO.File]::OpenRead($f.Path)
    try { $reader.CopyTo($writer) } finally { $reader.Dispose(); $writer.Dispose() }
  }
} finally {
  $archive.Dispose()
  $stream.Dispose()
}

$sha = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLowerInvariant()
$name = "Hoist-Resources-$($sha.Substring(0, 8)).zip"
$final = Join-Path $OutDir $name
Get-ChildItem $OutDir -Filter "Hoist-Resources-*.zip" | Where-Object Name -ne $name | Remove-Item -Force
Move-Item -Force $tmp $final
$manifest = [ordered]@{
  url_name      = $name
  sha256        = $sha
  size          = (Get-Item $final).Length
  unpacked_size = [long]($files | Measure-Object Length -Sum).Sum
  version       = $Version
}
$json = $manifest | ConvertTo-Json
[System.IO.File]::WriteAllText($ManifestPath, $json, (New-Object System.Text.UTF8Encoding $false))
$name
