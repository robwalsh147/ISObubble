// Embeds the ISObubble icon in the .exe so Explorer, the desktop shortcut and the
// Start menu all show it. If no resource compiler is available the build still succeeds
// (installed shortcuts then use the separate .ico file written at install time).
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/isobubble.ico");
    println!("cargo:rerun-if-changed=assets/isobubble.rc");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || !Path::new("assets/isobubble.ico").exists() {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        // windres breaks on paths with spaces, so compile in a space-free temp folder.
        let tmp = std::env::temp_dir().join("isobubble_res");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = std::fs::copy("assets/isobubble.ico", tmp.join("isobubble.ico"));
        let _ = std::fs::copy("assets/isobubble.rc", tmp.join("isobubble.rc"));
        let ok = Command::new("windres")
            .current_dir(&tmp)
            .args(["isobubble.rc", "-O", "coff", "-o", "isobubble_icon.o"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("isobubble_icon.o");
            if std::fs::copy(tmp.join("isobubble_icon.o"), &out).is_ok() {
                println!("cargo:rustc-link-arg-bins={}", out.display());
                return;
            }
        }
        println!("cargo:warning=icon not embedded (windres unavailable)");
        return;
    }
    if let Err(e) = embed_resource::compile("assets/isobubble.rc", embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=icon not embedded: {e}");
    }
}
