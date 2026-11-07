use crate::core::process::configure_command_no_window;
use crate::model::CoreType;
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
    #[error(
        "New binary for {core} failed verification ({reason}); rolled back to previous binary"
    )]
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
        .user_agent("NovaProxy-CoreUpdater/0.1.0")
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

    let backup_path = target_binary.with_extension("exe.bak");
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

fn select_windows_asset(core: CoreType, assets: &[GithubAsset]) -> Option<&GithubAsset> {
    match core {
        CoreType::SingBox => assets
            .iter()
            .find(|a| a.name.starts_with("sing-box-") && a.name.ends_with("-windows-amd64.zip")),
        CoreType::Mihomo => assets
            .iter()
            .find(|a| {
                a.name.starts_with("mihomo-windows-amd64-compatible-") && a.name.ends_with(".zip")
            })
            .or_else(|| {
                assets.iter().find(|a| {
                    a.name.starts_with("mihomo-windows-amd64-") && a.name.ends_with(".zip")
                })
            }),
        CoreType::Xray => assets.iter().find(|a| a.name == "Xray-windows-64.zip"),
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

        let asset = select_windows_asset(core, &release.assets).ok_or_else(|| {
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

    /// Downloads, extracts, verifies, and atomically installs a core update with rollback on failure
    pub async fn update_core(
        &self,
        core: CoreType,
        proxy_url: Option<&str>,
        expected_sha256: Option<&str>,
    ) -> Result<CoreUpdateResult, UpdaterError> {
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
        let zip_path = temp_dir.join(&check.asset_name);
        std::fs::write(&zip_path, &bytes)?;

        let candidate_path = temp_dir.join(format!("{}.candidate.exe", core.as_str()));
        extract_exe_from_zip(&zip_path, core, &candidate_path)?;
        let _ = std::fs::remove_file(&zip_path);

        let target_binary = self.cores_dir.join(core.binary_name());
        let sha256 = apply_binary_update_with_rollback(
            core,
            &target_binary,
            &candidate_path,
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
}

fn extract_exe_from_zip(
    zip_path: &Path,
    core: CoreType,
    dest_exe: &Path,
) -> Result<(), UpdaterError> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let expected_name = core.binary_name();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_ascii_lowercase();
        if name.ends_with(&expected_name.to_ascii_lowercase()) || name.ends_with(".exe") {
            let mut out = std::fs::File::create(dest_exe)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }

    Err(UpdaterError::BinaryMissingInArchive(
        expected_name.to_string(),
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
        assert!(!target_bin.with_extension("exe.bak").exists());
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
