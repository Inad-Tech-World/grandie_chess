//! Windows (MSVC) only: link the Visual C++ runtime statically, as the app does.
//!
//! The app crate's build script (tauri-build, `static_vcruntime.rs`) links the
//! runtime statically by putting an empty `msvcrt.lib` on the library search
//! path and asking for the static libraries with linker arguments. The search
//! path reaches every program that links the app crate, but the linker
//! arguments only apply to the app crate's own targets. Without the arguments
//! below, the fake engine, the calibration tool and their tests would link
//! against the empty library and miss `malloc`, `free` and the rest of the C
//! runtime.
//!
//! They also get the app's manifest (Common Controls v6, see `../build.rs`),
//! so they load even if they end up linking Tauri's dialog code.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    for lib in
        ["libvcruntimed.lib", "vcruntime.lib", "vcruntimed.lib", "libcmtd.lib", "msvcrt.lib", "msvcrtd.lib", "libucrt.lib", "libucrtd.lib"]
    {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:{lib}");
    }
    for lib in ["libcmt.lib", "libvcruntime.lib", "ucrt.lib"] {
        println!("cargo:rustc-link-arg=/DEFAULTLIB:{lib}");
    }
}
