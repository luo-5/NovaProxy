use std::path::PathBuf;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut attrs = tauri_build::Attributes::new();

    if target_os == "windows" {
        // Copy icon to an ASCII temporary directory to avoid GNU windres path encoding issues
        // when workspace paths contain non-ASCII characters.
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
        attrs = attrs.windows_attributes(windows_attr);
    }

    tauri_build::try_build(attrs).expect("failed to run tauri-build");

    // Ensure unit test binaries (--lib --test) receive the Common-Controls v6 SxS manifest
    // (`rustc-link-arg-tests` skips `--lib` unit test harnesses in Cargo), disable MinGW GCC 16's
    // incompatible default-manifest.o (which strips Common-Controls v6 -> 0xc0000139 on TaskDialogIndirect),
    // and prevent MinGW ld from auto-exporting >65535 archive symbols when building cdylib on Windows.
    if target_os == "windows" && target_env == "gnu" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,--exclude-all-symbols");
        if let Ok(out_dir) = std::env::var("OUT_DIR") {
            let out_path = PathBuf::from(&out_dir);
            let specs_path = out_path.join("novaproxy_no_default_manifest.specs");
            let specs_content = "*endfile:\n%{mdaz-ftz:crtfastmath.o%s;Ofast|ffast-math|funsafe-math-optimizations:%{!shared:%{!mno-daz-ftz:crtfastmath.o%s}}} %{fvtable-verify=none:%s; fvtable-verify=preinit:vtv_end.o%s; fvtable-verify=std:vtv_end.o%s} crtend.o%s\n";
            if std::fs::write(&specs_path, specs_content).is_ok() {
                println!("cargo:rustc-link-arg=-specs={}", specs_path.display());
            }

            let resource_obj = out_path.join("libresource.a");
            if resource_obj.exists() {
                println!("cargo:rustc-link-arg={}", resource_obj.display());
            }
        }
    }
}
