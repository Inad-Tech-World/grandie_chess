//! Installing engines the app downloads for the user: the official Stockfish
//! release, pinned to one version and checked by SHA-256.

use std::path::{Path, PathBuf};

use super::platform::engine_command;
use super::registry::{EngineConfig, OptionValue, Registry};
use crate::error::{Error, Result};
use crate::net::download::{download_file, Expect};

/// Registry id of the downloaded Stockfish.
pub const STOCKFISH_ID: &str = "stockfish";
pub const STOCKFISH_VERSION: &str = "19";

/// One downloadable Stockfish build. Bump all of these together; the hashes
/// come from the GitHub release's asset digests.
pub struct StockfishAsset {
    pub url: String,
    pub archive: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

const RELEASE: &str = "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/";

/// The build for this computer, if Stockfish publishes one.
pub fn stockfish_asset() -> Option<StockfishAsset> {
    let (file, size, sha256) = if cfg!(target_os = "macos") {
        ("stockfish-macos-universal.tar.gz", 82_323_876, "a1f0e3bcc5a6927a11fe6fc8e54a779754645f3c2bae2cf13420fd1957adaa77")
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        ("stockfish-windows-arm64-universal.zip", 80_190_536, "8372ad3f0d7276deb2c70f801f541ec7db463219fc6d9c7592864e542aa4f401")
    } else if cfg!(windows) {
        ("stockfish-windows-x86-64-universal.zip", 81_431_614, "3c8bf1f9ea66a09350a40df4f632288285ac206d99f33ab5842c408fc30b48a7")
    } else {
        return None;
    };
    let url = format!("{RELEASE}{file}");
    let archive = if file.ends_with(".zip") { "stockfish.zip" } else { "stockfish.tar.gz" };
    Some(StockfishAsset { url, archive, size, sha256 })
}

/// The archive tool. Windows 10 and later ship bsdtar, which also reads zip
/// files; it is named by full path so Git's GNU tar on PATH can't stand in.
fn tar_program() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join("System32").join("tar.exe")
    } else {
        PathBuf::from("tar")
    }
}

/// Download, unpack and register Stockfish. The caller must have stopped any
/// running Stockfish first: its folder is replaced, and Windows can't delete
/// a running program.
pub async fn install_stockfish(
    http: &reqwest::Client,
    engines_dir: &Path,
    registry: &Registry,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<EngineConfig> {
    let asset = stockfish_asset().ok_or_else(|| Error::other("Stockfish isn't available for this computer"))?;
    let dir = engines_dir.join(STOCKFISH_ID);
    let archive = engines_dir.join(asset.archive);
    download_file(http, &asset.url, &archive, Expect { size: Some(asset.size), sha256: Some(asset.sha256) }, progress).await?;
    // Unpack beside the old copy, then swap, so a failed unpack keeps it.
    let staging = engines_dir.join("stockfish.new");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let status = engine_command(tar_program()).arg("-xf").arg(&archive).arg("-C").arg(&staging).status().await?;
    let _ = std::fs::remove_file(&archive);
    if !status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(Error::other("could not unpack the Stockfish download"));
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| Error::other(format!("could not replace the old Stockfish (is it still running?): {e}")))?;
    }
    std::fs::rename(&staging, &dir)?;
    let exe = find_executable(&dir, STOCKFISH_ID).ok_or_else(|| Error::other("no Stockfish binary in the download"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))?;
    }
    let mut config = Registry::probe(&exe).await?;
    config.id = STOCKFISH_ID.into();
    config.options = default_stockfish_options();
    registry.upsert(config.clone())?;
    Ok(config)
}

/// Leave two cores for the app and the other engines.
pub fn default_stockfish_options() -> Vec<OptionValue> {
    let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(2).max(1)).unwrap_or(2);
    vec![OptionValue { name: "Threads".into(), value: threads.to_string() }, OptionValue { name: "Hash".into(), value: "256".into() }]
}

pub fn find_executable(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| is_engine_binary(n, prefix, cfg!(windows))) {
                return Some(path);
            }
        }
    }
    None
}

/// Whether a file in an engine download is the program itself: `.exe` on
/// Windows, no extension elsewhere (which skips READMEs and sources).
pub fn is_engine_binary(name: &str, prefix: &str, windows: bool) -> bool {
    if !name.starts_with(prefix) {
        return false;
    }
    if windows {
        name.strip_suffix(".exe").is_some_and(|stem| !stem.contains('.'))
    } else {
        !name.contains('.')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_stockfish_program_in_a_download() {
        assert!(is_engine_binary("stockfish-macos-universal", "stockfish", false));
        assert!(!is_engine_binary("stockfish.md", "stockfish", false));
        assert!(is_engine_binary("stockfish-windows-x86-64-universal.exe", "stockfish", true));
        assert!(!is_engine_binary("stockfish-windows-x86-64-universal.pdb", "stockfish", true));
    }

    #[test]
    fn pins_a_checksummed_asset_for_this_platform() {
        if let Some(asset) = stockfish_asset() {
            assert!(asset.url.contains("/download/sf_19/"), "pinned, never `latest`: {}", asset.url);
            assert_eq!(asset.sha256.len(), 64);
            assert!(asset.size > 10_000_000);
        }
    }
}
