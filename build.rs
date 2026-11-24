fn main() {
    slint_build::compile("ui/app.slint").expect("Slint compilation failed");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("ui/icons/app.ico");
        res.set("FileDescription", "ISOLite - Linux ISO Customization & Packaging Tool");
        res.set("ProductName", "ISOLite Studio");
        res.set("OriginalFilename", "ISOLite.exe");
        res.set("LegalCopyright", "Copyright (C) 2026 ISOLite Contributors");
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile Windows resource: {}", e);
        }
    }
}
