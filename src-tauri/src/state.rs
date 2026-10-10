use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use crate::analysis::live::{AnalysisManager, EngineLaunch};
use crate::db::import::UserIdentity;
use crate::db::DbPool;
use crate::engine::catalog::{network_path, resolve_full_network, Catalog};
use crate::engine::lc0::{self, Lc0Config};
use crate::engine::process::Engine;
use crate::engine::registry::{EngineConfig, EngineKind, Registry};
use crate::error::{Error, Result};
use crate::game::session::PlayManager;
use crate::net::download::http_client;
use crate::paths::{bundled_lc0, AppPaths};
use crate::settings::Settings;

/// Shared state behind every command.
pub struct AppState {
    pub paths: AppPaths,
    settings: RwLock<Settings>,
    pub catalog: Catalog,
    pub play: PlayManager,
    pub registry: Registry,
    pub db: DbPool,
    pub puzzles: DbPool,
    pub analysis: AnalysisManager,
    /// Cancel flags of running game reviews, by review id.
    pub reviews: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub http: reqwest::Client,
    pub explorer_cache: crate::net::explorer::ExplorerCache,
    /// Full-strength engine used for hints, kept warm between requests.
    helper: tokio::sync::Mutex<Option<(Lc0Config, Arc<Engine>)>>,
    downloads: Mutex<HashSet<String>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let paths = AppPaths::new(data_dir)?;
        let settings = Settings::load(&paths.settings_file);
        Ok(AppState {
            db: crate::db::open(&paths.db_file)?,
            puzzles: crate::training::puzzles::open(&paths.puzzles_db_file)?,
            registry: Registry::new(&paths.data_dir),
            analysis: AnalysisManager::default(),
            reviews: Mutex::new(HashMap::new()),
            paths,
            settings: RwLock::new(settings),
            catalog: Catalog::load(),
            play: PlayManager::default(),
            http: http_client(),
            explorer_cache: Default::default(),
            helper: tokio::sync::Mutex::new(None),
            downloads: Mutex::new(HashSet::new()),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn set_settings(&self, settings: Settings) -> Result<()> {
        settings.save(&self.paths.settings_file)?;
        *self.settings.write().unwrap() = settings;
        Ok(())
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut s = self.settings();
        f(&mut s);
        self.set_settings(s.clone())?;
        Ok(s)
    }

    /// Names that mark a game as the user's own.
    pub fn identity(&self) -> UserIdentity {
        let s = self.settings();
        UserIdentity::new([Some(s.player_name), s.lichess_username, s.chesscom_username])
    }

    pub fn lc0_path(&self) -> Result<PathBuf> {
        bundled_lc0().ok_or_else(|| Error::NotFound("The lc0 engine is missing from this build. Run engine/build-lc0.sh.".into()))
    }

    pub fn full_network(&self) -> Result<String> {
        let settings = self.settings();
        resolve_full_network(&self.catalog, &self.paths.networks_dir, settings.default_network.as_deref())
            .ok_or_else(|| Error::NotFound("No full-strength network is installed".into()))
    }

    /// The full-strength helper engine, restarted if settings changed.
    pub async fn helper_engine(&self) -> Result<Arc<Engine>> {
        let network = self.full_network()?;
        let config = Lc0Config::full(network_path(&self.paths.networks_dir, &network), &self.settings());
        let mut guard = self.helper.lock().await;
        if let Some((current, engine)) = guard.as_ref() {
            if *current == config && engine.is_alive() {
                return Ok(engine.clone());
            }
        }
        if let Some((_, old)) = guard.take() {
            old.quit().await;
        }
        let engine = lc0::start(&self.lc0_path()?, &config).await?;
        *guard = Some((config, engine.clone()));
        Ok(engine)
    }

    /// How to start an engine from the registry for analysis or review.
    pub fn launch_for(&self, config: &EngineConfig) -> Result<EngineLaunch> {
        match config.kind {
            EngineKind::Lc0 => {
                let network = self.full_network()?;
                let lc0_config = Lc0Config::full(network_path(&self.paths.networks_dir, &network), &self.settings());
                let mut options = lc0_config.commands();
                options.extend(config.options.iter().map(|o| (o.name.clone(), o.value.clone())));
                Ok(EngineLaunch { key: format!("{options:?}"), path: self.lc0_path()?, options })
            }
            EngineKind::Uci => {
                let path = PathBuf::from(config.path.clone().ok_or_else(|| Error::other("engine has no path"))?);
                let mut options: Vec<(String, String)> = config.options.iter().map(|o| (o.name.clone(), o.value.clone())).collect();
                if config.available_options.iter().any(|o| o.name == "UCI_ShowWDL") {
                    options.push(("UCI_ShowWDL".into(), "true".into()));
                }
                Ok(EngineLaunch { key: format!("{path:?}{options:?}"), path, options })
            }
        }
    }

    /// Mark a download as running; false if it already is.
    pub fn begin_download(&self, id: &str) -> bool {
        self.downloads.lock().unwrap().insert(id.to_string())
    }

    pub fn end_download(&self, id: &str) {
        self.downloads.lock().unwrap().remove(id);
    }

    pub fn shutdown(&self) {
        self.play.shutdown();
        self.analysis.shutdown();
        if let Ok(guard) = self.helper.try_lock() {
            if let Some((_, engine)) = guard.as_ref() {
                engine.kill();
            }
        }
    }
}
