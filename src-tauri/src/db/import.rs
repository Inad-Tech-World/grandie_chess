//! Turning PGN games into database rows, and bulk import with progress.

use std::io::Read;

use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shakmaty::Position;
use specta::Type;

use super::{now_secs, DbPool};
use crate::error::Result;
use crate::game::pgn::{for_each_game, write_pgn};
use crate::game::position::{hash_of, parse_fen, parse_uci, Side, START_FEN};
use crate::game::tree::GameTree;

/// Names that identify the user in imported games (case-insensitive).
#[derive(Debug, Clone, Default)]
pub struct UserIdentity {
    names: Vec<String>,
}

impl UserIdentity {
    pub fn new<I: IntoIterator<Item = Option<String>>>(names: I) -> Self {
        UserIdentity {
            names: names
                .into_iter()
                .flatten()
                .map(|n| n.trim().to_lowercase())
                .filter(|n| !n.is_empty() && n != "you" && n != "?")
                .collect(),
        }
    }

    fn is_user(&self, name: Option<&str>) -> bool {
        name.is_some_and(|n| self.names.contains(&n.trim().to_lowercase()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameRecord {
    pub source: String,
    pub source_id: String,
    pub event: Option<String>,
    pub site: Option<String>,
    pub date: Option<String>,
    pub round: Option<String>,
    pub white: Option<String>,
    pub black: Option<String>,
    pub white_elo: Option<i32>,
    pub black_elo: Option<i32>,
    pub result: String,
    pub eco: Option<String>,
    pub opening: Option<String>,
    pub time_control: Option<String>,
    pub speed: Option<String>,
    pub termination: Option<String>,
    pub user_color: Option<Side>,
    pub opponent_level: Option<String>,
    pub start_fen: Option<String>,
    pub moves: Vec<String>,
    pub pgn: String,
    pub played_at: Option<i64>,
    /// (Zobrist hash, ply, move played from that position).
    pub positions: Vec<(i64, u32, Option<String>)>,
}

fn clean(v: Option<&str>) -> Option<String> {
    v.map(str::trim).filter(|v| !v.is_empty() && *v != "?" && *v != "-" && *v != "????.??.??").map(str::to_string)
}

/// Lichess-style speed category from a PGN TimeControl ("180+2", "600", "1/86400", "-").
pub fn speed_of(tc: Option<&str>) -> Option<String> {
    let tc = tc?.trim();
    if tc.is_empty() || tc == "-" || tc == "?" {
        return None;
    }
    if tc.contains('/') {
        return Some("correspondence".into());
    }
    let mut parts = tc.split('+');
    let base: f64 = parts.next()?.parse().ok()?;
    let inc: f64 = parts.next().and_then(|i| i.parse().ok()).unwrap_or(0.0);
    let total = base + 40.0 * inc;
    Some(
        match total {
            t if t < 180.0 => "bullet",
            t if t < 480.0 => "blitz",
            t if t < 1500.0 => "rapid",
            _ => "classical",
        }
        .into(),
    )
}

fn played_at(tree: &GameTree) -> Option<i64> {
    let date = tree.header("UTCDate").or(tree.header("Date"))?;
    let time = tree.header("UTCTime").or(tree.header("StartTime")).unwrap_or("00:00:00");
    let dt = chrono::NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y.%m.%d %H:%M:%S").ok()?;
    Some(dt.and_utc().timestamp())
}

/// Opening name from a chess.com ECOUrl ("…/openings/Sicilian-Defense-Alapin").
fn opening_from_url(url: Option<&str>) -> Option<String> {
    let slug = url?.rsplit('/').next()?;
    let name = slug.replace('-', " ");
    (!name.is_empty()).then_some(name)
}

fn derive_source_id(tree: &GameTree, source: &str, moves: &[String]) -> String {
    if source == "lichess" {
        if let Some(id) = tree.header("Site").and_then(|s| s.strip_prefix("https://lichess.org/")) {
            return id.to_string();
        }
    }
    if source == "chesscom" {
        if let Some(link) = tree.header("Link") {
            return link.to_string();
        }
    }
    let mut h = Sha256::new();
    for key in ["White", "Black", "Date", "UTCTime", "Round", "Event"] {
        h.update(tree.header(key).unwrap_or("").as_bytes());
        h.update([0]);
    }
    h.update(moves.join(" ").as_bytes());
    hex::encode(&h.finalize()[..12])
}

pub fn record_from_tree(tree: &GameTree, source: &str, source_id: Option<String>, identity: &UserIdentity) -> Result<GameRecord> {
    let ids = tree.mainline();
    let moves: Vec<String> = ids.iter().filter_map(|&i| tree.nodes[i as usize].uci.clone()).collect();
    let mut pos = parse_fen(&tree.root().fen)?;
    let mut positions = Vec::with_capacity(moves.len() + 1);
    for (ply, uci) in moves.iter().enumerate() {
        positions.push((hash_of(&pos) as i64, ply as u32, Some(uci.clone())));
        let m = parse_uci(&pos, uci)?;
        pos.play_unchecked(m);
    }
    positions.push((hash_of(&pos) as i64, moves.len() as u32, None));

    let h = |k: &str| clean(tree.header(k));
    let elo = |k: &str| tree.header(k).and_then(|v| v.trim().parse::<i32>().ok());
    let user_color = match tree.header("GrandieUserSide") {
        Some("white") => Some(Side::White),
        Some("black") => Some(Side::Black),
        _ if identity.is_user(tree.header("White")) => Some(Side::White),
        _ if identity.is_user(tree.header("Black")) => Some(Side::Black),
        _ => None,
    };
    let start_fen = (tree.root().fen != START_FEN).then(|| tree.root().fen.clone());
    let result = tree.header("Result").filter(|r| ["1-0", "0-1", "1/2-1/2"].contains(r)).unwrap_or("*").to_string();
    Ok(GameRecord {
        source_id: source_id.unwrap_or_else(|| derive_source_id(tree, source, &moves)),
        source: source.to_string(),
        event: h("Event"),
        site: h("Site"),
        date: h("UTCDate").or_else(|| h("Date")),
        round: h("Round"),
        white: h("White"),
        black: h("Black"),
        white_elo: elo("WhiteElo"),
        black_elo: elo("BlackElo"),
        result,
        eco: h("ECO"),
        opening: h("Opening").or_else(|| opening_from_url(tree.header("ECOUrl"))),
        time_control: h("TimeControl"),
        speed: speed_of(tree.header("TimeControl")),
        termination: h("Termination"),
        user_color,
        opponent_level: h("GrandieOpponent"),
        start_fen,
        pgn: write_pgn(tree),
        played_at: played_at(tree),
        positions,
        moves,
    })
}

fn side_str(s: Side) -> &'static str {
    match s {
        Side::White => "white",
        Side::Black => "black",
    }
}

/// Insert a game; returns its id, or `None` if it was already in the database.
pub fn insert(tx: &Transaction, rec: &GameRecord) -> Result<Option<i64>> {
    let changed = tx.execute(
        "INSERT OR IGNORE INTO games (source, source_id, event, site, date, round, white, black, white_elo, black_elo,
            result, eco, opening, time_control, speed, termination, user_color, opponent_level, ply_count, start_fen,
            moves, pgn, played_at, imported_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            rec.source,
            rec.source_id,
            rec.event,
            rec.site,
            rec.date,
            rec.round,
            rec.white,
            rec.black,
            rec.white_elo,
            rec.black_elo,
            rec.result,
            rec.eco,
            rec.opening,
            rec.time_control,
            rec.speed,
            rec.termination,
            rec.user_color.map(side_str),
            rec.opponent_level,
            rec.moves.len() as i64,
            rec.start_fen,
            rec.moves.join(" "),
            rec.pgn,
            rec.played_at,
            now_secs(),
        ],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    let id = tx.last_insert_rowid();
    let mut stmt = tx.prepare_cached("INSERT OR IGNORE INTO positions (hash, game_id, ply, next_move) VALUES (?1, ?2, ?3, ?4)")?;
    for (hash, ply, next) in &rec.positions {
        stmt.execute(params![hash, id, ply, next])?;
    }
    Ok(Some(id))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub seen: u32,
    pub imported: u32,
    pub duplicates: u32,
    /// The first few problems (games that couldn't be read).
    pub errors: Vec<String>,
    /// Newest game time seen, used for incremental account sync.
    pub newest: Option<f64>,
}

const BATCH: usize = 250;

/// Import every game from a PGN stream. `progress(seen, imported)` is called
/// after each batch.
pub fn import_reader<R: Read>(
    pool: &DbPool,
    reader: R,
    source: &str,
    identity: &UserIdentity,
    mut progress: impl FnMut(&ImportSummary),
) -> Result<ImportSummary> {
    let mut summary = ImportSummary::default();
    let mut batch: Vec<GameRecord> = Vec::with_capacity(BATCH);
    let mut conn = pool.get()?;
    let mut flush = |batch: &mut Vec<GameRecord>, summary: &mut ImportSummary| -> Result<()> {
        let tx = conn.transaction()?;
        for rec in batch.drain(..) {
            match insert(&tx, &rec)? {
                Some(_) => summary.imported += 1,
                None => summary.duplicates += 1,
            }
            if let Some(t) = rec.played_at {
                summary.newest = Some(summary.newest.unwrap_or(0.0).max(t as f64));
            }
        }
        tx.commit()?;
        Ok(())
    };
    let mut failure: Option<crate::error::Error> = None;
    for_each_game(reader, |parsed| {
        summary.seen += 1;
        match parsed.map_err(crate::error::Error::Chess).and_then(|(tree, warning)| {
            if let Some(w) = warning {
                if summary.errors.len() < 20 {
                    summary.errors.push(format!("game {}: {w}", summary.seen));
                }
            }
            record_from_tree(&tree, source, None, identity)
        }) {
            Ok(rec) => batch.push(rec),
            Err(e) => {
                if summary.errors.len() < 20 {
                    summary.errors.push(format!("game {}: {e}", summary.seen));
                }
            }
        }
        if batch.len() >= BATCH {
            if let Err(e) = flush(&mut batch, &mut summary) {
                failure = Some(e);
                return false;
            }
            progress(&summary);
        }
        true
    })?;
    if let Some(e) = failure {
        return Err(e);
    }
    flush(&mut batch, &mut summary)?;
    progress(&summary);
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    pub const LICHESS_PGN: &str = r#"[Event "Rated Blitz game"]
[Site "https://lichess.org/AbCd1234"]
[Date "2026.09.30"]
[White "grandie_fan"]
[Black "opponent"]
[Result "1-0"]
[UTCDate "2026.09.30"]
[UTCTime "18:02:11"]
[WhiteElo "1612"]
[BlackElo "1598"]
[ECO "C50"]
[Opening "Italian Game"]
[TimeControl "300+3"]
[Termination "Normal"]

1. e4 { [%clk 0:05:00] } 1... e5 { [%clk 0:05:00] } 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d4 exd4 6. cxd4 Bb4+ 7. Nc3 Nxe4 8. O-O Nxc3 9. bxc3 Bxc3 10. Qb3 d5 11. Bxd5 O-O 12. Bxf7+ Kh8 13. Qxc3 1-0

[Event "Live Chess"]
[Site "Chess.com"]
[Date "2026.10.01"]
[White "someone"]
[Black "Grandie_Fan"]
[Result "0-1"]
[WhiteElo "1500"]
[BlackElo "1520"]
[TimeControl "600"]
[ECOUrl "https://www.chess.com/openings/Scandinavian-Defense"]
[Link "https://www.chess.com/game/live/987654"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 0-1
"#;

    #[test]
    fn builds_records_with_metadata() {
        let parsed = crate::game::pgn::parse_pgn(LICHESS_PGN);
        let identity = UserIdentity::new([Some("Grandie_Fan".to_string()), None]);
        let a = record_from_tree(&parsed.games[0], "lichess", None, &identity).unwrap();
        assert_eq!(a.source_id, "AbCd1234");
        assert_eq!(a.user_color, Some(Side::White));
        assert_eq!(a.speed.as_deref(), Some("blitz"));
        assert_eq!(a.white_elo, Some(1612));
        assert_eq!(a.positions.len(), a.moves.len() + 1);
        assert_eq!(a.positions[0].2.as_deref(), Some("e2e4"));
        assert!(a.played_at.is_some());
        let b = record_from_tree(&parsed.games[1], "chesscom", None, &identity).unwrap();
        assert_eq!(b.source_id, "https://www.chess.com/game/live/987654");
        assert_eq!(b.user_color, Some(Side::Black));
        assert_eq!(b.opening.as_deref(), Some("Scandinavian Defense"));
        assert_eq!(b.speed.as_deref(), Some("rapid"));
    }

    #[test]
    fn speeds() {
        assert_eq!(speed_of(Some("60+0")).as_deref(), Some("bullet"));
        assert_eq!(speed_of(Some("180+2")).as_deref(), Some("blitz"));
        assert_eq!(speed_of(Some("900+10")).as_deref(), Some("rapid"));
        assert_eq!(speed_of(Some("1800+30")).as_deref(), Some("classical"));
        assert_eq!(speed_of(Some("1/86400")).as_deref(), Some("correspondence"));
        assert_eq!(speed_of(Some("-")), None);
    }

    #[test]
    fn imports_and_skips_duplicates() {
        let pool = open_memory().unwrap();
        let identity = UserIdentity::default();
        let first = import_reader(&pool, LICHESS_PGN.as_bytes(), "pgn", &identity, |_| {}).unwrap();
        assert_eq!((first.seen, first.imported, first.duplicates), (2, 2, 0));
        let again = import_reader(&pool, LICHESS_PGN.as_bytes(), "pgn", &identity, |_| {}).unwrap();
        assert_eq!((again.imported, again.duplicates), (0, 2));
        let conn = pool.get().unwrap();
        let positions: i64 = conn.query_row("SELECT COUNT(*) FROM positions", [], |r| r.get(0)).unwrap();
        assert_eq!(positions, 26 + 7);
    }
}
