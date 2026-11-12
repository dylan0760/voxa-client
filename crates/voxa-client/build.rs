fn main() {
    println!("cargo:rerun-if-changed=../../assets/voxa.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/voxa.ico")
            .set("ProductName", "Voxa")
            .set("FileDescription", "Voxa voice chat client")
            .set("CompanyName", "Voxa Project")
            .set("LegalCopyright", "Voxa contributors - MIT OR Apache-2.0");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
        copy_webview2_loader();
    }
}

/// The video player (wry / WebView2) links Microsoft's redistributable WebView2Loader.dll
/// on GNU targets; put it next to voxa.exe so the program starts (and the MSI ships it).
fn copy_webview2_loader() {
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("aarch64") => "arm64",
        _ => "x86",
    };
    let home = std::env::var("CARGO_HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(".cargo")
    });
    let mut found = None;
    if let Ok(regs) = std::fs::read_dir(home.join("registry").join("src")) {
        for reg in regs.flatten() {
            let Ok(crates) = std::fs::read_dir(reg.path()) else { continue };
            for c in crates.flatten() {
                let name = c.file_name().to_string_lossy().to_string();
                if name.starts_with("webview2-com-sys-0.39.") {
                    let dll = c.path().join(arch).join("WebView2Loader.dll");
                    if dll.exists() {
                        found = Some(dll);
                    }
                }
            }
        }
    }
    let Some(dll) = found else {
        println!("cargo:warning=WebView2Loader.dll not found in the cargo registry");
        return;
    };
    // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if let Some(profile_dir) = out.ancestors().nth(3) {
        let _ = std::fs::copy(&dll, profile_dir.join("WebView2Loader.dll"));
    }
}
