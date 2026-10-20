
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();

        res.set_icon("assets/WinTune.ico");
        res.set("ProductName", "WinTune");
        res.set("FileDescription", "WinTune");
        res.set("InternalName", "WinTune");
        res.set("OriginalFilename", "WinTune.exe");

        res.set_manifest(
            r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
            </requestedPrivileges>
        </security>
    </trustInfo>
</assembly>
"#,
        );

        res.compile()
            .expect("Échec de la compilation des ressources Windows");
    }
}
