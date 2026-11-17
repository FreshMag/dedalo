# Installs the latest dedalo binary for Windows into %LOCALAPPDATA%\dedalo.
#
#     irm https://github.com/FreshMag/dedalo/releases/latest/download/install.ps1 | iex
#
# $env:DEDALO_REPO (owner/name) changes the repository.
$ErrorActionPreference = "Stop"
$repo = if ($env:DEDALO_REPO) { $env:DEDALO_REPO } else { "FreshMag/dedalo" }
$target = "x86_64-pc-windows-msvc"
$base = "https://github.com/$repo/releases/latest/download"
$dir = Join-Path $env:LOCALAPPDATA "dedalo"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("dedalo-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $tmp | Out-Null
try {
    Invoke-WebRequest "$base/dedalo-$target.zip" -OutFile "$tmp\dedalo.zip"
    Invoke-WebRequest "$base/SHA256SUMS" -OutFile "$tmp\SHA256SUMS"
    $expected = (Select-String -Path "$tmp\SHA256SUMS" -Pattern " dedalo-$target.zip$").Line.Split(" ")[0]
    $actual = (Get-FileHash "$tmp\dedalo.zip" -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) { throw "checksum mismatch; not installing" }
    Expand-Archive "$tmp\dedalo.zip" -DestinationPath $tmp
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item "$tmp\dedalo-$target\dedalo.exe" $dir -Force
    Write-Host "installed to $dir; add it to PATH to run dedalo from anywhere"
} finally {
    Remove-Item -Recurse -Force $tmp
}
