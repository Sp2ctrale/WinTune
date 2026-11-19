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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/WinTune.ico");
    println!("cargo:rerun-if-changed=assets/WinTune.manifest");

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
