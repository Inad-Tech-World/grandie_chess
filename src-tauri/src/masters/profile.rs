//! A master's profile: what the library says about how they play. The
//! numbers come from SQL over their games; an optional engine pass measures
//! style (accuracy, sharpness, sacrifices); an optional AI report puts it
//! into words.

use std::collections::HashMap;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::db::DbPool;
use crate::error::Result;
use crate::game::position::{parse_fen, parse_uci, san_of, START_FEN};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ColorStats {
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Points per game for the master, 0..100.
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MasterOpeningStat {
    pub eco: Option<String>,
    pub name: String,
    pub games: u32,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MasterMoveStat {
    pub san: String,
    pub games: u32,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpponentStat {
    pub name: String,
    pub games: u32,
    pub score: f64,
}

/// Measured by the coach's engine over a sample of games.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleMetrics {
    pub games_analysed: u32,
    /// Average move accuracy (0..100), Lichess-style.
    pub accuracy: f64,
    /// Average change in winning chances per move: high means sharp games.
    pub sharpness: f64,
    /// Brilliant moves (sound sacrifices) per game.
    pub sacrifices_per_game: f64,
    /// "Only moves" found (great moves) per game.
    pub great_per_game: f64,
    pub mistakes_per_game: f64,
    pub blunders_per_game: f64,
    /// Engine (coach) used.
    pub engine: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStats {
    pub id: String,
    pub name: String,
    pub games: u32,
    pub first_year: Option<i32>,
    pub last_year: Option<i32>,
    pub as_white: ColorStats,
    pub as_black: ColorStats,
    pub openings_white: Vec<MasterOpeningStat>,
    pub openings_black: Vec<MasterOpeningStat>,
    /// First moves with White.
    pub first_moves: Vec<MasterMoveStat>,
    /// Replies with Black to 1.e4 and to 1.d4.
    pub vs_e4: Vec<MasterMoveStat>,
    pub vs_d4: Vec<MasterMoveStat>,
    pub average_moves: f64,
    /// Share of decisive games, 0..100.
    pub decisive: f64,
    pub top_opponents: Vec<OpponentStat>,
    /// (decade, games), e.g. (1950, 120).
    pub decades: Vec<(i32, u32)>,
    pub style: Option<StyleMetrics>,
    /// The coach's write-up of the player's style, when generated.
    pub report: Option<String>,
}

/// What is stored in `player_profiles.data`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredProfile {
    pub style: Option<StyleMetrics>,
    pub report: Option<String>,
    /// Games in the library when the report was written.
    pub report_games: u32,
}

pub fn stored(pool: &DbPool, id: &str) -> Result<StoredProfile> {
    stored_in(&*pool.get()?, id)
}

fn stored_in(conn: &rusqlite::Connection, id: &str) -> Result<StoredProfile> {
    let data: Option<String> = conn.query_row("SELECT data FROM player_profiles WHERE subject = ?1", [id], |r| r.get(0)).optional()?;
    Ok(data.and_then(|d| serde_json::from_str(&d).ok()).unwrap_or_default())
}

pub fn save(pool: &DbPool, id: &str, profile: &StoredProfile) -> Result<()> {
    pool.get()?.execute(
        "INSERT INTO player_profiles (subject, data, version, updated_at) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT (subject) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
        rusqlite::params![id, serde_json::to_string(profile)?, crate::db::now_secs()],
    )?;
    Ok(())
}

/// Points (1, 0.5, 0) for the master in a game.
fn points(color: &str, result: &str) -> Option<f64> {
    match (color, result) {
        ("white", "1-0") | ("black", "0-1") => Some(1.0),
        (_, "1/2-1/2") => Some(0.5),
        ("white", "0-1") | ("black", "1-0") => Some(0.0),
        _ => None,
    }
}

fn pct(points: f64, games: u32) -> f64 {
    if games == 0 {
        0.0
    } else {
        (points / games as f64 * 1000.0).round() / 10.0
    }
}

struct Row {
    color: String,
    result: String,
    eco: Option<String>,
    opening: Option<String>,
    moves: String,
    start_fen: Option<String>,
    ply: u32,
    year: Option<i32>,
    opponent: Option<String>,
}

/// (colour, ECO letter, opening family).
/// (colour, opening family).
type OpeningKey = (String, String);
/// (games, points, ECO letter).
type OpeningTally = (u32, f64, Option<String>);

/// Statistics over every game of the master `id` in the library.
pub fn stats(pool: &DbPool, id: &str) -> Result<PlayerStats> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT s.color, g.result, g.eco, g.opening, g.moves, g.start_fen, g.ply_count, g.year,
                CASE s.color WHEN 'white' THEN g.black ELSE g.white END
         FROM game_subjects s JOIN games g ON g.id = s.game_id
         WHERE s.subject = ?1 AND s.color IS NOT NULL",
    )?;
    let rows: Vec<Row> = stmt
        .query_map([id], |r| {
            Ok(Row {
                color: r.get(0)?,
                result: r.get(1)?,
                eco: r.get(2)?,
                opening: r.get(3)?,
                moves: r.get(4)?,
                start_fen: r.get(5)?,
                ply: r.get::<_, i64>(6)? as u32,
                year: r.get(7)?,
                opponent: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let name = super::catalog_player(id).map(|(_, n)| n).unwrap_or_else(|| id.to_string());
    let mut out = PlayerStats { id: id.to_string(), name, games: rows.len() as u32, ..Default::default() };
    let mut openings: HashMap<OpeningKey, OpeningTally> = HashMap::new();
    let mut first: HashMap<String, (u32, f64)> = HashMap::new();
    let mut vs_e4: HashMap<String, (u32, f64)> = HashMap::new();
    let mut vs_d4: HashMap<String, (u32, f64)> = HashMap::new();
    let mut opponents: HashMap<String, (u32, f64)> = HashMap::new();
    let mut decades: HashMap<i32, u32> = HashMap::new();
    let (mut ply_total, mut decisive) = (0u64, 0u32);
    let start = parse_fen(START_FEN)?;
    for r in &rows {
        let p = points(&r.color, &r.result);
        let side = if r.color == "white" { &mut out.as_white } else { &mut out.as_black };
        side.games += 1;
        if let Some(points) = p {
            if points > 0.75 {
                side.wins += 1;
            } else if points > 0.25 {
                side.draws += 1;
            } else {
                side.losses += 1;
            }
        }
        side.score += p.unwrap_or(0.0);
        let p = p.unwrap_or(0.0);
        if r.result == "1-0" || r.result == "0-1" {
            decisive += 1;
        }
        ply_total += r.ply as u64;
        if let Some(y) = r.year {
            out.first_year = Some(out.first_year.map_or(y, |f| f.min(y)));
            out.last_year = Some(out.last_year.map_or(y, |l| l.max(y)));
            *decades.entry(y / 10 * 10).or_default() += 1;
        }
        if let Some(name) = r.opening.clone().filter(|n| !n.is_empty()) {
            // Group by the family ("Sicilian Defense: Najdorf" -> "Sicilian Defense").
            let family = name.split(':').next().unwrap_or(&name).trim().to_string();
            // Keyed by colour and family only: some games have no ECO code,
            // and the same family shouldn't be listed twice because of it.
            let e = openings.entry((r.color.clone(), family)).or_default();
            e.0 += 1;
            e.1 += p;
            if e.2.is_none() {
                e.2 = r.eco.as_ref().map(|e| e.chars().take(1).collect());
            }
        }
        if let Some(opp) = r.opponent.clone().filter(|o| !o.trim().is_empty()) {
            let e = opponents.entry(opp).or_default();
            e.0 += 1;
            e.1 += p;
        }
        if r.start_fen.is_none() {
            let mut moves = r.moves.split_whitespace();
            if let Some(m1) = moves.next() {
                if let Ok(mv) = parse_uci(&start, m1) {
                    let san1 = san_of(&start, mv);
                    if r.color == "white" {
                        let e = first.entry(san1.clone()).or_default();
                        e.0 += 1;
                        e.1 += p;
                    } else if let Some(m2) = moves.next() {
                        let mut pos = start.clone();
                        shakmaty::Position::play_unchecked(&mut pos, mv);
                        if let Ok(reply) = parse_uci(&pos, m2) {
                            let san2 = san_of(&pos, reply);
                            let table = match san1.as_str() {
                                "e4" => Some(&mut vs_e4),
                                "d4" => Some(&mut vs_d4),
                                _ => None,
                            };
                            if let Some(t) = table {
                                let e = t.entry(san2).or_default();
                                e.0 += 1;
                                e.1 += p;
                            }
                        }
                    }
                }
            }
        }
    }
    for side in [&mut out.as_white, &mut out.as_black] {
        side.score = pct(side.score, side.games);
    }
    let top = |m: HashMap<String, (u32, f64)>, n: usize| -> Vec<MasterMoveStat> {
        let mut v: Vec<MasterMoveStat> = m.into_iter().map(|(san, (g, p))| MasterMoveStat { san, games: g, score: pct(p, g) }).collect();
        v.sort_by(|a, b| b.games.cmp(&a.games).then(a.san.cmp(&b.san)));
        v.truncate(n);
        v
    };
    out.first_moves = top(first, 5);
    out.vs_e4 = top(vs_e4, 5);
    out.vs_d4 = top(vs_d4, 5);
    let mut ops: Vec<(OpeningKey, OpeningTally)> = openings.into_iter().collect();
    ops.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0 .1.cmp(&b.0 .1)));
    for ((color, name), (g, p, eco)) in ops {
        let list = if color == "white" { &mut out.openings_white } else { &mut out.openings_black };
        if list.len() < 8 {
            list.push(MasterOpeningStat { eco, name, games: g, score: pct(p, g) });
        }
    }
    let mut opp: Vec<OpponentStat> =
        opponents.into_iter().map(|(name, (g, p))| OpponentStat { name, games: g, score: pct(p, g) }).collect();
    opp.sort_by(|a, b| b.games.cmp(&a.games).then(a.name.cmp(&b.name)));
    opp.truncate(8);
    out.top_opponents = opp;
    let mut dec: Vec<(i32, u32)> = decades.into_iter().collect();
    dec.sort();
    out.decades = dec;
    if out.games > 0 {
        out.average_moves = ((ply_total as f64 / out.games as f64) / 2.0 * 10.0).round() / 10.0;
        out.decisive = pct(decisive as f64, out.games);
    }
    let stored = stored_in(&conn, id)?;
    out.style = stored.style;
    out.report = stored.report;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_a_players_games() {
        let pool = crate::db::open_memory().unwrap();
        let pgn = r#"[Event "A"]
[Date "1958.??.??"]
[White "Tal, Mikhail"]
[Black "X, Y"]
[Result "1-0"]
[ECO "B20"]
[Opening "Sicilian Defense: Smith-Morra"]

1. e4 c5 2. d4 1-0

[Event "B"]
[Date "1962.??.??"]
[White "Z, W"]
[Black "Tal, Mikhail"]
[Result "1/2-1/2"]

1. e4 e5 1/2-1/2

[Event "C"]
[Date "1965.??.??"]
[White "Tal, Mikhail"]
[Black "X, Y"]
[Result "0-1"]
[Opening "Sicilian Defense: Najdorf"]

1. e4 c5 0-1
"#;
        super::super::import_pgn_as_master(&pool, pgn.as_bytes(), "Tal", |_| {}).unwrap();
        let s = stats(&pool, "Tal").unwrap();
        assert_eq!(s.games, 3);
        assert_eq!((s.as_white.games, s.as_white.wins, s.as_white.losses, s.as_white.score), (2, 1, 1, 50.0));
        assert_eq!((s.as_black.draws, s.as_black.score), (1, 50.0));
        assert_eq!(s.first_moves[0].san, "e4");
        assert_eq!(s.vs_e4[0].san, "e5");
        assert_eq!(s.openings_white[0].name, "Sicilian Defense");
        assert_eq!(s.openings_white[0].games, 2);
        assert_eq!((s.first_year, s.last_year), (Some(1958), Some(1965)));
        assert_eq!(s.decades, vec![(1950, 1), (1960, 2)]);
        assert_eq!(s.top_opponents[0].name, "X, Y");
        assert!((s.decisive - 66.7).abs() < 0.01);
    }
}
