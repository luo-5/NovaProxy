use crate::core::process::{configure_command_no_window, ensure_executable};
use crate::model::CoreType;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum UpdaterError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Invalid proxy URL '{0}': {1}")]
    InvalidProxy(String, String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No compatible release asset found for {core} in release {tag}")]
    AssetNotFound { core: CoreType, tag: String },
    #[error("Executable '{0}' not found inside downloaded archive")]
    BinaryMissingInArchive(String),
    #[error("SHA-256 checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("Archive error: {0}")]
    Archive(String),
    #[error("Unsupported operation on this platform: {0}")]
    UnsupportedOnPlatform(String),
    #[error("Verification failed and rolled back to previous binary for {core}: {reason}")]
    VerificationFailedRolledBack { core: CoreType, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CoreLockEntry {
    pub version: String,
    pub sha256: String,
    pub url: String,
}

pub type CoresLockMap = BTreeMap<String, CoreLockEntry>;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CoreUpdateCheckResult {
    pub core: CoreType,
    pub current_version: Option<String>,
    pub latest_version: String,
    pub has_update: bool,
    pub download_url: String,
    pub asset_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CoreUpdateResult {
    pub core: CoreType,
    pub previous_version: Option<String>,
    pub new_version: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CoreBinaryStatus {
    pub core: CoreType,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub lock_version: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Builds a `reqwest::Client` optionally routed through the current local proxy
pub fn build_http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, UpdaterError> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("NovaProxy-CoreUpdater/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60));

    if let Some(raw_proxy) = proxy_url.map(str::trim).filter(|s| !s.is_empty()) {
        let proxy = reqwest::Proxy::all(raw_proxy)
            .map_err(|e| UpdaterError::InvalidProxy(raw_proxy.to_string(), e.to_string()))?;
        builder = builder.proxy(proxy);
    }

    Ok(builder.build()?)
}

/// Computes the uppercase hex SHA-256 digest of a file
pub fn compute_file_sha256(path: &Path) -> Result<String, UpdaterError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode_upper(hasher.finalize()))
}

/// Verifies that a core binary executes its version flag (`version` / `-v`) and returns the version output
pub async fn verify_core_binary_exec(core: CoreType, binary_path: &Path) -> Result<String, String> {
    if !binary_path.exists() {
        return Err(format!("Binary does not exist: {}", binary_path.display()));
    }
    ensure_executable(binary_path);

    let mut cmd = Command::new(binary_path);
    configure_command_no_window(&mut cmd);

    match core {
        CoreType::SingBox => {
            cmd.arg("version");
        }
        CoreType::Mihomo => {
            cmd.arg("-v");
        }
        CoreType::Xray => {
            cmd.arg("version");
        }
    }

    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| "Version command timed out".to_string())?
        .map_err(|e| format!("Failed to execute binary: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Binary exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim().to_string();
    Ok(first_line)
}

/// Atomically installs `candidate_binary` to `target_binary`, verifies execution,
/// and automatically rolls back to the backup if verification fails.
pub async fn apply_binary_update_with_rollback(
    core: CoreType,
    target_binary: &Path,
    candidate_binary: &Path,
    expected_sha256: Option<&str>,
) -> Result<String, UpdaterError> {
    let actual_sha256 = compute_file_sha256(candidate_binary)?;
    if let Some(expected) = expected_sha256.map(str::trim).filter(|s| !s.is_empty()) {
        if !actual_sha256.eq_ignore_ascii_case(expected) {
            return Err(UpdaterError::ChecksumMismatch {
                expected: expected.to_ascii_uppercase(),
                actual: actual_sha256,
            });
        }
    }

    if let Some(parent) = target_binary.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let backup_path = PathBuf::from(format!("{}.bak", target_binary.display()));
    let had_existing = target_binary.exists();

    if had_existing {
        if backup_path.exists() {
            let _ = std::fs::remove_file(&backup_path);
        }
        std::fs::rename(target_binary, &backup_path)?;
    }

    if let Err(copy_err) = std::fs::copy(candidate_binary, target_binary) {
        if had_existing && backup_path.exists() {
            let _ = std::fs::rename(&backup_path, target_binary);
        }
        return Err(UpdaterError::Io(copy_err));
    }

    match verify_core_binary_exec(core, target_binary).await {
        Ok(_version_line) => {
            if backup_path.exists() {
                let _ = std::fs::remove_file(&backup_path);
            }
            let _ = std::fs::remove_file(candidate_binary);
            Ok(actual_sha256)
        }
        Err(reason) => {
            // Rollback!
            let _ = std::fs::remove_file(target_binary);
            if had_existing && backup_path.exists() {
                let _ = std::fs::rename(&backup_path, target_binary);
            }
            let _ = std::fs::remove_file(candidate_binary);
            Err(UpdaterError::VerificationFailedRolledBack { core, reason })
        }
    }
}

fn github_repo_for_core(core: CoreType) -> &'static str {
    match core {
        CoreType::SingBox => "SagerNet/sing-box",
        CoreType::Mihomo => "MetaCubeX/mihomo",
        CoreType::Xray => "XTLS/Xray-core",
    }
}

fn select_platform_asset(core: CoreType, assets: &[GithubAsset]) -> Option<&GithubAsset> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (core, os, arch) {
        // ---- sing-box ----
        (CoreType::SingBox, "windows", "x86_64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-windows-amd64.zip")),
        (CoreType::SingBox, "windows", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-windows-arm64.zip")),
        (CoreType::SingBox, "macos", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-darwin-arm64.tar.gz")),
        (CoreType::SingBox, "macos", "x86_64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-darwin-amd64.tar.gz")),
        (CoreType::SingBox, "linux", "x86_64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-linux-amd64.tar.gz")),
        (CoreType::SingBox, "linux", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-linux-arm64.tar.gz")),
        (CoreType::SingBox, "android", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-android-arm64.tar.gz")),
        (CoreType::SingBox, "android", "x86_64") => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-android-amd64.tar.gz")),

        // ---- mihomo ----
        (CoreType::Mihomo, "windows", "x86_64") => assets
            .iter()
            .find(|a| {
                a.name.starts_with("mihomo-windows-amd64-compatible-") && a.name.ends_with(".zip")
            })
            .or_else(|| {
                assets.iter().find(|a| {
                    a.name.starts_with("mihomo-windows-amd64-") && a.name.ends_with(".zip")
                })
            }),
        (CoreType::Mihomo, "windows", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("mihomo-windows-arm64-") && a.name.ends_with(".zip")),
        (CoreType::Mihomo, "macos", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("mihomo-darwin-arm64-v") && a.name.ends_with(".gz"))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.starts_with("mihomo-darwin-arm64-") && a.name.ends_with(".gz"))
            }),
        (CoreType::Mihomo, "macos", "x86_64") => assets
            .iter()
            .find(|a| {
                a.name.starts_with("mihomo-darwin-amd64-compatible-") && a.name.ends_with(".gz")
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.starts_with("mihomo-darwin-amd64-") && a.name.ends_with(".gz"))
            }),
        (CoreType::Mihomo, "linux", "x86_64") => assets
            .iter()
            .find(|a| {
                a.name.starts_with("mihomo-linux-amd64-compatible-") && a.name.ends_with(".gz")
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.starts_with("mihomo-linux-amd64-") && a.name.ends_with(".gz"))
            }),
        (CoreType::Mihomo, "linux", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("mihomo-linux-arm64-v") && a.name.ends_with(".gz"))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.starts_with("mihomo-linux-arm64-") && a.name.ends_with(".gz"))
            }),
        (CoreType::Mihomo, "android", "aarch64") => assets
            .iter()
            .find(|a| a.name.starts_with("mihomo-android-arm64-v8-") && a.name.ends_with(".gz")),
        (CoreType::Mihomo, "android", "x86_64") => assets
            .iter()
            .find(|a| a.name.starts_with("mihomo-android-amd64-") && a.name.ends_with(".gz")),

        // ---- xray ----
        (CoreType::Xray, "windows", "x86_64") => {
            assets.iter().find(|a| a.name == "Xray-windows-64.zip")
        }
        (CoreType::Xray, "windows", "aarch64") => assets
            .iter()
            .find(|a| a.name == "Xray-windows-arm64-v8a.zip"),
        (CoreType::Xray, "macos", "aarch64") => {
            assets.iter().find(|a| a.name == "Xray-macos-arm64-v8a.zip")
        }
        (CoreType::Xray, "macos", "x86_64") => {
            assets.iter().find(|a| a.name == "Xray-macos-64.zip")
        }
        (CoreType::Xray, "linux", "x86_64") => {
            assets.iter().find(|a| a.name == "Xray-linux-64.zip")
        }
        (CoreType::Xray, "linux", "aarch64") => {
            assets.iter().find(|a| a.name == "Xray-linux-arm64-v8a.zip")
        }
        (CoreType::Xray, "android", "aarch64") => assets
            .iter()
            .find(|a| a.name == "Xray-android-arm64-v8a.zip"),
        (CoreType::Xray, "android", "x86_64") => {
            assets.iter().find(|a| a.name == "Xray-android-amd64.zip")
        }
        _ => None,
    }
}

fn normalize_version_tag(v: &str) -> String {
    v.trim().trim_start_matches('v').to_ascii_lowercase()
}

/// Manages checking and updating core binaries with lockfile persistence
pub struct CoreUpdater {
    cores_dir: PathBuf,
    lock_file_path: PathBuf,
}

impl CoreUpdater {
    pub fn new(cores_dir: impl Into<PathBuf>, lock_file_path: impl Into<PathBuf>) -> Self {
        Self {
            cores_dir: cores_dir.into(),
            lock_file_path: lock_file_path.into(),
        }
    }

    pub fn read_lock_file(&self) -> CoresLockMap {
        if !self.lock_file_path.exists() {
            return BTreeMap::new();
        }
        std::fs::read_to_string(&self.lock_file_path)
            .ok()
            .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default()
    }

    pub fn write_lock_file(&self, map: &CoresLockMap) -> Result<(), UpdaterError> {
        let json = serde_json::to_string_pretty(map)?;
        std::fs::write(&self.lock_file_path, json)?;
        Ok(())
    }

    pub async fn check_for_update(
        &self,
        core: CoreType,
        proxy_url: Option<&str>,
    ) -> Result<CoreUpdateCheckResult, UpdaterError> {
        let client = build_http_client(proxy_url)?;
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            github_repo_for_core(core)
        );
        let release: GithubRelease = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let asset = select_platform_asset(core, &release.assets).ok_or_else(|| {
            UpdaterError::AssetNotFound {
                core,
                tag: release.tag_name.clone(),
            }
        })?;

        let lock_map = self.read_lock_file();
        let current_version = lock_map.get(core.as_str()).map(|e| e.version.clone());

        let has_update = match &current_version {
            Some(curr) => normalize_version_tag(curr) != normalize_version_tag(&release.tag_name),
            None => true,
        };

        Ok(CoreUpdateCheckResult {
            core,
            current_version,
            latest_version: release.tag_name,
            has_update,
            download_url: asset.browser_download_url.clone(),
            asset_name: asset.name.clone(),
        })
    }

    pub fn cores_dir(&self) -> &Path {
        &self.cores_dir
    }

    /// Queries actual executable versions on disk alongside `cores.lock.json` metadata.
    pub async fn get_installed_statuses(&self) -> Vec<CoreBinaryStatus> {
        let lock_map = self.read_lock_file();
        let mut statuses = Vec::with_capacity(3);
        for core in [CoreType::SingBox, CoreType::Mihomo, CoreType::Xray] {
            let bin = self.cores_dir.join(core.binary_name());
            let installed = bin.exists();
            let installed_version = if installed {
                verify_core_binary_exec(core, &bin).await.ok()
            } else {
                None
            };
            let lock_entry = lock_map.get(core.as_str());
            statuses.push(CoreBinaryStatus {
                core,
                installed,
                installed_version,
                lock_version: lock_entry.map(|e| e.version.clone()),
                sha256: lock_entry.map(|e| e.sha256.clone()),
            });
        }
        statuses
    }

    /// Downloads & extracts a candidate binary update to temp staging BEFORE stopping the live core
    /// (allowing the download itself to traverse the currently running local proxy).
    pub async fn download_core_candidate(
        &self,
        core: CoreType,
        proxy_url: Option<&str>,
    ) -> Result<(CoreUpdateCheckResult, PathBuf), UpdaterError> {
        #[cfg(target_os = "android")]
        {
            let _ = (core, proxy_url);
            Err(UpdaterError::UnsupportedOnPlatform(
                "In-app binary replacement on Android is restricted by W^X; update via APK release"
                    .to_string(),
            ))
        }

        #[cfg(not(target_os = "android"))]
        {
            let check = self.check_for_update(core, proxy_url).await?;
            let client = build_http_client(proxy_url)?;

            let bytes = client
                .get(&check.download_url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;

            let temp_dir = std::env::temp_dir().join("novaproxy_updater");
            std::fs::create_dir_all(&temp_dir)?;
            let archive_path = temp_dir.join(&check.asset_name);
            std::fs::write(&archive_path, &bytes)?;

            let ext_suffix = if cfg!(target_os = "windows") {
                ".exe"
            } else {
                ""
            };
            let candidate_path =
                temp_dir.join(format!("{}.candidate{}", core.as_str(), ext_suffix));
            extract_binary_from_archive(&archive_path, core, &candidate_path)?;
            ensure_executable(&candidate_path);
            let _ = std::fs::remove_file(&archive_path);
            Ok((check, candidate_path))
        }
    }

    /// Installs a previously staged candidate binary, verifies it, and updates `cores.lock.json`.
    pub async fn finalize_candidate_update(
        &self,
        core: CoreType,
        check: CoreUpdateCheckResult,
        candidate_path: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<CoreUpdateResult, UpdaterError> {
        let target_binary = self.cores_dir.join(core.binary_name());
        let sha256 = apply_binary_update_with_rollback(
            core,
            &target_binary,
            candidate_path,
            expected_sha256,
        )
        .await?;

        let mut lock_map = self.read_lock_file();
        lock_map.insert(
            core.as_str().to_string(),
            CoreLockEntry {
                version: check.latest_version.clone(),
                sha256: sha256.clone(),
                url: check.download_url,
            },
        );
        let _ = self.write_lock_file(&lock_map);

        Ok(CoreUpdateResult {
            core,
            previous_version: check.current_version,
            new_version: check.latest_version,
            sha256,
        })
    }

    /// Downloads, extracts, verifies, and atomically installs a core update with rollback on failure
    pub async fn update_core(
        &self,
        core: CoreType,
        proxy_url: Option<&str>,
        expected_sha256: Option<&str>,
    ) -> Result<CoreUpdateResult, UpdaterError> {
        let (check, candidate_path) = self.download_core_candidate(core, proxy_url).await?;
        self.finalize_candidate_update(core, check, &candidate_path, expected_sha256)
            .await
    }
}

#[cfg_attr(target_os = "android", allow(dead_code))]
fn extract_binary_from_archive(
    archive_path: &Path,
    core: CoreType,
    dest_bin: &Path,
) -> Result<(), UpdaterError> {
    let lower_name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if lower_name.ends_with(".tar.gz") || lower_name.ends_with(".tgz") {
        let file = std::fs::File::open(archive_path)?;
        let gz = GzDecoder::new(file);
        let mut tar_arch = tar::Archive::new(gz);
        let expected_stem = core.as_str().to_ascii_lowercase();
        let entries = tar_arch
            .entries()
            .map_err(|e| UpdaterError::Archive(e.to_string()))?;
        for entry_res in entries {
            let mut entry = entry_res.map_err(|e| UpdaterError::Archive(e.to_string()))?;
            let path = entry
                .path()
                .map_err(|e| UpdaterError::Archive(e.to_string()))?;
            let fname = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if fname == expected_stem || fname == core.binary_name().to_ascii_lowercase() {
                let mut out = std::fs::File::create(dest_bin)?;
                std::io::copy(&mut entry, &mut out)?;
                ensure_executable(dest_bin);
                return Ok(());
            }
        }
        return Err(UpdaterError::BinaryMissingInArchive(
            core.binary_name().to_string(),
        ));
    }

    if lower_name.ends_with(".gz") {
        let file = std::fs::File::open(archive_path)?;
        let mut gz = GzDecoder::new(file);
        let mut out = std::fs::File::create(dest_bin)?;
        std::io::copy(&mut gz, &mut out)?;
        ensure_executable(dest_bin);
        return Ok(());
    }

    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let expected_name = core.binary_name().to_ascii_lowercase();
    let expected_stem = core.as_str().to_ascii_lowercase();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let fname = Path::new(entry.name())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if fname == expected_name
            || fname == expected_stem
            || (cfg!(target_os = "windows") && fname.ends_with(".exe"))
        {
            let mut out = std::fs::File::create(dest_bin)?;
            std::io::copy(&mut entry, &mut out)?;
            ensure_executable(dest_bin);
            return Ok(());
        }
    }

    Err(UpdaterError::BinaryMissingInArchive(
        core.binary_name().to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rollback_when_candidate_binary_is_corrupted() {
        let temp_dir = std::env::temp_dir().join("novaproxy_test_updater_rollback");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let target_bin = temp_dir.join("sing-box.exe");
        let original_bytes = b"ORIGINAL_WORKING_BINARY_PLACEHOLDER";
        std::fs::write(&target_bin, original_bytes).unwrap();

        // Write an invalid/corrupted executable as the update candidate
        let corrupted_candidate = temp_dir.join("sing-box.candidate.exe");
        std::fs::write(&corrupted_candidate, b"NOT_A_VALID_PE_EXECUTABLE").unwrap();

        let res = apply_binary_update_with_rollback(
            CoreType::SingBox,
            &target_bin,
            &corrupted_candidate,
            None,
        )
        .await;

        assert!(matches!(
            res,
            Err(UpdaterError::VerificationFailedRolledBack {
                core: CoreType::SingBox,
                ..
            })
        ));

        // Verify original binary was restored intact and backup/candidate were cleaned up
        let restored_bytes = std::fs::read(&target_bin).unwrap();
        assert_eq!(restored_bytes, original_bytes);
        assert!(!corrupted_candidate.exists());
        assert!(!PathBuf::from(format!("{}.bak", target_bin.display())).exists());
    }

    #[tokio::test]
    async fn test_checksum_mismatch_aborts_before_touching_target() {
        let temp_dir = std::env::temp_dir().join("novaproxy_test_updater_hash");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let target_bin = temp_dir.join("xray.exe");
        std::fs::write(&target_bin, b"ORIGINAL").unwrap();

        let candidate = temp_dir.join("xray.candidate.exe");
        std::fs::write(&candidate, b"CANDIDATE").unwrap();

        let res = apply_binary_update_with_rollback(
            CoreType::Xray,
            &target_bin,
            &candidate,
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
        )
        .await;

        assert!(matches!(res, Err(UpdaterError::ChecksumMismatch { .. })));
        assert_eq!(std::fs::read(&target_bin).unwrap(), b"ORIGINAL");
    }

    #[test]
    fn test_proxy_http_client_builder() {
        assert!(build_http_client(None).is_ok());
        assert!(build_http_client(Some("http://127.0.0.1:7890")).is_ok());
        assert!(build_http_client(Some("socks5h://127.0.0.1:7890")).is_ok());
    }
}
