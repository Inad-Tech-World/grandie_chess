//! PGN parsing and the user's own files. File access is limited to the kinds
//! of files the app opens and saves (PGN and plain text), so a compromised
//! page can't read or overwrite arbitrary files.

use std::path::PathBuf;

use super::blocking;
use crate::error::{Error, Result};
use crate::game::pgn::{parse_pgn, read_pgn_file, write_pgn, PgnParseResult};
use crate::game::tree::GameTree;

/// Extensions of the files the app reads and writes for the user.
const USER_FILE_EXTENSIONS: &[&str] = &["pgn", "txt"];

/// Accept only absolute paths to PGN or text files.
fn user_file(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    let ext_ok = p.extension().and_then(|e| e.to_str()).is_some_and(|e| USER_FILE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)));
    if !p.is_absolute() || !ext_ok {
        return Err(Error::other("Grandie only opens and saves .pgn and .txt files"));
    }
    Ok(p)
}

#[tauri::command]
#[specta::specta]
pub async fn pgn_parse(text: String) -> PgnParseResult {
    match tokio::task::spawn_blocking(move || parse_pgn(&text)).await {
        Ok(parsed) => parsed,
        Err(e) => PgnParseResult { games: vec![], errors: vec![format!("could not read the PGN: {e}")] },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn pgn_read_file(path: String) -> Result<PgnParseResult> {
    let path = user_file(&path)?;
    blocking(move || read_pgn_file(&path)).await
}

#[tauri::command]
#[specta::specta]
pub fn pgn_write(tree: GameTree) -> String {
    write_pgn(&tree)
}

#[tauri::command]
#[specta::specta]
pub async fn file_read_text(path: String) -> Result<String> {
    let path = user_file(&path)?;
    blocking(move || {
        let bytes = std::fs::read(path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn file_write_text(path: String, contents: String) -> Result<()> {
    let path = user_file(&path)?;
    blocking(move || Ok(std::fs::write(path, contents)?)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pgn_and_text_files_are_allowed() {
        let abs = if cfg!(windows) { "C:\\games\\" } else { "/games/" };
        assert!(user_file(&format!("{abs}fischer.pgn")).is_ok());
        assert!(user_file(&format!("{abs}NOTES.TXT")).is_ok());
        assert!(user_file(&format!("{abs}id_rsa")).is_err());
        assert!(user_file(&format!("{abs}run.sh")).is_err());
        assert!(user_file("relative.pgn").is_err());
    }
}
