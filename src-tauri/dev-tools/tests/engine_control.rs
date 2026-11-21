//! Engine control against the scripted fake engine: restarting live
//! analysis quickly never leaves a search running unstopped, searches time
//! out instead of hanging, and a full review runs end to end.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grandie_chess_lib::analysis::live::{AnalysisManager, AnalysisRequest, AnalysisSink, AnalysisUpdate, EngineLaunch};
use grandie_chess_lib::analysis::review::{self, ReviewDepth};
use grandie_chess_lib::engine::process::{Engine, SearchControl};
use grandie_chess_lib::game::position::START_FEN;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-uci");

#[derive(Default)]
struct Sink(Mutex<Vec<AnalysisUpdate>>);

impl AnalysisSink for Sink {
    fn update(&self, update: AnalysisUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

fn request(moves: &[&str]) -> AnalysisRequest {
    AnalysisRequest {
        tab_id: "t".into(),
        engine_id: "fake".into(),
        root_fen: START_FEN.into(),
        moves: moves.iter().map(|m| m.to_string()).collect(),
        multipv: 1,
    }
}

fn launch() -> EngineLaunch {
    EngineLaunch { key: "fake".into(), path: FAKE.into(), options: vec![] }
}

async fn wait_for(sink: &Sink, what: impl Fn(&[AnalysisUpdate]) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if what(&sink.0.lock().unwrap()) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn rapid_restarts_end_with_the_newest_position_and_stop_cleanly() {
    let manager = AnalysisManager::default();
    let sink = Arc::new(Sink::default());
    // Five positions in quick succession, as when stepping through a game.
    let lines: [&[&str]; 5] = [&[], &["e2e4"], &["e2e4", "e7e5"], &["e2e4", "e7e5", "g1f3"], &["e2e4", "e7e5", "g1f3", "b8c6"]];
    for moves in lines {
        manager.start(launch(), request(moves), sink.clone()).await.unwrap();
    }
    let last_fen = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";
    assert!(wait_for(&sink, |u| u.iter().any(|u| u.fen == last_fen && !u.lines.is_empty())).await, "the newest position gets analysed");
    // An explicit stop is silent (the UI already knows), but it must really
    // end the search: otherwise the next request would wait forever.
    manager.stop("fake").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after_stop = sink.0.lock().unwrap().len();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(sink.0.lock().unwrap().len(), after_stop, "no updates after stopping");
    // The engine is free again: a new request is answered.
    manager.start(launch(), request(&["d2d4"]), sink.clone()).await.unwrap();
    assert!(wait_for(&sink, |u| u.iter().any(|u| u.fen.starts_with("rnbqkbnr/pppppppp/8/8/3P4") && !u.lines.is_empty())).await);
    manager.close("fake").await;
}

#[tokio::test]
async fn a_search_that_never_ends_is_stopped_by_its_timeout() {
    let engine = Engine::spawn(std::path::Path::new(FAKE), &[]).await.unwrap();
    let ctl = SearchControl { timeout: Some(Duration::from_millis(300)), ..Default::default() };
    let started = Instant::now();
    let result = engine.search_ctl("position startpos", "go infinite", &ctl, |_| {}).await.unwrap();
    assert!(!result.best.is_empty());
    assert!(started.elapsed() < Duration::from_secs(5));
    // A cancelled request never starts.
    let ctl = SearchControl { cancelled: Some(Box::new(|| true)), ..Default::default() };
    assert_eq!(engine.search_ctl("position startpos", "go infinite", &ctl, |_| {}).await.unwrap().best, "(none)");
    engine.quit().await;
}

#[tokio::test]
async fn reviews_a_game_with_the_fake_engine() {
    let engine = Engine::spawn(std::path::Path::new(FAKE), &[]).await.unwrap();
    let moves: Vec<String> = ["e2e4", "e7e5", "g1f3", "b8c6"].iter().map(|s| s.to_string()).collect();
    let mut steps = 0;
    let result = review::run(&engine, false, START_FEN, &moves, ReviewDepth::Quick, Arc::new(AtomicBool::new(false)), |_, _| steps += 1)
        .await
        .unwrap();
    assert_eq!(result.moves.len(), 4);
    assert_eq!(result.evals.len(), 5);
    assert_eq!(steps, 5);
    // Cancelling stops with the typed error.
    let cancelled = review::run(&engine, false, START_FEN, &moves, ReviewDepth::Quick, Arc::new(AtomicBool::new(true)), |_, _| {}).await;
    assert_eq!(cancelled.unwrap_err().kind(), grandie_chess_lib::error::ErrorKind::Cancelled);
    engine.quit().await;
}
