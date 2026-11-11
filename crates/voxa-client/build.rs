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
    }
}
