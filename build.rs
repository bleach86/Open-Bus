//! On macOS, clang's runtime library linked in: SDL, built into the program
//! there, checks the macOS it's running on (`@available`) where it's built
//! for older ones than the newest, and clang makes those checks calls to
//! `__isPlatformVersionAtLeast`, which is in that library. Rust links
//! without it, and the symbol's missing. Elsewhere, nothing.

use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let resources = Command::new("clang")
        .arg("--print-resource-dir")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    match resources {
        Some(dir) => {
            println!("cargo:rustc-link-search=native={dir}/lib/darwin");
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        }
        None => println!("cargo:warning=clang's resource folder not found: SDL may not link"),
    }
}
