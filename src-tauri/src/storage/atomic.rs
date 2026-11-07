//! Atomic file writing utilities with temporary file staging and fsync

use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Atomically writes `content` to `path` using temporary file staging and fsync:
/// 1. Writes to `{path}.tmp.{uuid}`
/// 2. Flushes and calls `sync_all()` on the file descriptor
/// 3. Safely renames the temporary file to `target`
/// 4. Cleans up temporary file if any step fails
pub fn atomic_write_file(path: impl AsRef<Path>, content: &[u8]) -> std::io::Result<()> {
    let target = path.as_ref();
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let temp_name = format!(
        "{}.tmp.{}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file"),
        uuid::Uuid::new_v4()
    );
    let temp_path = target.with_file_name(temp_name);

    let write_res = (|| -> std::io::Result<()> {
        let mut file = File::create(&temp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    // Attempt rename. On Windows, if target exists and direct rename fails,
    // remove target first and then rename.
    if target.exists() {
        if std::fs::rename(&temp_path, target).is_err() {
            let _ = std::fs::remove_file(target);
            if let Err(e) = std::fs::rename(&temp_path, target) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(e);
            }
        }
    } else if let Err(e) = std::fs::rename(&temp_path, target) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_write_file_creation_and_overwrite() {
        let temp_dir = std::env::temp_dir().join("novaproxy_atomic_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("test_file.txt");

        let data1 = b"Hello NovaProxy Phase P6!";
        atomic_write_file(&test_file, data1).expect("write file 1");
        assert_eq!(std::fs::read(&test_file).unwrap(), data1);

        let data2 = b"Overwritten atomically with new data!";
        atomic_write_file(&test_file, data2).expect("overwrite file 2");
        assert_eq!(std::fs::read(&test_file).unwrap(), data2);

        let _ = std::fs::remove_file(&test_file);
    }
}
