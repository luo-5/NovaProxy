# build-android.ps1 - Cross-compile Android ARM64/x86_64 Rust targets & build universal APK on Windows
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$SkipCoreDownload
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $scriptDir) { $scriptDir = $PSScriptRoot }
$projectRoot = (Get-Item -Path $scriptDir).Parent.FullName
Set-Location $projectRoot

if (-not $env:JAVA_HOME) { $env:JAVA_HOME = Join-Path $env:USERPROFILE "android-dev\jdk-17.0.20+8" }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $env:USERPROFILE "android-dev\android-sdk" }
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
if (-not $env:NDK_HOME) { $env:NDK_HOME = Join-Path $env:ANDROID_HOME "ndk\28.2.13676358" }
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = "$env:USERPROFILE\.cargo\target_novaproxy" }

$ndkBin = Join-Path $env:NDK_HOME "toolchains\llvm\prebuilt\windows-x86_64\bin"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$env:PATH = "$($env:JAVA_HOME)\bin;$ndkBin;$(Join-Path $env:ANDROID_HOME 'platform-tools');$cargoBin;$env:PATH"

$env:CC_aarch64_linux_android = Join-Path $ndkBin "aarch64-linux-android24-clang.cmd"
$env:CXX_aarch64_linux_android = Join-Path $ndkBin "aarch64-linux-android24-clang++.cmd"
$env:AR_aarch64_linux_android = Join-Path $ndkBin "llvm-ar.exe"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $env:CC_aarch64_linux_android

$env:CC_x86_64_linux_android = Join-Path $ndkBin "x86_64-linux-android24-clang.cmd"
$env:CXX_x86_64_linux_android = Join-Path $ndkBin "x86_64-linux-android24-clang++.cmd"
$env:AR_x86_64_linux_android = Join-Path $ndkBin "llvm-ar.exe"
$env:CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = $env:CC_x86_64_linux_android

Write-Host "=== 1. Building Frontend Bundle ===" -ForegroundColor Cyan
pnpm build

if (-not (Test-Path "src-tauri\gen\android\app\build.gradle.kts")) {
    Write-Host "=== 2. Initializing Tauri Android Project ===" -ForegroundColor Cyan
    pnpm tauri android init --ci --skip-targets-install
}

Write-Host "=== 3. Applying NovaProxy Android Manifest & Gradle Configuration ===" -ForegroundColor Cyan
node scripts/configure-android.mjs

if (-not $SkipCoreDownload) {
    Write-Host "=== 4. Fetching Android Proxy Cores (arm64 + x86_64) ===" -ForegroundColor Cyan
    ./scripts/fetch-cores.ps1 -Platform android -Arch arm64
    ./scripts/fetch-cores.ps1 -Platform android -Arch amd64
}

$profileDir = if ($Release) { "release" } else { "debug" }
$cargoFlags = if ($Release) { @("--release") } else { @() }

Write-Host "=== 5. Cross-Compiling Rust Android Libraries ($profileDir) ===" -ForegroundColor Cyan
Push-Location src-tauri
try {
    cargo build --target aarch64-linux-android --lib @cargoFlags; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo build --target x86_64-linux-android --lib @cargoFlags; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path "src-tauri\gen\android\app\src\main\jniLibs\arm64-v8a" | Out-Null
New-Item -ItemType Directory -Force -Path "src-tauri\gen\android\app\src\main\jniLibs\x86_64" | Out-Null
Copy-Item "$($env:CARGO_TARGET_DIR)\aarch64-linux-android\$profileDir\libnovaproxy_lib.so" "src-tauri\gen\android\app\src\main\jniLibs\arm64-v8a\libnovaproxy_lib.so" -Force
Copy-Item "$($env:CARGO_TARGET_DIR)\x86_64-linux-android\$profileDir\libnovaproxy_lib.so" "src-tauri\gen\android\app\src\main\jniLibs\x86_64\libnovaproxy_lib.so" -Force

$gradleTask = if ($Release) { "assembleUniversalRelease" } else { "assembleUniversalDebug" }
$capProf = if ($Release) { "Release" } else { "Debug" }

Write-Host "=== 6. Assembling Universal APK ($gradleTask) ===" -ForegroundColor Cyan
Push-Location src-tauri\gen\android
try {
    .\gradlew.bat $gradleTask `
        "-PabiList=arm64-v8a,x86_64" `
        "-ParchList=arm64,x86_64" `
        "-PtargetList=aarch64,x86_64" `
        "-x" "rustBuildUniversal$capProf" `
        "-x" "rustBuildArm64$capProf" `
        "-x" "rustBuildX86_64$capProf"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

Write-Host "=== Android Build Completed ===" -ForegroundColor Green
Get-ChildItem "src-tauri\gen\android\app\build\outputs\apk\universal" -Filter "*.apk" -Recurse | Select-Object FullName, Length
