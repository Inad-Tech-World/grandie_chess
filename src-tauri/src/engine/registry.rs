//! Engines the user can analyse with: the bundled lc0 plus any UCI engine
//! they add (Stockfish, for example). Stored in `engines.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::process::Engine;
use super::uci::UciOption;
use crate::error::{Error, Result};

pub const LC0_ID: &str = "lc0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EngineKind {
    /// The lc0 binary bundled with the app, using the selected network.
    Lc0,
    /// Any other UCI engine on disk.
    Uci,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OptionValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    pub id: String,
    pub name: String,
    pub kind: EngineKind,
    /// Executable path (UCI engines only).
    pub path: Option<String>,
    /// Options sent with `setoption` after start-up.
    pub options: Vec<OptionValue>,
    /// Options the engine advertised when it was added.
    pub available_options: Vec<UciOption>,
}

/// How an engine is read from engines.json: fields added in later versions
/// default, so older files keep loading. (Kept apart from [`EngineConfig`]
/// so the TypeScript type stays non-optional.)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredEngine {
    id: String,
    name: String,
    kind: EngineKind,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    options: Vec<OptionValue>,
    #[serde(default)]
    available_options: Vec<UciOption>,
}

impl From<StoredEngine> for EngineConfig {
    fn from(e: StoredEngine) -> Self {
        EngineConfig { id: e.id, name: e.name, kind: e.kind, path: e.path, options: e.options, available_options: e.available_options }
    }
}

impl EngineConfig {
    pub fn lc0() -> Self {
        EngineConfig {
            id: LC0_ID.into(),
            name: "Leela Chess Zero".into(),
            kind: EngineKind::Lc0,
            path: None,
            options: Vec::new(),
            available_options: Vec::new(),
        }
    }

    pub fn option(&self, name: &str) -> Option<&str> {
        self.options.iter().find(|o| o.name.eq_ignore_ascii_case(name)).map(|o| o.value.as_str())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct EnginesFile {
    engines: Vec<EngineConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StoredFile {
    #[serde(default)]
    engines: Vec<StoredEngine>,
}

pub struct Registry {
    path: PathBuf,
    /// Serialises read-modify-write so concurrent changes aren't lost.
    write_lock: std::sync::Mutex<()>,
}

impl Registry {
    pub fn new(data_dir: &Path) -> Self {
        Registry { path: data_dir.join("engines.json"), write_lock: std::sync::Mutex::new(()) }
    }

    fn read(&self) -> Result<EnginesFile> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<StoredFile>(&bytes)
                .map(|f| EnginesFile { engines: f.engines.into_iter().map(EngineConfig::from).collect() })
                .map_err(Error::from),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(EnginesFile::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// All engines, with the bundled lc0 always first. An unreadable file
    /// lists only lc0 but is left alone (see [`Registry::upsert`]).
    pub fn list(&self) -> Vec<EngineConfig> {
        let mut file = self.read().unwrap_or_else(|e| {
            log::warn!("engines.json unreadable: {e}");
            EnginesFile::default()
        });
        if !file.engines.iter().any(|e| e.id == LC0_ID) {
            file.engines.insert(0, EngineConfig::lc0());
        }
        file.engines
    }

    /// Like [`Registry::list`], but an unreadable file is moved aside (so it
    /// can be recovered) instead of being silently overwritten.
    fn list_for_write(&self) -> Result<Vec<EngineConfig>> {
        if let Err(e) = self.read() {
            let backup = self.path.with_extension(format!("json.broken-{}", chrono::Utc::now().timestamp()));
            std::fs::rename(&self.path, &backup)?;
            log::warn!("engines.json was unreadable ({e}); moved to {}", backup.display());
        }
        Ok(self.list())
    }

    pub fn get(&self, id: &str) -> Result<EngineConfig> {
        self.list().into_iter().find(|e| e.id == id).ok_or_else(|| Error::NotFound(format!("unknown engine: {id}")))
    }

    fn save(&self, engines: Vec<EngineConfig>) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&EnginesFile { engines })?)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }

    pub fn upsert(&self, config: EngineConfig) -> Result<()> {
        let _lock = self.write_lock.lock().unwrap();
        let mut all = self.list_for_write()?;
        match all.iter_mut().find(|e| e.id == config.id) {
            Some(existing) => *existing = config,
            None => all.push(config),
        }
        self.save(all)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        if id == LC0_ID {
            return Err(Error::other("the bundled engine can't be removed"));
        }
        let _lock = self.write_lock.lock().unwrap();
        let all = self.list_for_write()?.into_iter().filter(|e| e.id != id).collect();
        self.save(all)
    }

    /// Start an engine to check it speaks UCI, and record its name and options.
    pub async fn probe(path: &Path) -> Result<EngineConfig> {
        if !path.is_file() {
            return Err(Error::NotFound(format!("{} is not a file", path.display())));
        }
        let engine = Engine::spawn(path, &[]).await?;
        let name = engine.name.clone();
        let options = engine.options.clone();
        engine.quit().await;
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        Ok(EngineConfig {
            id: format!("{slug}-{}", chrono::Utc::now().timestamp_millis() % 100_000),
            name,
            kind: EngineKind::Uci,
            path: Some(path.to_string_lossy().into_owned()),
            options: Vec::new(),
            available_options: options,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lc0_is_always_listed_and_cannot_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let r = Registry::new(dir.path());
        assert_eq!(r.list().len(), 1);
        assert_eq!(r.list()[0].kind, EngineKind::Lc0);
        assert!(r.remove(LC0_ID).is_err());

        let sf = EngineConfig {
            id: "stockfish-1".into(),
            name: "Stockfish 17".into(),
            kind: EngineKind::Uci,
            path: Some("/x/stockfish".into()),
            options: vec![OptionValue { name: "Threads".into(), value: "4".into() }],
            available_options: vec![],
        };
        r.upsert(sf.clone()).unwrap();
        assert_eq!(r.list().len(), 2);
        assert_eq!(r.get("stockfish-1").unwrap().option("threads"), Some("4"));
        r.remove("stockfish-1").unwrap();
        assert_eq!(r.list().len(), 1);
    }

    #[test]
    fn a_broken_file_is_kept_aside_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let r = Registry::new(dir.path());
        std::fs::write(dir.path().join("engines.json"), b"{ not json").unwrap();
        assert_eq!(r.list().len(), 1, "only lc0 while the file is unreadable");
        r.upsert(EngineConfig {
            id: "x".into(),
            name: "X".into(),
            kind: EngineKind::Uci,
            path: None,
            options: vec![],
            available_options: vec![],
        })
        .unwrap();
        let backups =
            std::fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains("broken")).count();
        assert_eq!(backups, 1, "the unreadable file was moved aside");
        // Older files without the newer fields still load.
        std::fs::write(dir.path().join("engines.json"), br#"{"engines":[{"id":"sf","name":"SF","kind":"uci"}]}"#).unwrap();
        assert!(r.get("sf").is_ok());
    }
}
