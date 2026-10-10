//! Lichess opening explorer (lichess and masters databases). Since 2025 the
//! explorer requires a Lichess API token; results are cached in memory.
//! https://lichess.org/api#tag/Opening-Explorer

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde::Deserialize;

use crate::db::explorer::{ExplorerMove, ExplorerResult};
use crate::error::{Error, Result};
use crate::game::openings::OpeningName;

const CACHE_FOR: Duration = Duration::from_secs(30 * 60);
const CACHE_MAX: usize = 2000;

#[derive(Deserialize)]
struct ApiMove {
    uci: String,
    san: String,
    #[serde(rename = "averageRating")]
    average_rating: Option<f64>,
    white: u32,
    draws: u32,
    black: u32,
}

#[derive(Deserialize)]
struct ApiOpening {
    eco: String,
    name: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    white: u32,
    draws: u32,
    black: u32,
    moves: Vec<ApiMove>,
    opening: Option<ApiOpening>,
}

#[derive(Default)]
pub struct ExplorerCache {
    entries: Mutex<HashMap<String, (Instant, ExplorerResult)>>,
}

impl ExplorerCache {
    fn get(&self, key: &str) -> Option<ExplorerResult> {
        let map = self.entries.lock().unwrap();
        map.get(key).filter(|(t, _)| t.elapsed() < CACHE_FOR).map(|(_, r)| r.clone())
    }

    fn put(&self, key: String, value: ExplorerResult) {
        let mut map = self.entries.lock().unwrap();
        if map.len() >= CACHE_MAX {
            map.retain(|_, (t, _)| t.elapsed() < CACHE_FOR);
            if map.len() >= CACHE_MAX {
                map.clear();
            }
        }
        map.insert(key, (Instant::now(), value));
    }
}

/// `db` is "lichess" or "masters". `speeds` / `ratings` filter the lichess database.
pub async fn explore(
    client: &reqwest::Client,
    cache: &ExplorerCache,
    token: &str,
    db: &str,
    fen: &str,
    speeds: &[String],
    ratings: &[u32],
) -> Result<ExplorerResult> {
    if db != "lichess" && db != "masters" {
        return Err(Error::other(format!("unknown explorer database {db}")));
    }
    let mut query: Vec<(String, String)> = vec![
        ("fen".into(), fen.to_string()),
        ("moves".into(), "15".into()),
        ("topGames".into(), "0".into()),
        ("recentGames".into(), "0".into()),
    ];
    if db == "lichess" {
        if !speeds.is_empty() {
            query.push(("speeds".into(), speeds.join(",")));
        }
        if !ratings.is_empty() {
            query.push(("ratings".into(), ratings.iter().map(u32::to_string).collect::<Vec<_>>().join(",")));
        }
    }
    let key = format!("{db}|{query:?}");
    if let Some(hit) = cache.get(&key) {
        return Ok(hit);
    }
    let url = reqwest::Url::parse_with_params(&format!("https://explorer.lichess.ovh/{db}"), &query)
        .map_err(|e| Error::other(format!("bad explorer URL: {e}")))?;
    let response = client.get(url).bearer_auth(token).send().await?;
    match response.status() {
        StatusCode::UNAUTHORIZED => {
            return Err(Error::other("Lichess rejected the API token. Check it in Settings."));
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(Error::other("The Lichess explorer is rate-limiting requests; wait a minute and try again."));
        }
        _ => {}
    }
    let api: ApiResponse = response.error_for_status()?.json().await?;
    let result = ExplorerResult {
        source: db.to_string(),
        fen: fen.to_string(),
        total: api.white + api.draws + api.black,
        white: api.white,
        draws: api.draws,
        black: api.black,
        moves: api
            .moves
            .into_iter()
            .map(|m| ExplorerMove {
                games: m.white + m.draws + m.black,
                uci: normalise_castling(&m.uci, &m.san),
                san: m.san,
                white: m.white,
                draws: m.draws,
                black: m.black,
                avg_rating: m.average_rating,
            })
            .collect(),
        opening: api.opening.map(|o| OpeningName { eco: o.eco, name: o.name }),
    };
    cache.put(key, result.clone());
    Ok(result)
}

/// Lichess writes castling as king-takes-rook (e1h1); the app uses e1g1.
fn normalise_castling(uci: &str, san: &str) -> String {
    match (uci, san.trim_end_matches(['+', '#'])) {
        ("e1h1", "O-O") => "e1g1".into(),
        ("e1a1", "O-O-O") => "e1c1".into(),
        ("e8h8", "O-O") => "e8g8".into(),
        ("e8a8", "O-O-O") => "e8c8".into(),
        _ => uci.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn castling_is_normalised() {
        assert_eq!(normalise_castling("e1h1", "O-O"), "e1g1");
        assert_eq!(normalise_castling("e8a8", "O-O-O+"), "e8c8");
        assert_eq!(normalise_castling("e1h1", "Rxh1"), "e1h1");
    }

    #[test]
    fn parses_api_response() {
        let json = r#"{"white":10,"draws":5,"black":3,"moves":[{"uci":"e2e4","san":"e4","averageRating":2412,"white":6,"draws":2,"black":1,"game":null}],"opening":null,"topGames":[]}"#;
        let r: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.moves[0].average_rating, Some(2412.0));
    }
}
