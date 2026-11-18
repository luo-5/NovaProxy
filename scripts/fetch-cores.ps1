# fetch-cores.ps1 - Cross-platform core downloader for NovaProxy (Windows, Linux, macOS, Android)
[CmdletBinding()]
param(
    [ValidateSet("windows", "linux", "macos", "android")]
    [string]$Platform = "windows",
    [ValidateSet("amd64", "arm64")]
    [string]$Arch = "amd64",
    [string]$TargetDir,
    [string]$LockFile
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $scriptDir) { $scriptDir = $PSScriptRoot }
$projectRoot = (Get-Item -Path $scriptDir).Parent.FullName

if (-not $TargetDir) {
    if ($Platform -eq "android") {
        $abi = if ($Arch -eq "arm64") { "arm64-v8a" } else { "x86_64" }
        $TargetDir = Join-Path $projectRoot "src-tauri\gen\android\app\src\main\jniLibs\$abi"
    } else {
        $TargetDir = Join-Path $projectRoot "src-tauri\resources\cores"
    }
}
if (-not $LockFile) {
    $LockFile = Join-Path $projectRoot "cores.lock.json"
}

if (-not (Test-Path -Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
}

$TempDir = Join-Path $env:TEMP "novaproxy_cores_$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

$lockData = @{}
if (($Platform -eq "windows") -and ($Arch -eq "amd64") -and (Test-Path -Path $LockFile)) {
    try {
        $lockData = Get-Content -Path $LockFile -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    } catch {
        $lockData = @{}
    }
}

function Get-LatestGitHubRelease {
    param([string]$Repo)
    $apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    $headers = @{ "User-Agent" = "NovaProxy-CoreFetcher" }
    if ($env:GITHUB_TOKEN) {
        $headers["Authorization"] = "Bearer $($env:GITHUB_TOKEN)"
    }
    $resp = Invoke-RestMethod -Uri $apiUrl -Headers $headers -TimeoutSec 20
    return $resp
}

function Expand-GzipFile {
    param([string]$SourceGz, [string]$DestFile)
    $inStream = [System.IO.File]::OpenRead($SourceGz)
    try {
        $gzStream = New-Object System.IO.Compression.GzipStream($inStream, [System.IO.Compression.CompressionMode]::Decompress)
        try {
            $outStream = [System.IO.File]::Create($DestFile)
            try {
                $gzStream.CopyTo($outStream)
            } finally {
                $outStream.Dispose()
            }
        } finally {
            $gzStream.Dispose()
        }
    } finally {
        $inStream.Dispose()
    }
}

function Get-OutputBinaryName {
    param([string]$BaseName)
    if ($Platform -eq "windows") {
        return "$BaseName.exe"
    }
    if ($Platform -eq "android") {
        switch ($BaseName) {
            "sing-box" { return "libsingbox.so" }
            "mihomo"   { return "libmihomo.so" }
            "xray"     { return "libxray.so" }
        }
    }
    return $BaseName
}

Write-Host "=== Fetching NovaProxy Core Binaries ($Platform/$Arch -> $TargetDir) ===" -ForegroundColor Cyan

# 1. sing-box
Write-Host "[1/3] Processing sing-box..." -ForegroundColor Yellow
$singboxRelease = Get-LatestGitHubRelease -Repo "SagerNet/sing-box"
$sbOs = if ($Platform -eq "macos") { "darwin" } else { $Platform }
$sbExt = if ($Platform -eq "windows") { "zip" } else { "tar\.gz" }
$sbPattern = "^sing-box-.*-$sbOs-$Arch\.$sbExt$"
$sbAsset = $singboxRelease.assets | Where-Object { $_.name -match $sbPattern } | Select-Object -First 1
if (-not $sbAsset) {
    throw "No sing-box asset matched $sbPattern"
}
$sbArchive = Join-Path $TempDir $sbAsset.name
Write-Host "Downloading $($sbAsset.name)..."
Invoke-WebRequest -Uri $sbAsset.browser_download_url -OutFile $sbArchive
$sbExtract = Join-Path $TempDir "sb_extract"
New-Item -ItemType Directory -Path $sbExtract -Force | Out-Null
if ($sbAsset.name.EndsWith(".zip")) {
    Expand-Archive -Path $sbArchive -DestinationPath $sbExtract -Force
} else {
    tar -xzf $sbArchive -C $sbExtract
}
$sbSrcName = if ($Platform -eq "windows") { "sing-box.exe" } else { "sing-box" }
$sbBin = Get-ChildItem -Path $sbExtract -Filter $sbSrcName -Recurse | Select-Object -First 1
if (-not $sbBin) {
    throw "Executable $sbSrcName not found in $($sbAsset.name)"
}
$sbDest = Join-Path $TargetDir (Get-OutputBinaryName "sing-box")
Copy-Item -Path $sbBin.FullName -Destination $sbDest -Force
Write-Host "  -> Saved $sbDest ($($singboxRelease.tag_name))" -ForegroundColor Green
if (($Platform -eq "windows") -and ($Arch -eq "amd64")) {
    $lockData["sing-box"] = @{
        version = $singboxRelease.tag_name
        sha256  = (Get-FileHash -Path $sbDest -Algorithm SHA256).Hash
        url     = $sbAsset.browser_download_url
    }
}

# 2. mihomo
Write-Host "[2/3] Processing mihomo..." -ForegroundColor Yellow
$mihomoRelease = Get-LatestGitHubRelease -Repo "MetaCubeX/mihomo"
$miPatterns = switch ("$Platform`:$Arch") {
    "windows:amd64" { @("^mihomo-windows-amd64-compatible-v.*\.zip$", "^mihomo-windows-amd64-v.*\.zip$") }
    "windows:arm64" { @("^mihomo-windows-arm64-v.*\.zip$") }
    "macos:arm64"   { @("^mihomo-darwin-arm64-v.*\.gz$") }
    "macos:amd64"   { @("^mihomo-darwin-amd64-compatible-v.*\.gz$", "^mihomo-darwin-amd64-v1\.1.*\.gz$") }
    "linux:amd64"   { @("^mihomo-linux-amd64-compatible-v.*\.gz$", "^mihomo-linux-amd64-v1\.1.*\.gz$") }
    "linux:arm64"   { @("^mihomo-linux-arm64-v.*\.gz$") }
    "android:arm64" { @("^mihomo-android-arm64-v8-v.*\.gz$") }
    "android:amd64" { @("^mihomo-android-amd64-v.*\.gz$") }
}
$miAsset = $null
foreach ($pat in $miPatterns) {
    $miAsset = $mihomoRelease.assets | Where-Object { $_.name -match $pat } | Select-Object -First 1
    if ($miAsset) { break }
}
if (-not $miAsset) {
    throw "No mihomo asset matched $Platform/$Arch"
}
$miArchive = Join-Path $TempDir $miAsset.name
Write-Host "Downloading $($miAsset.name)..."
Invoke-WebRequest -Uri $miAsset.browser_download_url -OutFile $miArchive
$miDest = Join-Path $TargetDir (Get-OutputBinaryName "mihomo")
if ($miAsset.name.EndsWith(".zip")) {
    $miExtract = Join-Path $TempDir "mi_extract"
    New-Item -ItemType Directory -Path $miExtract -Force | Out-Null
    Expand-Archive -Path $miArchive -DestinationPath $miExtract -Force
    $miBin = Get-ChildItem -Path $miExtract -Filter "*.exe" -Recurse | Select-Object -First 1
    if (-not $miBin) { throw "mihomo executable missing in $($miAsset.name)" }
    Copy-Item -Path $miBin.FullName -Destination $miDest -Force
} else {
    Expand-GzipFile -SourceGz $miArchive -DestFile $miDest
}
Write-Host "  -> Saved $miDest ($($mihomoRelease.tag_name))" -ForegroundColor Green
if (($Platform -eq "windows") -and ($Arch -eq "amd64")) {
    $lockData["mihomo"] = @{
        version = $mihomoRelease.tag_name
        sha256  = (Get-FileHash -Path $miDest -Algorithm SHA256).Hash
        url     = $miAsset.browser_download_url
    }
}

# 3. Xray-core
Write-Host "[3/3] Processing Xray-core..." -ForegroundColor Yellow
$xrayRelease = Get-LatestGitHubRelease -Repo "XTLS/Xray-core"
$xrExact = switch ("$Platform`:$Arch") {
    "windows:amd64" { "Xray-windows-64.zip" }
    "windows:arm64" { "Xray-windows-arm64-v8a.zip" }
    "macos:arm64"   { "Xray-macos-arm64-v8a.zip" }
    "macos:amd64"   { "Xray-macos-64.zip" }
    "linux:amd64"   { "Xray-linux-64.zip" }
    "linux:arm64"   { "Xray-linux-arm64-v8a.zip" }
    "android:arm64" { "Xray-android-arm64-v8a.zip" }
    "android:amd64" { "Xray-android-amd64.zip" }
}
$xrAsset = $xrayRelease.assets | Where-Object { $_.name -eq $xrExact } | Select-Object -First 1
if (-not $xrAsset) {
    throw "No Xray asset matched $xrExact"
}
$xrArchive = Join-Path $TempDir $xrAsset.name
Write-Host "Downloading $($xrAsset.name)..."
Invoke-WebRequest -Uri $xrAsset.browser_download_url -OutFile $xrArchive
$xrExtract = Join-Path $TempDir "xr_extract"
New-Item -ItemType Directory -Path $xrExtract -Force | Out-Null
Expand-Archive -Path $xrArchive -DestinationPath $xrExtract -Force
$xrSrcName = if ($Platform -eq "windows") { "xray.exe" } else { "xray" }
$xrBin = Get-ChildItem -Path $xrExtract -Filter $xrSrcName -Recurse | Select-Object -First 1
if (-not $xrBin) {
    throw "Executable $xrSrcName not found in $($xrAsset.name)"
}
$xrDest = Join-Path $TargetDir (Get-OutputBinaryName "xray")
Copy-Item -Path $xrBin.FullName -Destination $xrDest -Force
Write-Host "  -> Saved $xrDest ($($xrayRelease.tag_name))" -ForegroundColor Green
if (($Platform -eq "windows") -and ($Arch -eq "amd64")) {
    $lockData["xray"] = @{
        version = $xrayRelease.tag_name
        sha256  = (Get-FileHash -Path $xrDest -Algorithm SHA256).Hash
        url     = $xrAsset.browser_download_url
    }
}

if (($Platform -eq "windows") -and ($Arch -eq "amd64")) {
    $lockJson = $lockData | ConvertTo-Json -Depth 4
    Set-Content -Path $LockFile -Value $lockJson -Encoding UTF8
    Write-Host "Updated cores lockfile at $LockFile" -ForegroundColor Cyan
}

Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "Finished fetching cores cleanly." -ForegroundColor Green
