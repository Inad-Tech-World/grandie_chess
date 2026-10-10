//! Progress statistics: strength against each opponent level, game quality
//! from reviews, weakest openings and puzzle rating history.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::glicko::Rating;
use super::puzzles;
use crate::engine::catalog::Catalog;
use crate::error::Result;

/// ECO code, the user's scores and their accuracies in one opening family.
type OpeningSamples = (Option<String>, Vec<f64>, Vec<f64>);

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LevelResult {
    pub level: String,
    pub name: String,
    pub elo: Option<i32>,
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Performance rating against this level, if it has a rating.
    pub performance: Option<f64>,
}

/// Performance against one opponent level over time.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LevelSeries {
    pub level: String,
    pub name: String,
    pub elo: Option<i32>,
    pub points: Vec<RatingPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RatingPoint {
    /// Unix seconds.
    pub at: f64,
    pub rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameQuality {
    pub game_id: i32,
    pub at: Option<f64>,
    pub opponent: Option<String>,
    pub accuracy: Option<f64>,
    pub inaccuracies: u32,
    pub mistakes: u32,
    pub blunders: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpeningStat {
    pub family: String,
    pub eco: Option<String>,
    pub games: u32,
    /// The user's score, 0..1.
    pub score: f64,
    pub accuracy: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub games_last_30_days: u32,
    pub puzzles_attempted: u32,
    pub puzzles_solved: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSummary {
    pub totals: Totals,
    pub levels: Vec<LevelResult>,
    /// Rolling performance against each level, game by game.
    pub level_series: Vec<LevelSeries>,
    /// Estimated playing strength over time (rolling performance in Grandie games).
    pub strength: Vec<RatingPoint>,
    pub quality: Vec<GameQuality>,
    pub weakest_openings: Vec<OpeningStat>,
    pub puzzle_rating: Rating,
    pub puzzle_history: Vec<RatingPoint>,
}

/// Linear performance rating: average opponent rating + 400 × (W − L) / N.
pub fn performance(results: &[(f64, f64)]) -> Option<f64> {
    if results.is_empty() {
        return None;
    }
    let n = results.len() as f64;
    let avg = results.iter().map(|r| r.0).sum::<f64>() / n;
    let wl: f64 = results.iter().map(|r| (r.1 - 0.5) * 2.0).sum();
    Some(avg + 400.0 * wl / n)
}

fn user_score(user_color: &str, result: &str) -> Option<f64> {
    match (user_color, result) {
        (_, "1/2-1/2") => Some(0.5),
        ("white", "1-0") | ("black", "0-1") => Some(1.0),
        ("white", "0-1") | ("black", "1-0") => Some(0.0),
        _ => None,
    }
}

const ROLLING: usize = 10;
const LEVEL_ROLLING: usize = 5;

pub fn summary(conn: &Connection, catalog: &Catalog, now: i64) -> Result<ProgressSummary> {
    // All decided games the user played, oldest first.
    let mut stmt = conn.prepare(
        "SELECT g.id, g.user_color, g.result, g.played_at, g.opponent_level, g.opening, g.eco,
                CASE WHEN g.user_color = 'white' THEN g.black_elo ELSE g.white_elo END,
                CASE WHEN g.user_color = 'white' THEN g.black ELSE g.white END,
                r.user_accuracy, r.user_inaccuracies, r.user_mistakes, r.user_blunders, r.game_id IS NOT NULL
         FROM games g LEFT JOIN reviews r ON r.game_id = g.id
         WHERE g.user_color IS NOT NULL AND g.result != '*'
         ORDER BY COALESCE(g.played_at, g.imported_at), g.id",
    )?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        i64,
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<f64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        bool,
    )> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut totals = Totals::default();
    let mut by_level: HashMap<String, Vec<f64>> = HashMap::new();
    let mut level_points: HashMap<String, Vec<RatingPoint>> = HashMap::new();
    let mut rolling: Vec<(f64, f64)> = Vec::new();
    let mut strength = Vec::new();
    let mut quality = Vec::new();
    let mut openings: HashMap<String, OpeningSamples> = HashMap::new();

    for (id, color, result, played_at, level, opening, eco, opp_elo, opponent, acc, inacc, mist, blund, reviewed) in &rows {
        let Some(score) = user_score(color, result) else { continue };
        totals.games += 1;
        if score == 1.0 {
            totals.wins += 1;
        } else if score == 0.0 {
            totals.losses += 1;
        } else {
            totals.draws += 1;
        }
        if played_at.is_some_and(|t| t >= now - 30 * 86_400) {
            totals.games_last_30_days += 1;
        }
        // Against Grandie opponents the level's rating is the opponent's rating.
        let level_elo = level.as_deref().and_then(|l| catalog.level(l).ok()).and_then(|l| l.elo).map(|e| e as f64);
        if let Some(l) = level {
            let scores = by_level.entry(l.clone()).or_default();
            scores.push(score);
            if let (Some(elo), Some(t)) = (level_elo, *played_at) {
                let recent: Vec<(f64, f64)> = scores.iter().rev().take(LEVEL_ROLLING).map(|s| (elo, *s)).collect();
                if let Some(p) = performance(&recent) {
                    level_points.entry(l.clone()).or_default().push(RatingPoint { at: t as f64, rating: p });
                }
            }
        }
        if let Some(opp) = level_elo.or(opp_elo.map(|e| e as f64)) {
            rolling.push((opp, score));
            if rolling.len() > ROLLING {
                rolling.remove(0);
            }
            if let (Some(t), Some(p)) = (played_at, performance(&rolling)) {
                strength.push(RatingPoint { at: *t as f64, rating: p });
            }
        }
        if *reviewed {
            quality.push(GameQuality {
                game_id: *id as i32,
                at: played_at.map(|t| t as f64),
                opponent: opponent.clone(),
                accuracy: *acc,
                inaccuracies: inacc.unwrap_or(0) as u32,
                mistakes: mist.unwrap_or(0) as u32,
                blunders: blund.unwrap_or(0) as u32,
            });
        }
        if let Some(name) = opening {
            let family = name.split(':').next().unwrap_or(name).trim().to_string();
            let entry = openings.entry(family).or_insert((eco.clone(), Vec::new(), Vec::new()));
            entry.1.push(score);
            if let Some(a) = acc {
                entry.2.push(*a);
            }
        }
    }

    let mut levels: Vec<LevelResult> = by_level
        .into_iter()
        .map(|(id, scores)| {
            let level = catalog.level(&id).ok();
            let elo = level.and_then(|l| l.elo).map(|e| e as i32);
            LevelResult {
                name: level.map(|l| l.name.clone()).unwrap_or_else(|| id.clone()),
                performance: elo.and_then(|e| performance(&scores.iter().map(|s| (e as f64, *s)).collect::<Vec<_>>())),
                games: scores.len() as u32,
                wins: scores.iter().filter(|s| **s == 1.0).count() as u32,
                draws: scores.iter().filter(|s| **s == 0.5).count() as u32,
                losses: scores.iter().filter(|s| **s == 0.0).count() as u32,
                elo,
                level: id,
            }
        })
        .collect();
    levels.sort_by_key(|l| l.elo.unwrap_or(i32::MAX));
    let level_series: Vec<LevelSeries> = levels
        .iter()
        .filter_map(|l| {
            let points = level_points.remove(&l.level)?;
            Some(LevelSeries { level: l.level.clone(), name: l.name.clone(), elo: l.elo, points })
        })
        .collect();

    let mut weakest: Vec<OpeningStat> = openings
        .into_iter()
        .filter(|(_, (_, scores, _))| scores.len() >= 3)
        .map(|(family, (eco, scores, accs))| OpeningStat {
            score: scores.iter().sum::<f64>() / scores.len() as f64,
            accuracy: (!accs.is_empty()).then(|| accs.iter().sum::<f64>() / accs.len() as f64),
            games: scores.len() as u32,
            eco,
            family,
        })
        .collect();
    weakest.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal).then(b.games.cmp(&a.games)));
    weakest.truncate(8);

    let quality_len = quality.len();
    let quality = quality.into_iter().skip(quality_len.saturating_sub(50)).collect();

    let mut stmt = conn.prepare("SELECT attempted_at, rating_after, solved FROM puzzle_attempts ORDER BY attempted_at, id")?;
    let attempts: Vec<(i64, f64, bool)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    totals.puzzles_attempted = attempts.len() as u32;
    totals.puzzles_solved = attempts.iter().filter(|a| a.2).count() as u32;

    Ok(ProgressSummary {
        totals,
        levels,
        level_series,
        strength,
        quality,
        weakest_openings: weakest,
        puzzle_rating: puzzles::rating(conn)?,
        puzzle_history: attempts.into_iter().map(|(at, rating, _)| RatingPoint { at: at as f64, rating }).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use rusqlite::params;

    fn insert_game(conn: &Connection, id: i64, color: &str, result: &str, level: Option<&str>, opening: &str, at: i64) {
        conn.execute(
            "INSERT INTO games (id, source, source_id, user_color, result, opponent_level, opening, eco, ply_count, moves, pgn, played_at, imported_at)
             VALUES (?1, 'grandie', ?1, ?2, ?3, ?4, ?5, 'B20', 40, '', '', ?6, ?6)",
            params![id, color, result, level, opening, at],
        )
        .unwrap();
    }

    #[test]
    fn performance_formula() {
        assert_eq!(performance(&[]), None);
        assert_eq!(performance(&[(1500.0, 0.5), (1500.0, 0.5)]), Some(1500.0));
        assert_eq!(performance(&[(1500.0, 1.0), (1500.0, 0.0)]), Some(1500.0));
        assert_eq!(performance(&[(1500.0, 1.0)]), Some(1900.0));
    }

    #[test]
    fn summarises_levels_openings_and_puzzles() {
        let pool = open_memory().unwrap();
        let conn = pool.get().unwrap();
        let now = 1_800_000_000;
        insert_game(&conn, 1, "white", "1-0", Some("maia-1500"), "Sicilian Defense: Alapin", now - 10);
        insert_game(&conn, 2, "black", "1-0", Some("maia-1500"), "Sicilian Defense: Najdorf", now - 9);
        insert_game(&conn, 3, "white", "0-1", Some("maia-1500"), "Sicilian Defense", now - 8);
        insert_game(&conn, 4, "white", "1-0", Some("leela-3"), "Italian Game", now - 7);
        conn.execute(
            "INSERT INTO reviews (game_id, depth, user_accuracy, user_inaccuracies, user_mistakes, user_blunders, data, created_at)
             VALUES (1, 'quick', 81.5, 2, 1, 0, '{}', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO puzzle_attempts (puzzle_id, solved, puzzle_rating, themes, rating_before, rating_after, attempted_at)
             VALUES ('x', 1, 1500, 'fork', 1500, 1600, ?1)",
            [now],
        )
        .unwrap();
        let s = summary(&conn, &Catalog::load(), now).unwrap();
        assert_eq!((s.totals.games, s.totals.wins, s.totals.losses), (4, 2, 2));
        let maia = s.levels.iter().find(|l| l.level == "maia-1500").unwrap();
        assert_eq!((maia.games, maia.wins, maia.losses), (3, 1, 2));
        assert!(maia.performance.unwrap() < 1500.0);
        assert_eq!(s.strength.len(), 4);
        let maia_series = s.level_series.iter().find(|l| l.level == "maia-1500").unwrap();
        assert_eq!(maia_series.points.len(), 3);
        assert!(maia_series.points[0].rating > maia_series.points[2].rating, "win, then two losses");
        assert_eq!(s.quality.len(), 1);
        assert_eq!(s.quality[0].mistakes, 1);
        let sicilian = &s.weakest_openings[0];
        assert_eq!(sicilian.family, "Sicilian Defense");
        assert_eq!(sicilian.games, 3);
        assert!((sicilian.score - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.puzzle_history.len(), 1);
        assert_eq!(s.totals.puzzles_solved, 1);
    }
}
