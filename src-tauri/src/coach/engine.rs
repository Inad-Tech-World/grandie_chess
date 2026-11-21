//! The coaches and their engines. Each coach (Stockfish, or Leela in the
//! AlphaZero style) keeps one engine process warm, separate from live
//! analysis and games, so explaining a move never disturbs them.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use shakmaty::Position;
use specta::Type;

use crate::analysis::live::{position_after, to_white_line, AnalysisLine, EngineLaunch};
use crate::engine::process::{Engine, SearchControl};
use crate::engine::registry::EngineKind;
use crate::error::{Error, Result};
use crate::game::position::{fen_of, parse_fen};
use crate::state::AppState;

pub const STOCKFISH_COACH: &str = "stockfish";
pub const LEELA_COACH: &str = "leela";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoachDef {
    pub id: String,
    pub name: String,
    /// Registry id of the engine behind the coach.
    pub engine_id: String,
    pub tagline: String,
    /// How the coach talks (used in the AI prompt).
    pub persona: String,
}

/// A coach as listed in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoachInfo {
    pub id: String,
    pub name: String,
    pub tagline: String,
    /// Its engine is installed (Stockfish needs a download).
    pub available: bool,
    /// This is the coach used when none is chosen explicitly.
    pub default: bool,
}

pub fn coach_defs() -> Vec<CoachDef> {
    serde_json::from_str(include_str!("../../resources/coaches.json")).expect("coaches.json is valid")
}

/// An engine's view of a position: lines from White's point of view.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoachAnalysis {
    pub fen: String,
    pub lines: Vec<AnalysisLine>,
}

impl CoachAnalysis {
    pub fn best_uci(&self) -> Option<&str> {
        self.lines.first().and_then(|l| l.pv.first()).map(String::as_str)
    }

    /// White's win% of a line, defaulting to equal.
    pub fn white_win(&self, index: usize) -> f64 {
        self.lines.get(index).and_then(|l| l.white_win).unwrap_or(50.0)
    }
}

const CACHE_SIZE: usize = 512;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct CoachEngines {
    engines: tokio::sync::Mutex<HashMap<String, (String, Arc<Engine>)>>,
    cache: Mutex<(HashMap<String, CoachAnalysis>, VecDeque<String>)>,
}

impl CoachEngines {
    /// The coach to use: the requested one, else the setting, else Stockfish
    /// when installed, else Leela.
    pub fn resolve(&self, state: &AppState, requested: Option<&str>) -> Result<CoachDef> {
        let defs = coach_defs();
        let installed = |d: &CoachDef| state.registry.get(&d.engine_id).is_ok();
        let wanted = requested.map(str::to_string).or_else(|| state.settings().coach.coach_id);
        if let Some(id) = wanted {
            if let Some(d) = defs.iter().find(|d| d.id == id) {
                if installed(d) {
                    return Ok(d.clone());
                }
                if requested.is_some() {
                    return Err(Error::NotFound(format!("{} isn't installed yet. Download it in Engines.", d.name)));
                }
            }
        }
        defs.iter()
            .find(|d| d.id == STOCKFISH_COACH && installed(d))
            .or_else(|| defs.iter().find(|d| d.id == LEELA_COACH))
            .cloned()
            .ok_or_else(|| Error::NotFound("no coach is available".into()))
    }

    pub fn list(&self, state: &AppState) -> Vec<CoachInfo> {
        let default = self.resolve(state, None).map(|d| d.id).unwrap_or_default();
        coach_defs()
            .into_iter()
            .map(|d| CoachInfo {
                available: state.registry.get(&d.engine_id).is_ok(),
                default: d.id == default,
                id: d.id,
                name: d.name,
                tagline: d.tagline,
            })
            .collect()
    }

    async fn engine(&self, coach: &CoachDef, launch: &EngineLaunch) -> Result<Arc<Engine>> {
        let mut engines = self.engines.lock().await;
        if let Some((key, engine)) = engines.get(&coach.id) {
            if *key == launch.key && engine.is_alive() {
                return Ok(engine.clone());
            }
        }
        if let Some((_, old)) = engines.remove(&coach.id) {
            old.quit().await;
        }
        let engine = crate::engine::launch(&launch.path, &launch.options).await?;
        engines.insert(coach.id.clone(), (launch.key.clone(), engine.clone()));
        Ok(engine)
    }

    /// Analyse a position (with `multipv` lines), cached by coach and FEN.
    pub async fn analyse(&self, state: &AppState, coach: &CoachDef, fen: &str, multipv: u32) -> Result<CoachAnalysis> {
        let pos = parse_fen(fen)?;
        let fen = fen_of(&pos);
        let key = format!("{}|{multipv}|{fen}", coach.id);
        if let Some(hit) = self.cache.lock().unwrap().0.get(&key) {
            return Ok(hit.clone());
        }
        let config = state.registry.get(&coach.engine_id)?;
        let launch = state.launch_for(&config)?;
        let go = match config.kind {
            EngineKind::Lc0 => "go nodes 800",
            EngineKind::Uci => "go depth 18 movetime 1500",
        };
        let result = if pos.is_game_over() {
            CoachAnalysis { fen: fen.clone(), lines: vec![] }
        } else {
            let engine = self.engine(coach, &launch).await?;
            let ctl = SearchControl {
                setup: vec![format!("setoption name MultiPV value {}", multipv.clamp(1, 5))],
                timeout: Some(SEARCH_TIMEOUT),
                ..Default::default()
            };
            let search = engine.search_ctl(&format!("position fen {fen}"), go, &ctl, |_| {}).await?;
            let after = position_after(&fen, &[])?;
            CoachAnalysis {
                fen: fen.clone(),
                lines: search.lines.iter().filter(|l| !l.pv.is_empty()).map(|l| to_white_line(l, &after)).collect(),
            }
        };
        let mut cache = self.cache.lock().unwrap();
        if cache.1.len() >= CACHE_SIZE {
            if let Some(old) = cache.1.pop_front() {
                cache.0.remove(&old);
            }
        }
        cache.1.push_back(key.clone());
        cache.0.insert(key, result.clone());
        Ok(result)
    }

    /// Quit the engine behind a coach (it is being replaced or removed).
    pub async fn close_engine(&self, engine_id: &str) {
        let defs = coach_defs();
        let mut engines = self.engines.lock().await;
        for d in defs.iter().filter(|d| d.engine_id == engine_id) {
            if let Some((_, engine)) = engines.remove(&d.id) {
                engine.quit().await;
            }
        }
    }

    pub fn shutdown(&self) {
        if let Ok(engines) = self.engines.try_lock() {
            for (_, engine) in engines.values() {
                engine.kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_coaches_are_defined() {
        let defs = coach_defs();
        assert!(defs.iter().any(|d| d.id == STOCKFISH_COACH && d.engine_id == "stockfish"));
        let leela = defs.iter().find(|d| d.id == LEELA_COACH).unwrap();
        assert_eq!(leela.engine_id, crate::engine::registry::LC0_ID);
        assert!(leela.name.contains("AlphaZero-style"), "never just \"AlphaZero\"");
    }
}
