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
    /// Running game reviews, by review id.
    pub reviews: Mutex<HashMap<String, Arc<ReviewJob>>>,
    pub http: reqwest::Client,
    pub explorer_cache: crate::net::explorer::ExplorerCache,
    /// Full-strength engine used for hints, kept warm between requests.
    helper: tokio::sync::Mutex<Option<(Lc0Config, Arc<Engine>)>>,
    downloads: Arc<Mutex<HashSet<String>>>,
    /// The microphone, for voice input.
    pub recorder: crate::voice::recorder::Recorder,
    /// Engines behind the coaches.
    pub coach: crate::coach::engine::CoachEngines,
    /// Cancel flags of running coach requests, by request id.
    pub coach_requests: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The OpenRouter key, read from the credential store once. `None` until
    /// read; `Some(None)` when no key is saved.
    ai_key: RwLock<Option<Option<String>>>,
}

/// A running game review: its cancel flag and, once started, its engine (so
/// app shutdown can stop it).
#[derive(Default)]
pub struct ReviewJob {
    pub cancel: AtomicBool,
    pub engine: Mutex<Option<Arc<Engine>>>,
}

/// Marks a download as running until dropped, so an error path can't leave
/// it stuck as "already downloading".
pub struct DownloadGuard {
    id: String,
    downloads: Arc<Mutex<HashSet<String>>>,
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.downloads.lock().unwrap().remove(&self.id);
    }
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
            downloads: Arc::new(Mutex::new(HashSet::new())),
            ai_key: RwLock::new(None),
            coach: Default::default(),
            recorder: Default::default(),
            coach_requests: Mutex::new(HashMap::new()),
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
                // No UCI_ShowWDL: Stockfish's win/draw/loss figures are
                // calibrated for engine play (+1 pawn is already a 50% win), so
                // its centipawns go through the human-calibrated curve instead,
                // as Lichess does. Otherwise a one-pawn slip reads as a blunder.
                let options: Vec<(String, String)> = config.options.iter().map(|o| (o.name.clone(), o.value.clone())).collect();
                Ok(EngineLaunch { key: format!("{path:?}{options:?}"), path, options })
            }
        }
    }

    /// Mark a download as running until the guard is dropped; `None` if it
    /// already is.
    pub fn begin_download(&self, id: &str) -> Option<DownloadGuard> {
        self.downloads
            .lock()
            .unwrap()
            .insert(id.to_string())
            .then(|| DownloadGuard { id: id.to_string(), downloads: self.downloads.clone() })
    }

    /// The engine for game reviews: the user's choice, else Stockfish when
    /// installed (much faster on a CPU), else the bundled lc0.
    pub fn review_engine_id(&self, requested: Option<&str>) -> String {
        let configured = self.settings().review_engine_id;
        let wanted = requested.map(str::to_string).or(configured);
        let engines = self.registry.list();
        if let Some(id) = wanted.filter(|id| engines.iter().any(|e| &e.id == id)) {
            return id;
        }
        if engines.iter().any(|e| e.id == crate::engine::install::STOCKFISH_ID) {
            return crate::engine::install::STOCKFISH_ID.into();
        }
        crate::engine::registry::LC0_ID.into()
    }

    /// Shut down every helper process that runs the engine `id`, before the
    /// engine is replaced or removed.
    pub async fn close_engine_users(&self, id: &str) {
        let reviews: Vec<Arc<ReviewJob>> = self.reviews.lock().unwrap().values().cloned().collect();
        for job in reviews {
            let engine = job.engine.lock().unwrap().clone();
            if engine.is_some_and(|e| e.path.to_string_lossy().contains(id)) {
                job.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        self.coach.close_engine(id).await;
    }

    /// The saved OpenRouter key, if any (cached after the first read).
    pub async fn ai_key(&self) -> Result<Option<String>> {
        if let Some(k) = self.ai_key.read().unwrap().clone() {
            return Ok(k);
        }
        let key = crate::commands::blocking(|| crate::secrets::get(crate::secrets::Secret::OpenRouterKey)).await?;
        *self.ai_key.write().unwrap() = Some(key.clone());
        Ok(key)
    }

    /// Forget the cached key after it was changed.
    pub fn set_cached_ai_key(&self, key: Option<String>) {
        *self.ai_key.write().unwrap() = Some(key);
    }

    /// An OpenRouter client, or a [`Error::NoApiKey`] the UI turns into an
    /// "Open AI settings" prompt.
    pub async fn openrouter(&self) -> Result<crate::ai::OpenRouter> {
        match self.ai_key().await? {
            Some(key) => Ok(crate::ai::OpenRouter::new(self.http.clone(), key)),
            None => Err(Error::NoApiKey("Add an OpenRouter API key in Settings → AI coach to use AI features.".into())),
        }
    }

    /// True when the coach should use AI for its words: a key is saved and
    /// AI is switched on.
    pub async fn ai_ready(&self) -> bool {
        self.settings().ai.enabled && self.ai_key().await.ok().flatten().is_some()
    }

    pub fn shutdown(&self) {
        self.play.shutdown();
        self.analysis.shutdown();
        self.coach.shutdown();
        for job in self.reviews.lock().unwrap().values() {
            job.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(engine) = job.engine.lock().unwrap().as_ref() {
                engine.kill();
            }
        }
        if let Ok(guard) = self.helper.try_lock() {
            if let Some((_, engine)) = guard.as_ref() {
                engine.kill();
            }
        }
    }
}
