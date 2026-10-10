//! Checks against the real lc0 build and downloaded networks. Ignored by
//! default; run with `cargo test --test lc0_real -- --ignored`.

use std::path::PathBuf;
use std::time::Duration;

use grandie_chess_lib::engine::catalog::{network_path, Catalog};
use grandie_chess_lib::engine::lc0::{self, Lc0Config};
use grandie_chess_lib::engine::process::position_command;
use grandie_chess_lib::game::position::{apply_uci, parse_fen, START_FEN};
use grandie_chess_lib::settings::Settings;

fn lc0_path() -> PathBuf {
    let triple = if cfg!(target_arch = "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" };
    PathBuf::from(format!("{}/binaries/lc0-{triple}", env!("CARGO_MANIFEST_DIR")))
}

fn networks_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/com.grandie.chess/networks")
}

#[tokio::test]
#[ignore]
async fn maia_plays_a_legal_human_move_with_wdl() {
    let catalog = Catalog::load();
    let level = catalog.level("maia-1500").unwrap();
    let weights = network_path(&networks_dir(), "maia-1500");
    let engine = lc0::start(&lc0_path(), &Lc0Config::for_level(level, weights, &Settings::default())).await.unwrap();
    let moves = vec!["e2e4".to_string(), "e7e5".to_string()];
    let result = engine.search(&position_command(START_FEN, &moves), "go nodes 1", |_| {}).await.unwrap();
    let mut pos = parse_fen(START_FEN).unwrap();
    for m in &moves {
        pos = apply_uci(&pos, m).unwrap().0;
    }
    apply_uci(&pos, &result.best).expect("Maia's move is legal");
    let wdl = result.top().and_then(|l| l.wdl).expect("UCI_ShowWDL gives wdl");
    assert_eq!(wdl.w + wdl.d + wdl.l, 1000);
    engine.quit().await;
}

#[tokio::test]
#[ignore]
async fn full_strength_finds_mate_in_one_on_metal() {
    let settings = Settings {
        backend: Some("metal".into()),
        backend_opts: Some(std::env::var("GRANDIE_GPU").unwrap_or_else(|_| "gpu=1".into())),
        ..Settings::default()
    };
    let weights = network_path(&networks_dir(), "791556");
    let engine = lc0::start(&lc0_path(), &Lc0Config::full(weights, &settings)).await.unwrap();
    let fen = "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1";
    let result = tokio::time::timeout(Duration::from_secs(120), engine.search(&format!("position fen {fen}"), "go nodes 200", |_| {}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.best, "d1d8");
    // lc0 loads the network (and prints its backend banner) on the first search.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let banner = lc0::backend_in_use(&engine);
    println!("backend: {banner:?}");
    assert!(banner.is_some_and(|b| b.starts_with("metal")));
    engine.quit().await;
}

mod full_game {
    use super::*;
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Arc, Mutex};

    use grandie_chess_lib::game::clock::TimeControl;
    use grandie_chess_lib::game::pgn::read_pgn_file;
    use grandie_chess_lib::game::session::{ClockTick, PlayContext, PlayManager, PlayObserver, PlaySetup, PlaySnapshot, SideChoice};

    struct Collector(Mutex<Sender<PlaySnapshot>>);
    impl PlayObserver for Collector {
        fn update(&self, s: PlaySnapshot) {
            let _ = self.0.lock().unwrap().send(s);
        }
        fn clock(&self, _t: ClockTick) {}
    }

    /// A timed game against real Maia 1100, with the "user" always playing its
    /// alphabetically first legal move, runs to a decisive end and the saved
    /// PGN reloads with every move.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn timed_game_against_maia_plays_to_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let settings = Settings { human_think_time: false, ..Settings::default() };
        let ctx = Arc::new(PlayContext {
            observer: Arc::new(Collector(Mutex::new(tx))),
            settings,
            lc0_path: lc0_path(),
            networks_dir: networks_dir(),
            pgn_dir: dir.path().to_path_buf(),
        });
        let manager = PlayManager::default();
        let setup = PlaySetup {
            opponent: "maia-1100".into(),
            user_side: SideChoice::White,
            time_control: Some(TimeControl { initial_ms: 300_000, increment_ms: 2_000 }),
            start_fen: None,
            allow_takebacks: false,
        };
        let mut snap = manager.start(ctx, &Catalog::load(), setup).await.unwrap();
        let id = snap.session_id;
        let deadline = std::time::Instant::now() + Duration::from_secs(240);
        while snap.outcome.is_none() {
            assert!(std::time::Instant::now() < deadline, "game took too long");
            if !snap.position.dests.is_empty() {
                let mut moves: Vec<String> =
                    snap.position.dests.iter().flat_map(|(from, tos)| tos.iter().map(move |to| format!("{from}{to}"))).collect();
                moves.sort();
                let mut uci = moves[0].clone();
                let rank = uci.as_bytes()[3];
                let piece_is_pawn = snap.position.fen.split(' ').next().unwrap().contains('P');
                if piece_is_pawn && rank == b'8' && apply_uci(&parse_fen(&snap.position.fen).unwrap(), &uci).is_err() {
                    uci.push('q');
                }
                snap = manager.user_move(id, &uci).await.unwrap();
            }
            if snap.outcome.is_none() && snap.position.dests.is_empty() {
                // Skip stale updates until the engine's reply (or the end) arrives.
                let plies = snap.moves.len();
                loop {
                    let next = rx.recv_timeout(Duration::from_secs(60)).expect("engine replied");
                    if next.outcome.is_some() || (next.moves.len() > plies && !next.engine_thinking) {
                        snap = next;
                        break;
                    }
                }
            }
        }
        let outcome = snap.outcome.clone().unwrap();
        println!("result {} by {:?} after {} plies", outcome.result, outcome.reason, snap.moves.len());
        let path = snap.pgn_path.expect("saved");
        let parsed = read_pgn_file(std::path::Path::new(&path)).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.games[0].mainline().len(), snap.moves.len());
        assert_eq!(parsed.games[0].header("Result"), Some(outcome.result.as_str()));
    }
}

mod review {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use grandie_chess_lib::analysis::classify::MoveClass;
    use grandie_chess_lib::analysis::review::{run, ReviewDepth};
    use grandie_chess_lib::engine::process::Engine;

    /// Scholar's mate: 3...Nf6?? must be flagged as a blunder with a better
    /// move offered, and White's accuracy should beat Black's.
    #[tokio::test]
    #[ignore]
    async fn real_review_flags_the_blunder() {
        let settings = Settings {
            backend: Some("metal".into()),
            backend_opts: Some(std::env::var("GRANDIE_GPU").unwrap_or_else(|_| "gpu=1".into())),
            ..Settings::default()
        };
        let config = Lc0Config::full(network_path(&networks_dir(), "791556"), &settings);
        let engine: Arc<Engine> = lc0::start(&lc0_path(), &config).await.unwrap();
        let moves: Vec<String> = ["e2e4", "e7e5", "d1h5", "b8c6", "f1c4", "g8f6", "h5f7"].iter().map(|s| s.to_string()).collect();
        let started = std::time::Instant::now();
        let r = run(&engine, true, START_FEN, &moves, ReviewDepth::Quick, Arc::new(AtomicBool::new(false)), |_, _| {}).await.unwrap();
        println!("review took {:?}", started.elapsed());
        for m in &r.moves {
            println!("{:>2} {:6} {:?} {:.0}% -> {:.0}% best {:?}", m.ply, m.san, m.class, m.win_before, m.win_after, m.best_san);
        }
        let nf6 = &r.moves[5];
        assert_eq!(nf6.san, "Nf6");
        assert_eq!(nf6.class, MoveClass::Blunder);
        assert!(nf6.best_san.is_some() && nf6.best_san.as_deref() != Some("Nf6"));
        assert_eq!(r.moves[6].san, "Qxf7#");
        assert!(r.white.accuracy.unwrap() > r.black.accuracy.unwrap());
        assert_eq!(r.key_moments.first(), Some(&6));
        engine.quit().await;
    }
}
