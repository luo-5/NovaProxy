# build.ps1 - Build NovaProxy client
[CmdletBinding()]
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $scriptDir) { $scriptDir = $PSScriptRoot }
$projectRoot = (Get-Item -Path $scriptDir).Parent.FullName

Set-Location $projectRoot

$mingwBin = "C:\Users\31752\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"

$env:PATH = "$mingwBin;$cargoBin;$env:PATH"
$env:CARGO_TARGET_DIR = "$env:USERPROFILE\.cargo\target_novaproxy"

Write-Host "=== Building NovaProxy Frontend ===" -ForegroundColor Cyan
pnpm build

Write-Host "=== Building NovaProxy Desktop Application ===" -ForegroundColor Cyan
if ($Release) {
    pnpm tauri build
} else {
    pnpm tauri build --debug
}

Write-Host "=== Build Completed ===" -ForegroundColor Green
