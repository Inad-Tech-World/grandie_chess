//! End-to-end games against the scripted fake engine (no GPU needed).

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use grandie_chess_lib::engine::catalog::{network_path, Catalog};
use grandie_chess_lib::engine::process::{position_command, Engine};
use grandie_chess_lib::game::clock::TimeControl;
use grandie_chess_lib::game::pgn::read_pgn_file;
use grandie_chess_lib::game::position::{EndReason, Side, START_FEN};
use grandie_chess_lib::game::session::{ClockTick, PlayContext, PlayManager, PlayObserver, PlaySetup, PlaySnapshot, SideChoice};
use grandie_chess_lib::settings::Settings;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-uci");

struct Collector(Mutex<Sender<PlaySnapshot>>);

impl PlayObserver for Collector {
    fn update(&self, snapshot: PlaySnapshot) {
        let _ = self.0.lock().unwrap().send(snapshot);
    }
    fn clock(&self, _tick: ClockTick) {}
}

struct Fixture {
    _dir: tempfile::TempDir,
    ctx: Arc<PlayContext>,
    rx: Receiver<PlaySnapshot>,
    pgn_dir: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let networks = dir.path().join("networks");
    let pgn_dir = dir.path().join("pgn");
    std::fs::create_dir_all(&networks).unwrap();
    std::fs::create_dir_all(&pgn_dir).unwrap();
    for id in ["maia-1100", "791556"] {
        std::fs::write(network_path(&networks, id), b"fake").unwrap();
    }
    let (tx, rx) = channel();
    let settings = Settings { human_think_time: false, player_name: "Tester".into(), ..Settings::default() };
    let ctx = Arc::new(PlayContext {
        observer: Arc::new(Collector(Mutex::new(tx))),
        settings,
        lc0_path: PathBuf::from(FAKE),
        networks_dir: networks,
        pgn_dir: pgn_dir.clone(),
    });
    Fixture { _dir: dir, ctx, rx, pgn_dir }
}

fn wait_for(rx: &Receiver<PlaySnapshot>, what: &str, pred: impl Fn(&PlaySnapshot) -> bool) -> PlaySnapshot {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(s) if pred(&s) => return s,
            Ok(_) => continue,
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

fn setup(side: SideChoice, tc: Option<TimeControl>) -> PlaySetup {
    PlaySetup { opponent: "maia-1100".into(), user_side: side, time_control: tc, start_fen: None, allow_takebacks: true }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn engine_handshake_and_search() {
    let engine = Engine::spawn(std::path::Path::new(FAKE), &[]).await.unwrap();
    assert_eq!(engine.name, "FakeUCI 1.0");
    assert!(engine.has_option("WeightsFile"));
    engine.ready(Duration::from_secs(5)).await.unwrap();
    let mut seen = 0;
    let result = engine.search(&position_command(START_FEN, &[]), "go nodes 1", |_| seen += 1).await.unwrap();
    assert_eq!(result.best, "a2a3");
    assert_eq!(seen, 1);
    let top = result.top().unwrap();
    assert_eq!(top.wdl.unwrap().w, 300);
    engine.quit().await;
    assert!(!engine.is_alive());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plays_moves_takes_back_and_resigns() {
    let f = fixture();
    let manager = PlayManager::default();
    let catalog = Catalog::load();
    let start = manager.start(f.ctx.clone(), &catalog, setup(SideChoice::White, None)).await.unwrap();
    assert_eq!(start.user_side, Side::White);
    assert_eq!(start.white_name, "Tester");
    assert_eq!(start.black_name, "Maia 1100");
    assert_eq!(start.position.dests.len(), 10, "white's pieces with moves");
    let id = start.session_id;

    assert!(manager.user_move(id, "e2e5").await.is_err(), "illegal move rejected");
    manager.user_move(id, "e2e4").await.unwrap();
    let reply = wait_for(&f.rx, "engine reply", |s| s.moves.len() == 2 && !s.engine_thinking);
    assert_eq!(reply.moves[0].san, "e4");
    assert_eq!(reply.moves[1].uci, "a7a5", "fake engine plays the first move alphabetically");
    assert!(!reply.position.dests.is_empty());
    assert!(reply.can_takeback);

    let back = manager.takeback(id).await.unwrap();
    assert!(back.moves.is_empty());
    assert_eq!(back.position.fen, START_FEN);

    manager.user_move(id, "d2d4").await.unwrap();
    wait_for(&f.rx, "second reply", |s| s.moves.len() == 2 && !s.engine_thinking);
    let resigned = manager.resign(id).await.unwrap();
    let outcome = resigned.outcome.unwrap();
    assert_eq!(outcome.result, "0-1");
    assert_eq!(outcome.reason, EndReason::Resignation);
    let path = resigned.pgn_path.expect("finished games are saved");
    assert!(path.starts_with(f.pgn_dir.to_str().unwrap()));
    let parsed = read_pgn_file(std::path::Path::new(&path)).unwrap();
    let game = &parsed.games[0];
    assert_eq!(game.header("Result"), Some("0-1"));
    assert_eq!(game.header("GrandieOpponent"), Some("maia-1100"));
    assert_eq!(game.header("BlackElo"), Some("1100"));
    assert_eq!(game.mainline().len(), 2);
    assert!(manager.user_move(id, "e2e4").await.is_err(), "no moves after the game ends");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn engine_moves_first_when_user_is_black() {
    let f = fixture();
    let manager = PlayManager::default();
    let snap = manager.start(f.ctx.clone(), &Catalog::load(), setup(SideChoice::Black, None)).await.unwrap();
    assert!(snap.position.dests.is_empty(), "no moves for the user while the engine is to move");
    let first = wait_for(&f.rx, "engine opening move", |s| s.moves.len() == 1 && !s.engine_thinking);
    assert_eq!(first.moves[0].uci, "a2a3");
    assert_eq!(first.position.turn, Side::Black);
    assert!(!first.position.dests.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flag_fall_ends_the_game() {
    let f = fixture();
    let manager = PlayManager::default();
    let tc = TimeControl { initial_ms: 600, increment_ms: 0 };
    let snap = manager.start(f.ctx.clone(), &Catalog::load(), setup(SideChoice::White, Some(tc))).await.unwrap();
    let id = snap.session_id;
    manager.user_move(id, "e2e4").await.unwrap();
    let after_reply = wait_for(&f.rx, "reply", |s| s.moves.len() == 2 && !s.engine_thinking);
    assert_eq!(after_reply.clock.unwrap().running, Some(Side::White));
    let flagged = wait_for(&f.rx, "flag", |s| s.outcome.is_some());
    let outcome = flagged.outcome.unwrap();
    assert_eq!(outcome.reason, EndReason::Timeout);
    assert_eq!(outcome.winner, Some(Side::Black));
    assert_eq!(flagged.clock.unwrap().white_ms, 0.0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reports_missing_networks() {
    let f = fixture();
    let catalog = Catalog::load();
    let missing = PlayManager::missing_networks(&catalog, &f.ctx.settings, &f.ctx.networks_dir, "maia-1900").unwrap();
    assert_eq!(missing, vec!["maia-1900"]);
    assert!(PlayManager::missing_networks(&catalog, &f.ctx.settings, &f.ctx.networks_dir, "full").unwrap().is_empty());
    let manager = PlayManager::default();
    let err = manager
        .start(f.ctx.clone(), &catalog, PlaySetup { opponent: "maia-1900".into(), ..setup(SideChoice::White, None) })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("network file missing"), "{err}");
}
