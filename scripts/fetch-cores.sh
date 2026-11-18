#!/usr/bin/env bash
set -euo pipefail

# Cross-platform core fetcher for NovaProxy (Linux, macOS, Android)
# Usage: ./scripts/fetch-cores.sh [--platform linux|macos|android] [--arch amd64|arm64] [--target-dir <dir>]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

OS_UNAME="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH_UNAME="$(uname -m)"

PLATFORM=""
case "${OS_UNAME}" in
  darwin) PLATFORM="macos" ;;
  linux) PLATFORM="linux" ;;
  *) PLATFORM="linux" ;;
esac

ARCH=""
case "${ARCH_UNAME}" in
  x86_64|amd64) ARCH="amd64" ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) ARCH="amd64" ;;
esac

TARGET_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform) PLATFORM="$2"; shift 2 ;;
    --arch) ARCH="$2"; shift 2 ;;
    --target-dir) TARGET_DIR="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "${TARGET_DIR}" ]]; then
  if [[ "${PLATFORM}" == "android" ]]; then
    if [[ "${ARCH}" == "arm64" ]]; then
      ABI="arm64-v8a"
    else
      ABI="x86_64"
    fi
    TARGET_DIR="${PROJECT_ROOT}/src-tauri/gen/android/app/src/main/jniLibs/${ABI}"
  else
    TARGET_DIR="${PROJECT_ROOT}/src-tauri/resources/cores"
  fi
fi

mkdir -p "${TARGET_DIR}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

fetch_release_json() {
  local repo="$1"
  local url="https://api.github.com/repos/${repo}/releases/latest"
  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl -fsSL -H "User-Agent: NovaProxy-CoreFetcher" -H "Authorization: Bearer ${GITHUB_TOKEN}" "${url}"
  else
    curl -fsSL -H "User-Agent: NovaProxy-CoreFetcher" "${url}"
  fi
}

pick_asset_url() {
  local json="$1"
  shift
  python3 - "$json" "$@" <<'PYEOF'
import json, re, sys
data = json.loads(sys.argv[1])
patterns = sys.argv[2:]
assets = data.get("assets", [])
for pat in patterns:
    rx = re.compile(pat)
    for asset in assets:
        if rx.match(asset.get("name", "")):
            print(asset["browser_download_url"])
            sys.exit(0)
sys.exit(1)
PYEOF
}

download_file() {
  local url="$1"
  local dest="$2"
  curl -fsSL -H "User-Agent: NovaProxy-CoreFetcher" -o "${dest}" "${url}"
}

out_name() {
  local base="$1"
  if [[ "${PLATFORM}" == "android" ]]; then
    case "${base}" in
      sing-box) echo "libsingbox.so" ;;
      mihomo) echo "libmihomo.so" ;;
      xray) echo "libxray.so" ;;
    esac
  else
    echo "${base}"
  fi
}

echo "=== Fetching NovaProxy Cores (platform=${PLATFORM}, arch=${ARCH}) -> ${TARGET_DIR} ==="

# 1. sing-box
SB_PAT=""
case "${PLATFORM}:${ARCH}" in
  macos:arm64) SB_PAT="^sing-box-.*-darwin-arm64\.tar\.gz$" ;;
  macos:amd64) SB_PAT="^sing-box-.*-darwin-amd64\.tar\.gz$" ;;
  linux:amd64) SB_PAT="^sing-box-.*-linux-amd64\.tar\.gz$" ;;
  linux:arm64) SB_PAT="^sing-box-.*-linux-arm64\.tar\.gz$" ;;
  android:arm64) SB_PAT="^sing-box-.*-android-arm64\.tar\.gz$" ;;
  android:amd64) SB_PAT="^sing-box-.*-android-amd64\.tar\.gz$" ;;
  *) echo "Unsupported sing-box target ${PLATFORM}:${ARCH}" >&2; exit 1 ;;
esac

SB_JSON="$(fetch_release_json "SagerNet/sing-box")"
SB_URL="$(pick_asset_url "${SB_JSON}" "${SB_PAT}")"
echo "[1/3] Downloading sing-box: ${SB_URL}"
download_file "${SB_URL}" "${TMP_DIR}/singbox.tar.gz"
mkdir -p "${TMP_DIR}/sb_ext"
tar -xzf "${TMP_DIR}/singbox.tar.gz" -C "${TMP_DIR}/sb_ext"
SB_BIN="$(find "${TMP_DIR}/sb_ext" -type f -name 'sing-box' | head -n1)"
cp -f "${SB_BIN}" "${TARGET_DIR}/$(out_name sing-box)"
chmod 755 "${TARGET_DIR}/$(out_name sing-box)"

# 2. mihomo
MI_PATS=()
case "${PLATFORM}:${ARCH}" in
  macos:arm64) MI_PATS=("^mihomo-darwin-arm64-v.*\.gz$" "^mihomo-darwin-arm64-.*\.gz$") ;;
  macos:amd64) MI_PATS=("^mihomo-darwin-amd64-compatible-.*\.gz$" "^mihomo-darwin-amd64-.*\.gz$") ;;
  linux:amd64) MI_PATS=("^mihomo-linux-amd64-compatible-.*\.gz$" "^mihomo-linux-amd64-v[0-9]+\..*\.gz$" "^mihomo-linux-amd64-v1-v.*\.gz$") ;;
  linux:arm64) MI_PATS=("^mihomo-linux-arm64-v.*\.gz$" "^mihomo-linux-arm64-.*\.gz$") ;;
  android:arm64) MI_PATS=("^mihomo-android-arm64-v8-.*\.gz$") ;;
  android:amd64) MI_PATS=("^mihomo-android-amd64-.*\.gz$") ;;
  *) echo "Unsupported mihomo target ${PLATFORM}:${ARCH}" >&2; exit 1 ;;
esac

MI_JSON="$(fetch_release_json "MetaCubeX/mihomo")"
MI_URL="$(pick_asset_url "${MI_JSON}" "${MI_PATS[@]}")"
echo "[2/3] Downloading mihomo: ${MI_URL}"
download_file "${MI_URL}" "${TMP_DIR}/mihomo.gz"
gzip -dc "${TMP_DIR}/mihomo.gz" > "${TARGET_DIR}/$(out_name mihomo)"
chmod 755 "${TARGET_DIR}/$(out_name mihomo)"

# 3. xray
XR_PAT=""
case "${PLATFORM}:${ARCH}" in
  macos:arm64) XR_PAT="^Xray-macos-arm64-v8a\.zip$" ;;
  macos:amd64) XR_PAT="^Xray-macos-64\.zip$" ;;
  linux:amd64) XR_PAT="^Xray-linux-64\.zip$" ;;
  linux:arm64) XR_PAT="^Xray-linux-arm64-v8a\.zip$" ;;
  android:arm64) XR_PAT="^Xray-android-arm64-v8a\.zip$" ;;
  android:amd64) XR_PAT="^Xray-android-amd64\.zip$" ;;
  *) echo "Unsupported xray target ${PLATFORM}:${ARCH}" >&2; exit 1 ;;
esac

XR_JSON="$(fetch_release_json "XTLS/Xray-core")"
XR_URL="$(pick_asset_url "${XR_JSON}" "${XR_PAT}")"
echo "[3/3] Downloading Xray-core: ${XR_URL}"
download_file "${XR_URL}" "${TMP_DIR}/xray.zip"
mkdir -p "${TMP_DIR}/xr_ext"
unzip -qo "${TMP_DIR}/xray.zip" -d "${TMP_DIR}/xr_ext"
XR_BIN="$(find "${TMP_DIR}/xr_ext" -type f -name 'xray' | head -n1)"
cp -f "${XR_BIN}" "${TARGET_DIR}/$(out_name xray)"
chmod 755 "${TARGET_DIR}/$(out_name xray)"

echo "=== Completed core downloads cleanly ==="
ls -lh "${TARGET_DIR}"
