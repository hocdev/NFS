mod icon;
fn main() {
    slint_build::compile("ui/app.slint").expect("Slint compilation failed");
    println!("cargo:rerun-if-changed=ui/icons/app.ico");
    if std::env::var("TARGET")
        .unwrap_or_default()
        .starts_with("x86_64-pc-windows")
    {
        let path =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("app-icon.obj");
        icon::write(
            &path,
            &std::fs::read("ui/icons/app.ico").expect("Read icon"),
        )
        .expect("Build icon resource");
        println!("cargo:rustc-link-arg-bin=isolite={}", path.display());
    }
}
