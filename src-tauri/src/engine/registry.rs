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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct EnginesFile {
    engines: Vec<EngineConfig>,
}

pub struct Registry {
    path: PathBuf,
}

impl Registry {
    pub fn new(data_dir: &Path) -> Self {
        Registry { path: data_dir.join("engines.json") }
    }

    /// All engines, with the bundled lc0 always first.
    pub fn list(&self) -> Vec<EngineConfig> {
        let mut file: EnginesFile = std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if !file.engines.iter().any(|e| e.id == LC0_ID) {
            file.engines.insert(0, EngineConfig::lc0());
        }
        file.engines
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
        let mut all = self.list();
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
        let all = self.list().into_iter().filter(|e| e.id != id).collect();
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
}
