//! Estimate opponent-level ratings with engine-vs-engine matches.
//!
//! Plays `--games` games between two levels from `resources/levels.json`,
//! alternating colours, and prints the score and the implied rating of the
//! first level relative to the second level's listed Elo.
//!
//! cargo run --release --bin calibrate -- --a leela-3 --b maia-1500 --games 20
//! cargo run --release --bin calibrate -- --a leela-7 --b stockfish:2000
//!
//! `stockfish:<elo>` uses Stockfish with UCI_LimitStrength as the anchor.
//! Options: --lc0 <path> (default: src-tauri/binaries/lc0-<triple>),
//! --networks <dir> (default: the app's networks folder), --max-plies <n>,
//! --stockfish <path> (default: the copy installed by the app).

use std::path::PathBuf;
use std::sync::Arc;

use grandie_chess_lib::engine::catalog::{network_path, Catalog};
use grandie_chess_lib::engine::lc0::{self, Lc0Config};
use grandie_chess_lib::engine::process::{position_command, Engine};
use grandie_chess_lib::game::position::{parse_uci, RepetitionHistory, START_FEN};
use grandie_chess_lib::settings::Settings;
use shakmaty::{Chess, Color, Position};

struct Args {
    a: String,
    b: String,
    games: u32,
    max_plies: usize,
    lc0: PathBuf,
    networks: PathBuf,
    stockfish: PathBuf,
}

fn parse_args() -> Args {
    let mut args = std::env::args().skip(1);
    let home = std::env::var("HOME").unwrap_or_default();
    let triple = if cfg!(target_arch = "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" };
    let mut out = Args {
        a: "leela-3".into(),
        b: "maia-1500".into(),
        games: 10,
        max_plies: 300,
        lc0: PathBuf::from(format!("{}/binaries/lc0-{triple}", env!("CARGO_MANIFEST_DIR"))),
        networks: PathBuf::from(format!("{home}/Library/Application Support/com.grandie.chess/networks")),
        stockfish: PathBuf::from(format!(
            "{home}/Library/Application Support/com.grandie.chess/engines/stockfish/stockfish/stockfish-macos-universal"
        )),
    };
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        match flag.as_str() {
            "--a" => out.a = value,
            "--b" => out.b = value,
            "--games" => out.games = value.parse().expect("--games takes a number"),
            "--max-plies" => out.max_plies = value.parse().expect("--max-plies takes a number"),
            "--lc0" => out.lc0 = value.into(),
            "--networks" => out.networks = value.into(),
            "--stockfish" => out.stockfish = value.into(),
            other => panic!("unknown flag {other}"),
        }
    }
    out
}

/// A calibration player: a Grandie level, or Stockfish limited to an Elo.
struct Player {
    id: String,
    elo: Option<u32>,
    go: String,
    engine: Arc<Engine>,
}

async fn player(spec: &str, catalog: &Catalog, args: &Args, settings: &Settings) -> Player {
    if let Some(elo) = spec.strip_prefix("stockfish:") {
        let elo: u32 = elo.parse().expect("stockfish:<elo>");
        let engine = Engine::spawn(&args.stockfish, &[]).await.expect("Stockfish starts");
        for (name, value) in [("Threads", "1".to_string()), ("Hash", "16".into()), ("UCI_LimitStrength", "true".into()), ("UCI_Elo", elo.to_string())] {
            engine.set_option(name, value).await.unwrap();
        }
        engine.ready(std::time::Duration::from_secs(10)).await.unwrap();
        return Player { id: spec.to_string(), elo: Some(elo), go: "go movetime 150".into(), engine };
    }
    let level = catalog.level(spec).unwrap_or_else(|e| panic!("{e}")).clone();
    let network = level.network.clone().unwrap_or_else(|| settings.default_network.clone().unwrap_or("791556".into()));
    let config = Lc0Config::for_level(&level, network_path(&args.networks, &network), settings);
    let engine = lc0::start(&args.lc0, &config).await.unwrap_or_else(|e| panic!("cannot start {}: {e}", level.id));
    let go = match level.nodes {
        Some(n) => format!("go nodes {n}"),
        None => "go movetime 1000".into(),
    };
    Player { id: level.id.clone(), elo: level.elo, go, engine }
}

/// Plays one game; returns the score for White (1, 0.5 or 0).
async fn play(white: &Player, black: &Player, max_plies: usize) -> f64 {
    let mut pos = Chess::default();
    let mut history = RepetitionHistory::new(&pos);
    let mut moves: Vec<String> = Vec::new();
    for p in [white, black] {
        p.engine.send("ucinewgame").await.unwrap();
    }
    while moves.len() < max_plies {
        if let Some(outcome) = history.outcome(&pos) {
            return match outcome.result.as_str() {
                "1-0" => 1.0,
                "0-1" => 0.0,
                _ => 0.5,
            };
        }
        let p = if pos.turn() == Color::White { white } else { black };
        let result = p.engine.search(&position_command(START_FEN, &moves), &p.go, |_| {}).await.unwrap();
        let m = parse_uci(&pos, &result.best).expect("engine played a legal move");
        pos.play_unchecked(m);
        history.push(m, &pos);
        moves.push(result.best);
    }
    0.5
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let catalog = Catalog::load();
    let mut settings = Settings::default();
    settings.opponent_variety = true;
    let a = player(&args.a, &catalog, &args, &settings).await;
    let b = player(&args.b, &catalog, &args, &settings).await;
    let mut score_a = 0.0;
    for game in 0..args.games {
        let a_white = game % 2 == 0;
        let white_score = if a_white { play(&a, &b, args.max_plies).await } else { play(&b, &a, args.max_plies).await };
        let s = if a_white { white_score } else { 1.0 - white_score };
        score_a += s;
        println!("game {:>3}: {} {} -> {}  (running score {:.1}/{})", game + 1, a.id, if a_white { "white" } else { "black" }, s, score_a, game + 1);
    }
    let pct = score_a / args.games as f64;
    let clamped = pct.clamp(0.03, 0.97);
    let diff = -400.0 * (1.0 / clamped - 1.0).log10();
    println!("\n{} scored {:.1}/{} ({:.0}%) against {}", a.id, score_a, args.games, pct * 100.0, b.id);
    println!("rating difference ≈ {diff:+.0}");
    if let Some(elo_b) = b.elo {
        println!("implied rating for {} ≈ {:.0} (listed: {:?})", a.id, elo_b as f64 + diff, a.elo);
    }
    a.engine.quit().await;
    b.engine.quit().await;
}
