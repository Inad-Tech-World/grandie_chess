use std::path::{Path, PathBuf};

use crate::error::Result;

/// Locations of everything Grandie Chess stores on disk, all under the
/// platform's app data folder: `~/Library/Application Support/com.grandie.chess/`
/// on macOS, `%APPDATA%\com.grandie.chess\` on Windows.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub networks_dir: PathBuf,
    pub engines_dir: PathBuf,
    pub pgn_dir: PathBuf,
    pub settings_file: PathBuf,
    pub db_file: PathBuf,
    pub puzzles_db_file: PathBuf,
}

impl AppPaths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        let paths = AppPaths {
            networks_dir: data_dir.join("networks"),
            engines_dir: data_dir.join("engines"),
            pgn_dir: data_dir.join("pgn"),
            settings_file: data_dir.join("settings.json"),
            db_file: data_dir.join("grandie.db"),
            puzzles_db_file: data_dir.join("puzzles.db"),
            data_dir,
        };
        for dir in [&paths.data_dir, &paths.networks_dir, &paths.engines_dir, &paths.pgn_dir] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(paths)
    }
}

/// Path of the bundled lc0 sidecar (`lc0`, or `lc0.exe` on Windows). Tauri
/// copies it next to the main executable in both `tauri dev` and the packaged
/// app. `GRANDIE_LC0` overrides it (tests point it at the fake engine).
pub fn bundled_lc0() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GRANDIE_LC0") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(format!("lc0{}", std::env::consts::EXE_SUFFIX));
    candidate.exists().then_some(candidate)
}

/// The folder Tauri's `app_data_dir()` resolves to, for tests and developer
/// tools that run without Tauri.
pub fn default_data_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    };
    Some(base.join("com.grandie.chess"))
}

/// lc0 for tests and developer tools that run outside the app (`GRANDIE_LC0`
/// overrides it). On macOS it is the sidecar in `src-tauri/binaries/`. On
/// Windows it is the copy the build places in the target folder, next to the
/// DLLs it needs; the copy in `binaries/` can't find them.
pub fn dev_lc0() -> PathBuf {
    if let Some(p) = std::env::var_os("GRANDIE_LC0") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        // Binaries live in target/<profile>/, test binaries in target/<profile>/deps/.
        let exe = std::env::current_exe().unwrap_or_default();
        exe.ancestors().skip(1).take(2).map(|d| d.join("lc0.exe")).find(|p| p.exists()).unwrap_or_else(|| PathBuf::from("lc0.exe"))
    } else {
        let triple = if cfg!(target_arch = "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" };
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!("lc0-{triple}"))
    }
}

pub fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
