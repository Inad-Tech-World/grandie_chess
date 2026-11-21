//! Live analysis: engines think about the position on the board and stream
//! their best lines to the UI. Scores are converted to White's point of view.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, Position};
use specta::Type;

use super::accuracy::win_percent;
use crate::engine::process::{position_command, Engine, SearchControl, SearchResult};
use crate::engine::uci::{InfoLine, Score, Wdl};
use crate::error::Result;
use crate::game::position::{fen_of, parse_fen, parse_uci, san_of};

const EMIT_EVERY: Duration = Duration::from_millis(150);
const PV_SAN_MOVES: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisLine {
    pub multipv: u32,
    pub depth: u32,
    pub seldepth: Option<u32>,
    pub nodes: f64,
    pub nps: Option<f64>,
    /// From White's point of view.
    pub score: Option<Score>,
    /// From White's point of view, per mille.
    pub wdl: Option<Wdl>,
    /// White's win percentage (0..100), from WDL or the score.
    pub white_win: Option<f64>,
    pub pv: Vec<String>,
    pub san: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisUpdate {
    pub tab_id: String,
    pub engine_id: String,
    /// Position being analysed (after `moves`).
    pub fen: String,
    pub lines: Vec<AnalysisLine>,
    /// True once the search has stopped.
    pub done: bool,
    /// Why the search stopped early, if it failed.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
    pub tab_id: String,
    pub engine_id: String,
    /// Start of the game; `moves` lead from it to the analysed position so the
    /// engine knows the history (repetitions, fifty-move counter).
    pub root_fen: String,
    pub moves: Vec<String>,
    pub multipv: u32,
}

pub trait AnalysisSink: Send + Sync + 'static {
    fn update(&self, update: AnalysisUpdate);
}

/// How to start an engine; `key` changes whenever a restart is needed.
#[derive(Debug, Clone)]
pub struct EngineLaunch {
    pub key: String,
    pub path: std::path::PathBuf,
    pub options: Vec<(String, String)>,
}

struct Slot {
    key: String,
    engine: Arc<Engine>,
}

/// One engine id's state. The token counts requests: a search only runs (and
/// keeps running) while its token is the newest one.
#[derive(Default)]
struct Entry {
    token: Arc<AtomicU64>,
    slot: tokio::sync::Mutex<Option<Slot>>,
}

#[derive(Default)]
pub struct AnalysisManager {
    entries: std::sync::Mutex<HashMap<String, Arc<Entry>>>,
}

/// Convert an engine line to White's point of view, with SAN for the PV.
pub fn to_white_line(info: &InfoLine, pos: &Chess) -> AnalysisLine {
    let black = pos.turn() == Color::Black;
    let score = info.score.map(|s| match (s, black) {
        (Score::Cp(c), true) => Score::Cp(-c),
        (Score::Mate(m), true) => Score::Mate(-m),
        (s, false) => s,
    });
    let wdl = info.wdl.map(|w| if black { w.flipped() } else { w });
    let mut san = Vec::new();
    let mut p = pos.clone();
    for uci in info.pv.iter().take(PV_SAN_MOVES) {
        let Ok(m) = parse_uci(&p, uci) else { break };
        san.push(san_of(&p, m));
        p.play_unchecked(m);
    }
    AnalysisLine {
        multipv: info.multipv.unwrap_or(1),
        depth: info.depth.unwrap_or(0),
        seldepth: info.seldepth,
        nodes: info.nodes.unwrap_or(0.0),
        nps: info.nps,
        score,
        wdl,
        white_win: win_percent(score, wdl),
        pv: info.pv.clone(),
        san,
    }
}

pub fn position_after(root_fen: &str, moves: &[String]) -> Result<Chess> {
    let mut pos = parse_fen(root_fen)?;
    for uci in moves {
        let m = parse_uci(&pos, uci)?;
        pos.play_unchecked(m);
    }
    Ok(pos)
}

impl AnalysisManager {
    fn entry(&self, engine_id: &str) -> Arc<Entry> {
        self.entries.lock().unwrap().entry(engine_id.to_string()).or_default().clone()
    }

    /// The running engine for an id, (re)started if its launch settings
    /// changed. Only callers for the same engine wait on a slow start.
    async fn engine_for(entry: &Entry, launch: &EngineLaunch) -> Result<Arc<Engine>> {
        let mut slot = entry.slot.lock().await;
        if let Some(s) = slot.as_ref() {
            if s.key == launch.key && s.engine.is_alive() {
                return Ok(s.engine.clone());
            }
        }
        if let Some(old) = slot.take() {
            old.engine.quit().await;
        }
        let engine = crate::engine::launch(&launch.path, &launch.options).await?;
        *slot = Some(Slot { key: launch.key.clone(), engine: engine.clone() });
        Ok(engine)
    }

    /// Start (or restart) infinite analysis of a position.
    pub async fn start(&self, launch: EngineLaunch, req: AnalysisRequest, sink: Arc<dyn AnalysisSink>) -> Result<()> {
        let pos = position_after(&req.root_fen, &req.moves)?;
        let entry = self.entry(&req.engine_id);
        let token = entry.token.clone();
        let my_token = token.fetch_add(1, Ordering::SeqCst) + 1;
        let engine = Self::engine_for(&entry, &launch).await?;
        if token.load(Ordering::SeqCst) != my_token {
            return Ok(()); // superseded while the engine was starting
        }
        // Stop the previous search now; it would also notice by itself.
        engine.stop().await?;
        let fen = fen_of(&pos);
        if pos.is_game_over() {
            sink.update(AnalysisUpdate { tab_id: req.tab_id, engine_id: req.engine_id, fen, lines: vec![], done: true, error: None });
            return Ok(());
        }
        let position = position_command(&req.root_fen, &req.moves);
        let ctl = {
            let token = token.clone();
            SearchControl {
                setup: vec![format!("setoption name MultiPV value {}", req.multipv.clamp(1, 8))],
                cancelled: Some(Box::new(move || token.load(Ordering::SeqCst) != my_token)),
                timeout: None,
            }
        };
        tokio::spawn(async move {
            let mut lines: Vec<AnalysisLine> = Vec::new();
            let mut last_emit = Instant::now() - EMIT_EVERY;
            let emit = |lines: &[AnalysisLine], done: bool, error: Option<String>| {
                sink.update(AnalysisUpdate {
                    tab_id: req.tab_id.clone(),
                    engine_id: req.engine_id.clone(),
                    fen: fen.clone(),
                    lines: lines.to_vec(),
                    done,
                    error,
                });
            };
            let result: Result<SearchResult> = engine
                .search_ctl(&position, "go infinite", &ctl, |info| {
                    if token.load(Ordering::SeqCst) != my_token {
                        return;
                    }
                    let line = to_white_line(info, &pos);
                    let idx = line.multipv.max(1) as usize - 1;
                    if lines.len() <= idx {
                        lines.resize(idx + 1, line.clone());
                    }
                    lines[idx] = line;
                    if last_emit.elapsed() >= EMIT_EVERY {
                        emit(&lines, false, None);
                        last_emit = Instant::now();
                    }
                })
                .await;
            if token.load(Ordering::SeqCst) == my_token {
                let error = result.err().map(|e| {
                    log::warn!("analysis stopped: {e}");
                    e.to_string()
                });
                emit(&lines, true, error);
            }
        });
        Ok(())
    }

    pub async fn stop(&self, engine_id: &str) -> Result<()> {
        let entry = self.entry(engine_id);
        entry.token.fetch_add(1, Ordering::SeqCst);
        // While the engine is still starting there is nothing to stop: the
        // bumped token keeps its search from beginning.
        let engine = entry.slot.try_lock().ok().and_then(|s| s.as_ref().map(|s| s.engine.clone()));
        if let Some(engine) = engine {
            engine.stop().await?;
        }
        Ok(())
    }

    /// Stop and quit an engine's process (it was removed or reconfigured).
    pub async fn close(&self, engine_id: &str) {
        let entry = self.entries.lock().unwrap().remove(engine_id);
        if let Some(entry) = entry {
            entry.token.fetch_add(1, Ordering::SeqCst);
            if let Some(slot) = entry.slot.lock().await.take() {
                slot.engine.quit().await;
            }
        }
    }

    /// Engine banner (e.g. backend in use) for an engine that has run.
    pub async fn backend_banner(&self, engine_id: &str) -> Option<String> {
        let entry = self.entry(engine_id);
        let slot = entry.slot.lock().await;
        slot.as_ref().and_then(|s| crate::engine::lc0::backend_in_use(&s.engine))
    }

    pub fn shutdown(&self) {
        for entry in self.entries.lock().unwrap().values() {
            entry.token.fetch_add(1, Ordering::SeqCst);
            if let Ok(slot) = entry.slot.try_lock() {
                if let Some(s) = slot.as_ref() {
                    s.engine.kill();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_black_to_move_lines_to_white_view() {
        let pos = position_after(crate::game::position::START_FEN, &["e2e4".into()]).unwrap();
        let info = InfoLine {
            depth: Some(10),
            score: Some(Score::Cp(-30)),
            wdl: Some(Wdl { w: 100, d: 700, l: 200 }),
            pv: vec!["e7e5".into(), "g1f3".into(), "zz".into()],
            ..Default::default()
        };
        let line = to_white_line(&info, &pos);
        assert_eq!(line.score, Some(Score::Cp(30)));
        assert_eq!(line.wdl, Some(Wdl { w: 200, d: 700, l: 100 }));
        assert_eq!(line.san, vec!["e5", "Nf3"], "SAN stops at the first bad move");
        assert!((line.white_win.unwrap() - 55.0).abs() < 1e-9);
    }
}
