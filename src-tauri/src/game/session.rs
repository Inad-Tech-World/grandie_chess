//! Playing a game against an engine opponent. Rust owns the authoritative
//! game: moves, repetition history, clocks and the engine's turns. The UI
//! receives a [`PlaySnapshot`] after every change.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position};
use specta::Type;
use tokio::sync::Mutex;

use super::clock::{Clock, ClockState, TimeControl};
use super::pgn::write_pgn;
use super::position::{
    cannot_mate, fen_of, parse_fen, parse_uci, position_info, san_of, uci_of, EndReason, GameOutcome, PositionInfo, RepetitionHistory,
    Side, START_FEN,
};
use super::tree::GameTree;
use crate::engine::catalog::{network_path, resolve_full_network, Catalog, OpponentKind, OpponentLevel};
use crate::engine::lc0::{self, Lc0Config};
use crate::engine::process::{position_command, Engine};
use crate::engine::uci::Wdl;
use crate::error::{Error, Result};
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SideChoice {
    White,
    Black,
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaySetup {
    /// Opponent level id from the catalog, e.g. "maia-1500", "leela-3", "full".
    pub opponent: String,
    pub user_side: SideChoice,
    pub time_control: Option<TimeControl>,
    pub start_fen: Option<String>,
    pub allow_takebacks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayedMove {
    pub uci: String,
    pub san: String,
    pub fen: String,
    /// Mover's clock after the move (with increment), if the game is timed.
    pub clock_ms: Option<f64>,
    pub think_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaySnapshot {
    pub session_id: u32,
    pub setup: PlaySetup,
    pub level: OpponentLevel,
    pub network: String,
    pub user_side: Side,
    pub white_name: String,
    pub black_name: String,
    pub start_fen: String,
    pub moves: Vec<PlayedMove>,
    /// Current position. `dests` is empty unless it is the user's turn.
    pub position: PositionInfo,
    pub last_move: Option<String>,
    pub clock: Option<ClockState>,
    pub outcome: Option<GameOutcome>,
    pub engine_thinking: bool,
    pub hint: Option<String>,
    pub pgn_path: Option<String>,
    pub can_takeback: bool,
    pub can_offer_draw: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClockTick {
    pub session_id: u32,
    pub clock: ClockState,
}

/// Receives session updates; the app forwards them to the UI as events.
pub trait PlayObserver: Send + Sync + 'static {
    fn update(&self, snapshot: PlaySnapshot);
    fn clock(&self, tick: ClockTick);
}

/// Everything a session needs from the app, captured at game start.
pub struct PlayContext {
    pub observer: Arc<dyn PlayObserver>,
    pub settings: Settings,
    pub lc0_path: PathBuf,
    pub networks_dir: PathBuf,
    pub pgn_dir: PathBuf,
}

/// Plies the user must wait between draw offers.
const DRAW_OFFER_COOLDOWN: usize = 10;

pub struct PlaySession {
    id: u32,
    ctx: Arc<PlayContext>,
    setup: PlaySetup,
    level: OpponentLevel,
    network: String,
    user_side: Side,
    white_name: String,
    black_name: String,
    start_fen: String,
    start: Chess,
    pos: Chess,
    moves: Vec<PlayedMove>,
    history: RepetitionHistory,
    clock: Option<Clock>,
    outcome: Option<GameOutcome>,
    /// Bumped whenever an in-flight engine move must be discarded.
    generation: u64,
    engine: Arc<Engine>,
    engine_thinking: bool,
    /// Engine's last evaluation, from the engine's own point of view.
    engine_wdl: Option<Wdl>,
    hint: Option<String>,
    started_at: chrono::DateTime<chrono::Local>,
    pgn_path: Option<String>,
    last_draw_offer_ply: Option<usize>,
    error: Option<String>,
    closed: bool,
}

type SessionRef = Arc<Mutex<PlaySession>>;

impl PlaySession {
    fn engine_side(&self) -> Side {
        self.user_side.other()
    }

    fn turn(&self) -> Side {
        self.pos.turn().into()
    }

    fn is_users_turn(&self) -> bool {
        self.outcome.is_none() && self.turn() == self.user_side
    }

    pub fn snapshot(&self) -> PlaySnapshot {
        let now = Instant::now();
        let mut position = position_info(&self.pos);
        if !self.is_users_turn() || self.engine_thinking {
            position.dests.clear();
        }
        if position.outcome.is_none() {
            position.outcome = self.outcome.clone();
        }
        PlaySnapshot {
            session_id: self.id,
            setup: self.setup.clone(),
            level: self.level.clone(),
            network: self.network.clone(),
            user_side: self.user_side,
            white_name: self.white_name.clone(),
            black_name: self.black_name.clone(),
            start_fen: self.start_fen.clone(),
            moves: self.moves.clone(),
            position,
            last_move: self.moves.last().map(|m| m.uci.clone()),
            clock: self.clock.as_ref().map(|c| c.state(now)),
            outcome: self.outcome.clone(),
            engine_thinking: self.engine_thinking,
            hint: self.hint.clone(),
            pgn_path: self.pgn_path.clone(),
            can_takeback: self.setup.allow_takebacks
                && self.outcome.is_none()
                && self.moves.iter().enumerate().any(|(i, _)| self.mover_of(i) == self.user_side),
            can_offer_draw: self.outcome.is_none() && self.last_draw_offer_ply.is_none_or(|p| self.moves.len() >= p + DRAW_OFFER_COOLDOWN),
            error: self.error.clone(),
        }
    }

    fn emit(&self) {
        self.ctx.observer.update(self.snapshot());
    }

    fn mover_of(&self, index: usize) -> Side {
        let first: Side = self.start.turn().into();
        if index.is_multiple_of(2) {
            first
        } else {
            first.other()
        }
    }

    fn apply_move(&mut self, uci: &str, now: Instant) -> Result<()> {
        let m = parse_uci(&self.pos, uci)?;
        let san = san_of(&self.pos, m);
        let mover = self.turn();
        let mut think_ms = 0.0;
        let mut clock_ms = None;
        if let Some(clock) = &mut self.clock {
            if clock.running().is_none() {
                // The first move of the game is untimed; the opponent's clock starts now.
                clock.start(mover.other(), now);
            } else {
                think_ms = clock.press(mover, now) as f64;
            }
            clock_ms = Some(clock.remaining(mover, now).max(0) as f64);
        }
        self.pos.play_unchecked(m);
        self.history.push(m, &self.pos);
        self.moves.push(PlayedMove { uci: uci_of(m), san, fen: fen_of(&self.pos), clock_ms, think_ms });
        self.hint = None;
        if let Some(outcome) = self.history.outcome(&self.pos) {
            self.finish(outcome, now);
        }
        Ok(())
    }

    fn finish(&mut self, outcome: GameOutcome, now: Instant) {
        if self.outcome.is_some() {
            return;
        }
        self.generation += 1;
        if let Some(clock) = &mut self.clock {
            clock.stop(now);
        }
        self.engine_thinking = false;
        let save = outcome.reason != EndReason::Aborted && !self.moves.is_empty();
        self.outcome = Some(outcome);
        let engine = self.engine.clone();
        tokio::spawn(async move { engine.quit().await });
        if save {
            match self.save_pgn() {
                Ok(path) => self.pgn_path = Some(path),
                Err(e) => self.error = Some(format!("could not save the game: {e}")),
            }
        }
    }

    /// The game as a PGN tree with headers.
    pub fn game_tree(&self) -> Result<GameTree> {
        let start_fen = (self.start_fen != START_FEN).then_some(self.start_fen.as_str());
        let mut tree = GameTree::new(start_fen)?;
        let mut parent = 0;
        for m in &self.moves {
            parent = tree.add_applied(parent, &m.uci, &m.san, m.fen.clone());
            tree.nodes[parent as usize].clock_ms = m.clock_ms;
        }
        let result = self.outcome.as_ref().map(|o| o.result.clone()).unwrap_or_else(|| "*".into());
        tree.set_header("Event", "Grandie Chess training game");
        tree.set_header("Site", "Grandie Chess");
        tree.set_header("Date", self.started_at.format("%Y.%m.%d").to_string());
        tree.set_header("Round", "-");
        tree.set_header("White", &self.white_name);
        tree.set_header("Black", &self.black_name);
        tree.set_header("Result", result);
        if let Some(elo) = self.level.elo {
            let tag = match self.engine_side() {
                Side::White => "WhiteElo",
                Side::Black => "BlackElo",
            };
            tree.set_header(tag, elo.to_string());
        }
        tree.set_header("TimeControl", self.setup.time_control.map(|tc| tc.pgn_tag()).unwrap_or_else(|| "-".into()));
        if self.start_fen != START_FEN {
            tree.set_header("SetUp", "1");
            tree.set_header("FEN", &self.start_fen);
        }
        if let Some(o) = &self.outcome {
            tree.set_header("Termination", termination(o.reason));
        }
        tree.set_header("StartTime", self.started_at.format("%H:%M:%S").to_string());
        tree.set_header("GrandieOpponent", &self.level.id);
        tree.set_header("GrandieNetwork", &self.network);
        tree.set_header(
            "GrandieUserSide",
            match self.user_side {
                Side::White => "white",
                Side::Black => "black",
            },
        );
        Ok(tree)
    }

    fn save_pgn(&self) -> Result<String> {
        let tree = self.game_tree()?;
        let name = format!("{}_{}.pgn", self.started_at.format("%Y-%m-%d_%H%M%S"), self.level.id);
        let path = self.ctx.pgn_dir.join(name);
        std::fs::write(&path, write_pgn(&tree))?;
        Ok(path.to_string_lossy().into_owned())
    }

    fn go_command(&self, now: Instant) -> String {
        if let Some(nodes) = self.level.nodes {
            return format!("go nodes {nodes}");
        }
        match (&self.clock, self.setup.time_control) {
            (Some(clock), Some(tc)) if clock.running().is_some() => format!(
                "go wtime {} btime {} winc {} binc {}",
                clock.remaining(Side::White, now).max(1),
                clock.remaining(Side::Black, now).max(1),
                tc.increment_ms,
                tc.increment_ms
            ),
            // The untimed first move of a timed game, and untimed games.
            _ => "go movetime 2000".into(),
        }
    }

    /// A short pause so low-strength opponents don't answer instantly.
    fn human_delay(&self, now: Instant) -> Option<Duration> {
        if !self.ctx.settings.human_think_time || self.level.kind == OpponentKind::Full {
            return None;
        }
        let (lo, hi) = match self.setup.time_control {
            None => (600, 2_000),
            Some(tc) if tc.initial_ms >= 600_000 => (1_000, 4_000),
            Some(tc) if tc.initial_ms >= 180_000 => (500, 2_500),
            Some(_) => (200, 900),
        };
        let mut ms = rand::random_range(lo..=hi) as i64;
        if self.moves.len() < 16 {
            ms /= 2;
        }
        if let Some(clock) = &self.clock {
            let left = clock.remaining(self.engine_side(), now);
            ms = ms.min(left / 30);
        }
        (ms > 0).then(|| Duration::from_millis(ms as u64))
    }

    fn rebuild_from_moves(&mut self) -> Result<()> {
        let mut pos = self.start.clone();
        let mut history = RepetitionHistory::new(&pos);
        for m in &self.moves {
            let mv = parse_uci(&pos, &m.uci)?;
            pos.play_unchecked(mv);
            history.push(mv, &pos);
        }
        self.pos = pos;
        self.history = history;
        Ok(())
    }
}

fn termination(reason: EndReason) -> &'static str {
    match reason {
        EndReason::Timeout | EndReason::TimeoutVsInsufficientMaterial => "Time forfeit",
        EndReason::Aborted => "Abandoned",
        _ => "Normal",
    }
}

#[derive(Default)]
pub struct PlayManager {
    sessions: StdMutex<HashMap<u32, SessionRef>>,
    next_id: AtomicU32,
}

impl PlayManager {
    fn get(&self, id: u32) -> Result<SessionRef> {
        self.sessions.lock().unwrap().get(&id).cloned().ok_or_else(|| Error::NotFound(format!("no game with id {id}")))
    }

    /// Networks a level needs that are not installed yet.
    pub fn missing_networks(catalog: &Catalog, settings: &Settings, networks_dir: &std::path::Path, opponent: &str) -> Result<Vec<String>> {
        let level = catalog.level(opponent)?;
        Ok(match level.kind {
            OpponentKind::Full => {
                if resolve_full_network(catalog, networks_dir, settings.default_network.as_deref()).is_some() {
                    vec![]
                } else {
                    vec![settings.default_network.clone().unwrap_or_else(|| "791556".into())]
                }
            }
            _ => level.network.iter().filter(|id| !network_path(networks_dir, id).exists()).cloned().collect(),
        })
    }

    pub async fn start(&self, ctx: Arc<PlayContext>, catalog: &Catalog, setup: PlaySetup) -> Result<PlaySnapshot> {
        let level = catalog.level(&setup.opponent)?.clone();
        let network = match level.kind {
            OpponentKind::Full => resolve_full_network(catalog, &ctx.networks_dir, ctx.settings.default_network.as_deref())
                .ok_or_else(|| Error::NotFound("No full-strength network is installed".into()))?,
            _ => level.network.clone().expect("non-full levels name a network"),
        };
        let weights = network_path(&ctx.networks_dir, &network);
        let config = Lc0Config::for_level(&level, weights, &ctx.settings);
        let engine = lc0::start(&ctx.lc0_path, &config).await?;
        engine.send("ucinewgame").await?;
        engine.ready(lc0::READY_TIMEOUT).await?;

        let start = match setup.start_fen.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            Some(fen) => parse_fen(fen)?,
            None => Chess::default(),
        };
        if start.is_game_over() {
            return Err(Error::Chess("that position is already finished".into()));
        }
        let user_side = match setup.user_side {
            SideChoice::White => Side::White,
            SideChoice::Black => Side::Black,
            SideChoice::Random => {
                if rand::random_bool(0.5) {
                    Side::White
                } else {
                    Side::Black
                }
            }
        };
        let opponent_name = match level.kind {
            OpponentKind::Full => format!("Leela ({network})"),
            _ => level.name.clone(),
        };
        let user_name = ctx.settings.player_name.clone();
        let (white_name, black_name) = match user_side {
            Side::White => (user_name, opponent_name),
            Side::Black => (opponent_name, user_name),
        };
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let session = PlaySession {
            id,
            ctx: ctx.clone(),
            level,
            network,
            user_side,
            white_name,
            black_name,
            start_fen: fen_of(&start),
            history: RepetitionHistory::new(&start),
            pos: start.clone(),
            start,
            moves: Vec::new(),
            clock: setup.time_control.map(Clock::new),
            outcome: None,
            generation: 0,
            engine,
            engine_thinking: false,
            engine_wdl: None,
            hint: None,
            started_at: chrono::Local::now(),
            pgn_path: None,
            last_draw_offer_ply: None,
            error: None,
            closed: false,
            setup,
        };
        let snapshot = session.snapshot();
        let has_clock = session.clock.is_some();
        let engine_first = !session.is_users_turn();
        let session = Arc::new(Mutex::new(session));
        self.sessions.lock().unwrap().insert(id, session.clone());
        ctx.observer.update(snapshot.clone());
        if has_clock {
            spawn_ticker(session.clone());
        }
        if engine_first {
            spawn_engine_move(session);
        }
        Ok(snapshot)
    }

    pub async fn snapshot(&self, id: u32) -> Result<PlaySnapshot> {
        Ok(self.get(id)?.lock().await.snapshot())
    }

    pub async fn user_move(&self, id: u32, uci: &str) -> Result<PlaySnapshot> {
        let session = self.get(id)?;
        let mut s = session.lock().await;
        if s.outcome.is_some() {
            return Err(Error::Chess("the game is over".into()));
        }
        if !s.is_users_turn() || s.engine_thinking {
            return Err(Error::Chess("it is not your turn".into()));
        }
        s.apply_move(uci, Instant::now())?;
        s.emit();
        let snapshot = s.snapshot();
        let engine_next = s.outcome.is_none();
        drop(s);
        if engine_next {
            spawn_engine_move(session);
        }
        Ok(snapshot)
    }

    pub async fn takeback(&self, id: u32) -> Result<PlaySnapshot> {
        let session = self.get(id)?;
        let mut s = session.lock().await;
        if !s.setup.allow_takebacks {
            return Err(Error::Chess("takebacks are off for this game".into()));
        }
        if s.outcome.is_some() {
            return Err(Error::Chess("the game is over".into()));
        }
        if !s.moves.iter().enumerate().any(|(i, _)| s.mover_of(i) == s.user_side) {
            return Err(Error::Chess("nothing to take back".into()));
        }
        s.generation += 1;
        if s.engine_thinking {
            let engine = s.engine.clone();
            tokio::spawn(async move {
                let _ = engine.stop().await;
            });
        }
        s.engine_thinking = false;
        loop {
            let index = s.moves.len() - 1;
            let mover = s.mover_of(index);
            s.moves.pop();
            if mover == s.user_side {
                break;
            }
        }
        s.rebuild_from_moves()?;
        let now = Instant::now();
        let turn = s.turn();
        if let Some(tc) = s.setup.time_control {
            if s.moves.is_empty() {
                s.clock = Some(Clock::new(tc));
            } else if let Some(clock) = &mut s.clock {
                clock.start(turn, now);
            }
        }
        s.hint = None;
        s.emit();
        Ok(s.snapshot())
    }

    pub async fn resign(&self, id: u32) -> Result<PlaySnapshot> {
        let session = self.get(id)?;
        let mut s = session.lock().await;
        if s.outcome.is_none() {
            let winner = s.engine_side();
            s.finish(GameOutcome::win(winner, EndReason::Resignation), Instant::now());
            s.emit();
        }
        Ok(s.snapshot())
    }

    /// Offer a draw. The engine accepts once the game is past move 20 and it
    /// doesn't think it is clearly better.
    pub async fn offer_draw(&self, id: u32) -> Result<bool> {
        let session = self.get(id)?;
        let mut s = session.lock().await;
        if s.outcome.is_some() {
            return Ok(false);
        }
        let ply = s.moves.len();
        if s.last_draw_offer_ply.is_some_and(|p| ply < p + DRAW_OFFER_COOLDOWN) {
            return Ok(false);
        }
        s.last_draw_offer_ply = Some(ply);
        let accept = s.pos.fullmoves().get() >= 20 && s.engine_wdl.is_some_and(|w| w.expected_score() <= 0.55);
        if accept {
            s.finish(GameOutcome::draw(EndReason::Agreement), Instant::now());
        }
        s.emit();
        Ok(accept)
    }

    pub async fn set_hint(&self, id: u32, hint: Option<String>) -> Result<()> {
        let session = self.get(id)?;
        let mut s = session.lock().await;
        s.hint = hint;
        s.emit();
        Ok(())
    }

    /// Start fen and moves of the current position, for asking another engine.
    pub async fn position(&self, id: u32) -> Result<(String, Vec<String>, bool)> {
        let session = self.get(id)?;
        let s = session.lock().await;
        Ok((s.start_fen.clone(), s.moves.iter().map(|m| m.uci.clone()).collect(), s.is_users_turn()))
    }

    pub async fn pgn(&self, id: u32) -> Result<String> {
        let session = self.get(id)?;
        let s = session.lock().await;
        Ok(write_pgn(&s.game_tree()?))
    }

    /// Close a game tab. An unfinished game is abandoned and not saved.
    pub async fn close(&self, id: u32) -> Result<()> {
        let Some(session) = self.sessions.lock().unwrap().remove(&id) else { return Ok(()) };
        let mut s = session.lock().await;
        s.closed = true;
        if s.outcome.is_none() {
            s.generation += 1;
            s.outcome = Some(GameOutcome::aborted());
            let engine = s.engine.clone();
            tokio::spawn(async move { engine.quit().await });
        }
        Ok(())
    }

    /// Kill every engine immediately (app shutdown).
    pub fn shutdown(&self) {
        for session in self.sessions.lock().unwrap().values() {
            if let Ok(s) = session.try_lock() {
                s.engine.kill();
            }
        }
    }
}

fn spawn_engine_move(session: SessionRef) {
    tokio::spawn(async move {
        if let Err(e) = engine_move(&session).await {
            log::error!("engine move failed: {e}");
            let mut s = session.lock().await;
            if s.outcome.is_none() {
                s.error = Some(e.to_string());
                s.finish(GameOutcome::aborted(), Instant::now());
            }
            s.emit();
        }
    });
}

async fn engine_move(session: &SessionRef) -> Result<()> {
    let (engine, generation, position, go, delay) = {
        let mut s = session.lock().await;
        if s.outcome.is_some() || s.is_users_turn() || s.closed {
            return Ok(());
        }
        let now = Instant::now();
        s.engine_thinking = true;
        s.emit();
        let moves: Vec<String> = s.moves.iter().map(|m| m.uci.clone()).collect();
        (s.engine.clone(), s.generation, position_command(&s.start_fen, &moves), s.go_command(now), s.human_delay(now))
    };
    let started = Instant::now();
    let mut last_wdl = None;
    let result = engine
        .search(&position, &go, |info| {
            if info.wdl.is_some() {
                last_wdl = info.wdl;
            }
        })
        .await;
    if let (Some(delay), Ok(_)) = (delay, &result) {
        if let Some(rest) = delay.checked_sub(started.elapsed()) {
            tokio::time::sleep(rest).await;
        }
    }
    let mut s = session.lock().await;
    if s.generation != generation || s.outcome.is_some() || s.closed {
        return Ok(());
    }
    let result = result?;
    s.engine_thinking = false;
    if last_wdl.is_some() {
        s.engine_wdl = last_wdl;
    }
    s.apply_move(&result.best, Instant::now()).map_err(|e| Error::Engine(format!("engine played an illegal move {}: {e}", result.best)))?;
    s.emit();
    Ok(())
}

fn spawn_ticker(session: SessionRef) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            interval.tick().await;
            let mut s = session.lock().await;
            if s.outcome.is_some() || s.closed {
                break;
            }
            let now = Instant::now();
            let Some(clock) = &s.clock else { break };
            if clock.running().is_none() {
                continue;
            }
            if let Some(flagged) = clock.flagged(now) {
                let winner = flagged.other();
                let outcome = if cannot_mate(&s.pos, winner) {
                    GameOutcome::draw(EndReason::TimeoutVsInsufficientMaterial)
                } else {
                    GameOutcome::win(winner, EndReason::Timeout)
                };
                s.finish(outcome, now);
                s.emit();
                break;
            }
            let tick = ClockTick { session_id: s.id, clock: clock.state(now) };
            s.ctx.observer.clock(tick);
        }
    });
}
