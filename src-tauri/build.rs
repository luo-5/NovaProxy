use std::path::PathBuf;

fn main() {
    // Copy icon to an ASCII temporary directory to avoid GNU windres path encoding issues with non-ASCII workspace paths
    let icon_source = PathBuf::from("icons/icon.ico");
    let temp_icon = std::env::temp_dir().join("novaproxy_app_icon.ico");

    let mut windows_attr = tauri_build::WindowsAttributes::new();

    if icon_source.exists() {
        if let Err(e) = std::fs::copy(&icon_source, &temp_icon) {
            eprintln!("cargo:warning=Failed to copy icon to temp path: {}", e);
        } else {
            windows_attr = windows_attr.window_icon_path(temp_icon);
        }
    }

    let attrs = tauri_build::Attributes::new().windows_attributes(windows_attr);
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
