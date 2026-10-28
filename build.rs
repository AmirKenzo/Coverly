fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    #[cfg(windows)]
    {
        let version = env!("CARGO_PKG_VERSION");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico")
            .set("FileDescription", "Coverly")
            .set("ProductName", "Coverly")
            .set("CompanyName", "Coverly")
            .set("LegalCopyright", "MIT License")
            .set("OriginalFilename", "coverly.exe")
            .set("FileVersion", version)
            .set("ProductVersion", version);
        if let Err(e) = res.compile() {
            println!(
                "cargo:warning=Program icon and name not embedded ({e}). Install the Windows SDK to include them."
            );
        }
    }
}
