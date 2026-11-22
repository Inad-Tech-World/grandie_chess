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
    embed_lichess_token();
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}

/// Builds in the Lichess token the opening explorer uses when the user hasn't
/// saved their own. It comes from the GRANDIE_LICHESS_TOKEN environment
/// variable (release builds: an Actions secret) or the untracked file
/// src-tauri/.lichess-token (local builds), and is never committed. It's
/// stored scrambled with the key in src/secrets.rs, so the binary doesn't
/// contain it as plain text. Without either, the app asks users for a token.
fn embed_lichess_token() {
    const SCRAMBLE: &[u8] = b"grandie-chess:explorer";
    println!("cargo:rerun-if-env-changed=GRANDIE_LICHESS_TOKEN");
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".lichess-token");
    // Only watch the file if it exists: Cargo reruns the script on every
    // build for a missing one. (After creating it, `touch build.rs`.)
    if file.exists() {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let token = std::env::var("GRANDIE_LICHESS_TOKEN")
        .ok()
        .or_else(|| std::fs::read_to_string(&file).ok())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let Some(token) = token else { return };
    if !token.starts_with("lip_") || token.contains(char::is_whitespace) {
        println!("cargo:warning=GRANDIE_LICHESS_TOKEN doesn't look like a Lichess token (lip_…); not built in");
        return;
    }
    let hex: String = token.bytes().zip(SCRAMBLE.iter().cycle()).map(|(b, k)| format!("{:02x}", b ^ k)).collect();
    println!("cargo:rustc-env=GRANDIE_LICHESS_TOKEN_X={hex}");
}

/// Embeds `manifest` in every program this package links (the app and its tests).
fn embed_windows_manifest(manifest: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
