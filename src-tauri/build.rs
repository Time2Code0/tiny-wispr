use std::path::PathBuf;
use std::process::Command;

fn main() {
    link_clang_runtime();
    tauri_build::build();
}

/// whisper.cpp's Metal backend uses `@available`, which calls
/// `__isPlatformVersionAtLeast` from clang's runtime whenever the deployment
/// target is older than the SDK (as in a bundle build). rustc links with
/// `-nodefaultlibs`, so the runtime has to be named.
fn link_clang_runtime() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(output) = Command::new("clang").arg("--print-resource-dir").output() else {
        return;
    };
    let dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        .join("lib")
        .join("darwin");
    if dir.join("libclang_rt.osx.a").is_file() {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
}
