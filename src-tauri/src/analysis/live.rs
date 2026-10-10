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
use crate::engine::process::{position_command, Engine, SearchResult};
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
    token: Arc<AtomicU64>,
}

#[derive(Default)]
pub struct AnalysisManager {
    slots: tokio::sync::Mutex<HashMap<String, Slot>>,
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
    async fn engine_for(&self, engine_id: &str, launch: &EngineLaunch) -> Result<(Arc<Engine>, Arc<AtomicU64>)> {
        let mut slots = self.slots.lock().await;
        if let Some(slot) = slots.get(engine_id) {
            if slot.key == launch.key && slot.engine.is_alive() {
                return Ok((slot.engine.clone(), slot.token.clone()));
            }
        }
        if let Some(old) = slots.remove(engine_id) {
            old.token.fetch_add(1, Ordering::SeqCst);
            old.engine.quit().await;
        }
        let engine = Engine::spawn(&launch.path, &[]).await?;
        for (name, value) in &launch.options {
            engine.set_option(name, value).await?;
        }
        engine.ready(crate::engine::lc0::READY_TIMEOUT).await?;
        let token = Arc::new(AtomicU64::new(0));
        slots.insert(
            engine_id.to_string(),
            Slot { key: launch.key.clone(), engine: engine.clone(), token: token.clone() },
        );
        Ok((engine, token))
    }

    /// Start (or restart) infinite analysis of a position.
    pub async fn start(&self, launch: EngineLaunch, req: AnalysisRequest, sink: Arc<dyn AnalysisSink>) -> Result<()> {
        let pos = position_after(&req.root_fen, &req.moves)?;
        let (engine, token) = self.engine_for(&req.engine_id, &launch).await?;
        let my_token = token.fetch_add(1, Ordering::SeqCst) + 1;
        engine.stop().await?;
        let fen = fen_of(&pos);
        if pos.is_game_over() {
            sink.update(AnalysisUpdate { tab_id: req.tab_id, engine_id: req.engine_id, fen, lines: vec![], done: true });
            return Ok(());
        }
        let setup = vec![format!("setoption name MultiPV value {}", req.multipv.clamp(1, 8))];
        let position = position_command(&req.root_fen, &req.moves);
        tokio::spawn(async move {
            let mut lines: Vec<AnalysisLine> = Vec::new();
            let mut last_emit = Instant::now() - EMIT_EVERY;
            let emit = |lines: &[AnalysisLine], done: bool| {
                sink.update(AnalysisUpdate {
                    tab_id: req.tab_id.clone(),
                    engine_id: req.engine_id.clone(),
                    fen: fen.clone(),
                    lines: lines.to_vec(),
                    done,
                });
            };
            let result: Result<SearchResult> = engine
                .search_with(&setup, &position, "go infinite", |info| {
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
                        emit(&lines, false);
                        last_emit = Instant::now();
                    }
                })
                .await;
            if token.load(Ordering::SeqCst) == my_token {
                if let Err(e) = &result {
                    log::warn!("analysis stopped: {e}");
                }
                emit(&lines, true);
            }
        });
        Ok(())
    }

    pub async fn stop(&self, engine_id: &str) -> Result<()> {
        let slots = self.slots.lock().await;
        if let Some(slot) = slots.get(engine_id) {
            slot.token.fetch_add(1, Ordering::SeqCst);
            slot.engine.stop().await?;
        }
        Ok(())
    }

    /// Engine banner (e.g. backend in use) for an engine that has run.
    pub async fn backend_banner(&self, engine_id: &str) -> Option<String> {
        let slots = self.slots.lock().await;
        slots.get(engine_id).and_then(|s| crate::engine::lc0::backend_in_use(&s.engine))
    }

    pub fn shutdown(&self) {
        if let Ok(slots) = self.slots.try_lock() {
            for slot in slots.values() {
                slot.engine.kill();
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
