fn main() {
    let mut attributes = tauri_build::Attributes::new();
    // On Windows, tauri-build puts the application manifest in a resource that
    // is linked into the app binary only. Test programs then start without
    // Common Controls v6 and Windows refuses to load them
    // (STATUS_ENTRYPOINT_NOT_FOUND, for TaskDialogIndirect). Embedding the same
    // manifest with the linker instead covers the app and the tests alike.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        attributes = attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        embed_windows_manifest(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml"));
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}

/// Embeds `manifest` in every program this package links (the app and its tests).
fn embed_windows_manifest(manifest: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
