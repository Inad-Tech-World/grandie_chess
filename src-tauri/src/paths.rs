use std::path::{Path, PathBuf};

use crate::error::Result;

/// Locations of everything Grandie Chess stores on disk, all under
/// `~/Library/Application Support/com.grandie.chess/`.
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

/// Path of the bundled lc0 sidecar. Tauri copies it next to the main
/// executable in both `tauri dev` and the packaged app. `GRANDIE_LC0`
/// overrides it (tests point it at the fake engine).
pub fn bundled_lc0() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GRANDIE_LC0") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join("lc0");
    candidate.exists().then_some(candidate)
}

pub fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
