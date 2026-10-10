//! Checks against the live Lichess and Chess.com APIs (read-only requests).
//! Ignored by default; run with `cargo test --test net_real -- --ignored --nocapture`.

use grandie_chess_lib::db::import::{import_reader, UserIdentity};
use grandie_chess_lib::db::open_memory;
use grandie_chess_lib::net::{chesscom, download::http_client, lichess};

#[tokio::test]
#[ignore]
async fn imports_a_few_lichess_games() {
    let client = http_client();
    let mut last = (0, 0);
    let bytes = lichess::fetch_user_games(&client, "DrNykterstein", None, Some(5), |g, b| last = (g, b)).await.unwrap();
    assert!(last.1 > 0);
    let pool = open_memory().unwrap();
    let identity = UserIdentity::new([Some("DrNykterstein".to_string())]);
    let summary = import_reader(&pool, bytes.as_slice(), "lichess", &identity, |_| {}).unwrap();
    println!("lichess: {summary:?}");
    assert_eq!(summary.seen, 5);
    assert_eq!(summary.imported, 5);
    assert!(summary.newest.is_some());
    let conn = pool.get().unwrap();
    let mine: i64 = conn.query_row("SELECT COUNT(*) FROM games WHERE user_color IS NOT NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(mine, 5, "every game is marked as the user's");
}

#[tokio::test]
#[ignore]
async fn imports_a_chesscom_month() {
    let client = http_client();
    let archives = chesscom::archives(&client, "erik").await.unwrap();
    assert!(!archives.is_empty());
    // An early, small month keeps the download light.
    let first = archives.first().unwrap();
    let pgn = chesscom::month_pgn(&client, first).await.unwrap();
    let pool = open_memory().unwrap();
    let summary = import_reader(&pool, pgn.as_bytes(), "chesscom", &UserIdentity::new([Some("erik".to_string())]), |_| {}).unwrap();
    println!("chess.com {first}: {summary:?}");
    assert!(summary.imported > 0);
    assert!(summary.errors.len() < summary.seen as usize);
}
