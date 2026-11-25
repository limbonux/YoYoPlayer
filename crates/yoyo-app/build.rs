use std::path::PathBuf;

fn main() {
    slint_build::compile("ui/app.slint").expect("Slint build failed");

    // The program icon and version shown by Explorer.
    println!("cargo:rerun-if-changed=../../assets/yoyo.ico");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/yoyo.ico").set("ProductName", "YoYo Player").set("FileDescription", "YoYo Player");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the program icon: {e}");
        }
    }

    // Copy libmpv-2.dll next to the built executable so `cargo run` finds it.
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dll = manifest.join("../../third_party/mpv/libmpv-2.dll");
    println!("cargo:rerun-if-changed={}", dll.display());
    if dll.exists() {
        // OUT_DIR is target/<profile>/build/<pkg>[/<hash>]/out; the profile dir is the
        // parent of the "build" ancestor, whichever layout cargo uses.
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let profile_dir = out.ancestors().find(|p| p.file_name().is_some_and(|n| n == "build")).and_then(|p| p.parent());
        match profile_dir {
            Some(dir) => {
                if let Err(e) = std::fs::copy(&dll, dir.join("libmpv-2.dll")) {
                    println!("cargo:warning=could not copy libmpv-2.dll: {e}");
                }
            }
            None => println!("cargo:warning=could not find the target profile dir from {}", out.display()),
        }
    } else {
        println!("cargo:warning=third_party/mpv/libmpv-2.dll is missing; run scripts/fetch-deps.ps1");
    }
}
