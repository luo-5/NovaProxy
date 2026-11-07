# fetch-cores.ps1 - Download sing-box, mihomo, Xray-core for Windows x64
[CmdletBinding()]
param(
    [string]$TargetDir,
    [string]$LockFile
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $scriptDir) { $scriptDir = $PSScriptRoot }
$projectRoot = (Get-Item -Path $scriptDir).Parent.FullName

if (-not $TargetDir) {
    $TargetDir = Join-Path $projectRoot "src-tauri\resources\cores"
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
if (Test-Path -Path $LockFile) {
    try {
        $lockData = Get-Content -Path $LockFile -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    } catch {
        $lockData = @{}
    }
}

function Get-LatestGitHubRelease {
    param([string]$Repo)
    $apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $headers = @{ "User-Agent" = "NovaProxy-CoreFetcher" }
        $resp = Invoke-RestMethod -Uri $apiUrl -Headers $headers -TimeoutSec 15
        return $resp
    } catch {
        Write-Warning "Failed to query GitHub API for ${Repo}: $_"
        return $null
    }
}

Write-Host "=== Fetching NovaProxy Core Binaries (sing-box, mihomo, Xray) ===" -ForegroundColor Cyan

# 1. sing-box
try {
    Write-Host "[1/3] Processing sing-box..." -ForegroundColor Yellow
    $singboxRelease = Get-LatestGitHubRelease -Repo "SagerNet/sing-box"
    if ($singboxRelease) {
        $version = $singboxRelease.tag_name.TrimStart('v')
        $asset = $singboxRelease.assets | Where-Object { $_.name -match "sing-box-.*-windows-amd64\.zip$" } | Select-Object -First 1
        if ($asset) {
            $zipPath = Join-Path $TempDir $asset.name
            Write-Host "Downloading sing-box $version from $($asset.browser_download_url)..."
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath
            
            $extractDir = Join-Path $TempDir "sing-box-extract"
            Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
            $exe = Get-ChildItem -Path $extractDir -Filter "sing-box.exe" -Recurse | Select-Object -First 1
            if ($exe) {
                Copy-Item -Path $exe.FullName -Destination (Join-Path $TargetDir "sing-box.exe") -Force
                $hash = (Get-FileHash -Path (Join-Path $TargetDir "sing-box.exe") -Algorithm SHA256).Hash
                $lockData["sing-box"] = @{
                    version = $singboxRelease.tag_name
                    sha256 = $hash
                    url = $asset.browser_download_url
                }
                Write-Host "  -> sing-box saved successfully ($version)" -ForegroundColor Green
            }
        }
    }
} catch {
    Write-Error "Failed to download sing-box: $_"
}

# 2. mihomo (Clash Meta)
try {
    Write-Host "[2/3] Processing mihomo..." -ForegroundColor Yellow
    $mihomoRelease = Get-LatestGitHubRelease -Repo "MetaCubeX/mihomo"
    if ($mihomoRelease) {
        $version = $mihomoRelease.tag_name
        $asset = $mihomoRelease.assets | Where-Object { $_.name -match "mihomo-windows-amd64-(compatible-)?v.*\.zip$" } | Select-Object -First 1
        if (-not $asset) {
            $asset = $mihomoRelease.assets | Where-Object { $_.name -match "mihomo-windows-amd64-.*\.zip$" } | Select-Object -First 1
        }
        if ($asset) {
            $zipPath = Join-Path $TempDir $asset.name
            Write-Host "Downloading mihomo $version from $($asset.browser_download_url)..."
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath
            
            $extractDir = Join-Path $TempDir "mihomo-extract"
            Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
            $exe = Get-ChildItem -Path $extractDir -Filter "*.exe" -Recurse | Select-Object -First 1
            if ($exe) {
                Copy-Item -Path $exe.FullName -Destination (Join-Path $TargetDir "mihomo.exe") -Force
                $hash = (Get-FileHash -Path (Join-Path $TargetDir "mihomo.exe") -Algorithm SHA256).Hash
                $lockData["mihomo"] = @{
                    version = $mihomoRelease.tag_name
                    sha256 = $hash
                    url = $asset.browser_download_url
                }
                Write-Host "  -> mihomo saved successfully ($version)" -ForegroundColor Green
            }
        }
    }
} catch {
    Write-Error "Failed to download mihomo: $_"
}

# 3. Xray-core
try {
    Write-Host "[3/3] Processing Xray-core..." -ForegroundColor Yellow
    $xrayRelease = Get-LatestGitHubRelease -Repo "XTLS/Xray-core"
    if ($xrayRelease) {
        $version = $xrayRelease.tag_name
        $asset = $xrayRelease.assets | Where-Object { $_.name -eq "Xray-windows-64.zip" } | Select-Object -First 1
        if ($asset) {
            $zipPath = Join-Path $TempDir $asset.name
            Write-Host "Downloading Xray-core $version from $($asset.browser_download_url)..."
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath
            
            $extractDir = Join-Path $TempDir "xray-extract"
            Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
            $exe = Get-ChildItem -Path $extractDir -Filter "xray.exe" -Recurse | Select-Object -First 1
            if ($exe) {
                Copy-Item -Path $exe.FullName -Destination (Join-Path $TargetDir "xray.exe") -Force
                $hash = (Get-FileHash -Path (Join-Path $TargetDir "xray.exe") -Algorithm SHA256).Hash
                $lockData["xray"] = @{
                    version = $xrayRelease.tag_name
                    sha256 = $hash
                    url = $asset.browser_download_url
                }
                Write-Host "  -> xray saved successfully ($version)" -ForegroundColor Green
            }
        }
    }
} catch {
    Write-Error "Failed to download Xray: $_"
}

# Write lock file
$lockJson = $lockData | ConvertTo-Json -Depth 4
Set-Content -Path $LockFile -Value $lockJson -Encoding UTF8
Write-Host "Updated cores lockfile at $LockFile" -ForegroundColor Cyan

# Cleanup
Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "Finished fetching cores." -ForegroundColor Green
