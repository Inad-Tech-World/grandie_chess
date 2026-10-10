//! Phase 3 target: a few thousand games import, and position search returns
//! in under a second. Run optimised: `cargo test --profile perf --test db_perf -- --ignored --nocapture`.

use std::time::Instant;

use grandie_chess_lib::db::games::{query, GameQuery};
use grandie_chess_lib::db::import::{insert, record_from_tree, UserIdentity};
use grandie_chess_lib::db::open;
use grandie_chess_lib::db::search::PositionQuery;
use grandie_chess_lib::game::position::{fen_of, uci_of};
use grandie_chess_lib::game::tree::GameTree;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use shakmaty::{Chess, Position};

fn random_game(rng: &mut StdRng, plies: usize) -> GameTree {
    let mut tree = GameTree::new(None).unwrap();
    let mut pos = Chess::default();
    let mut node = 0;
    // A common opening so exact search has plenty of hits.
    for uci in ["e2e4", "e7e5", "g1f3"] {
        node = tree.add_move(node, uci).unwrap();
        pos = grandie_chess_lib::game::position::parse_fen(&tree.nodes[node as usize].fen).unwrap();
    }
    for _ in 0..plies {
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        let m = moves[rng.random_range(0..moves.len())];
        node = tree.add_move(node, &uci_of(m)).unwrap();
        pos.play_unchecked(m);
    }
    tree.set_header("White", format!("Player{}", rng.random_range(0..200)));
    tree.set_header("Black", format!("Player{}", rng.random_range(0..200)));
    tree.set_header("Date", "2026.05.01");
    tree.set_header("Result", "1/2-1/2");
    tree
}

#[test]
#[ignore]
fn five_thousand_games_search_under_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open(&dir.path().join("perf.db")).unwrap();
    let mut rng = StdRng::seed_from_u64(7);
    let identity = UserIdentity::default();
    let started = Instant::now();
    {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0..5000 {
            let tree = random_game(&mut rng, 60 + (i % 40));
            let rec = record_from_tree(&tree, "pgn", Some(format!("g{i}")), &identity).unwrap();
            insert(&tx, &rec).unwrap();
        }
        tx.commit().unwrap();
    }
    println!("generated and imported 5000 games in {:?}", started.elapsed());

    let after_nf3 = {
        let mut p = Chess::default();
        for u in ["e2e4", "e7e5", "g1f3"] {
            let m = grandie_chess_lib::game::position::parse_uci(&p, u).unwrap();
            p.play_unchecked(m);
        }
        fen_of(&p)
    };
    let t = Instant::now();
    let exact =
        query(&pool, &GameQuery { position: Some(PositionQuery { fen: after_nf3, exact: true }), limit: 200, ..Default::default() })
            .unwrap();
    let exact_time = t.elapsed();
    println!("exact search: {} games in {:?}", exact.total, exact_time);
    assert_eq!(exact.total, 5000);

    let t = Instant::now();
    // Partial: a white queen on h5 and a black king on e8 at the same time.
    let partial =
        query(&pool, &GameQuery { position: Some(PositionQuery { fen: "4k3/8/8/7Q/8/8/8/8".into(), exact: false }), ..Default::default() })
            .unwrap();
    let partial_time = t.elapsed();
    println!("partial search: {} games in {:?}", partial.total, partial_time);

    assert!(exact_time.as_secs_f64() < 1.0, "exact search took {exact_time:?}");
    assert!(partial_time.as_secs_f64() < 1.0, "partial search took {partial_time:?}");
}
