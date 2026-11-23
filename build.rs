//! Embeds the Windows resources (icon, version info, UAC manifest) into the
//! `WinTune` binary only.
//!
//! The resources are linked with `rustc-link-arg-bin`, so the library's test
//! harness does NOT inherit the `requireAdministrator` manifest and
//! `cargo test` runs from a non-elevated terminal.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn rc_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "\\\\")
}

/// Writes `$OUT_DIR/i18n_langs.rs`: one `include_str!` per `i18n/strings*.resx`
/// (`strings.resx` is `en`, `strings.<code>.resx` is `<code>`), English first,
/// then by code. Runs on every target.
fn generate_language_table() {
    println!("cargo:rerun-if-changed=i18n");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dir = manifest_dir.join("i18n");

    let mut langs: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.ends_with(".resx") {
                println!("cargo:rerun-if-changed={}", path.display());
            }
            if name == "strings.resx" {
                langs.push(("en".to_string(), path.clone()));
            } else if let Some(code) = name
                .strip_prefix("strings.")
                .and_then(|n| n.strip_suffix(".resx"))
            {
                if !code.is_empty() {
                    langs.push((code.to_string(), path.clone()));
                }
            }
        }
    }
    assert!(
        langs.iter().any(|(code, _)| code == "en"),
        "i18n/strings.resx is missing: the English reference catalog must exist in {}",
        dir.display()
    );
    langs.sort_by(|a, b| (a.0 != "en", &a.0).cmp(&(b.0 != "en", &b.0)));

    let mut src = String::from("pub(crate) static LANG_FILES: &[(&str, &str)] = &[\n");
    for (code, path) in &langs {
        // `{:?}` yields an escaped string literal (backslashes on Windows).
        src.push_str(&format!(
            "    ({code:?}, include_str!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    src.push_str("];\n");
    fs::write(out_dir.join("i18n_langs.rs"), src).expect("Failed to write i18n_langs.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/WinTune.ico");
    println!("cargo:rerun-if-changed=assets/WinTune.manifest");

    generate_language_table();

    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let icon = manifest_dir.join("assets").join("WinTune.ico");
    let manifest = manifest_dir.join("assets").join("WinTune.manifest");

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let parse = |key: &str| {
        env::var(key)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(0)
    };
    let (major, minor, patch) = (
        parse("CARGO_PKG_VERSION_MAJOR"),
        parse("CARGO_PKG_VERSION_MINOR"),
        parse("CARGO_PKG_VERSION_PATCH"),
    );

    let rc = format!(
        r#"1 ICON "{icon}"
1 24 "{manifest}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "ProductName", "WinTune"
      VALUE "FileDescription", "WinTune"
      VALUE "InternalName", "WinTune"
      VALUE "OriginalFilename", "WinTune.exe"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = rc_path(&icon),
        manifest = rc_path(&manifest),
    );

    let rc_file = out_dir.join("wintune.rc");
    fs::write(&rc_file, rc).expect("Failed to write wintune.rc");

    embed_resource::compile_for(&rc_file, ["WinTune"], embed_resource::NONE)
        .manifest_required()
        .expect("Failed to compile Windows resources");
}
