fn main() {
    #[cfg(windows)]
    {
        // Embeds the app icon and version metadata into the .exe so Explorer and the
        // taskbar show it, the way a shipped Windows tool does.
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../packaging/assets/icon.ico");
        res.set("ProductName", "persistex");
        res.set("FileDescription", "Multisine excitation designer");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=icon embedding skipped: {e}");
        }
    }
    println!("cargo:rerun-if-changed=../packaging/assets/icon.ico");
}
