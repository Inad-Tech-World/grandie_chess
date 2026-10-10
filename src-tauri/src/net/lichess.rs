//! Lichess public API: a user's games as PGN, streamed.
//! https://lichess.org/api#tag/Games/operation/apiGamesUser

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{header::ACCEPT, StatusCode};

use crate::error::{Error, Result};

pub fn valid_username(u: &str) -> bool {
    (2..=30).contains(&u.len()) && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Download a user's games played after `since_ms` (all if `None`), oldest
/// first. `progress(games, bytes)` reports what has arrived so far.
pub async fn fetch_user_games(
    client: &reqwest::Client,
    username: &str,
    since_ms: Option<i64>,
    max: Option<u32>,
    mut progress: impl FnMut(u32, u64),
) -> Result<Vec<u8>> {
    if !valid_username(username) {
        return Err(Error::other(format!("\"{username}\" is not a valid Lichess username")));
    }
    let mut url = format!("https://lichess.org/api/games/user/{username}?clocks=true&opening=true&evals=false&sort=dateAsc");
    if let Some(since) = since_ms {
        url.push_str(&format!("&since={since}"));
    }
    if let Some(max) = max {
        url.push_str(&format!("&max={max}"));
    }
    let mut attempt = 0;
    let response = loop {
        attempt += 1;
        let r = client.get(&url).header(ACCEPT, "application/x-chess-pgn").send().await?;
        match r.status() {
            StatusCode::TOO_MANY_REQUESTS if attempt < 3 => {
                // Lichess asks clients to wait a full minute after a 429.
                log::warn!("lichess rate limit; waiting 60 s");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            StatusCode::NOT_FOUND => return Err(Error::NotFound(format!("Lichess has no user \"{username}\""))),
            _ => break r.error_for_status()?,
        }
    };
    let mut body = Vec::new();
    let mut games = 0u32;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        games += count_games(&chunk);
        body.extend_from_slice(&chunk);
        progress(games, body.len() as u64);
    }
    Ok(body)
}

/// Rough count of games in a chunk of PGN (each starts with an Event tag).
pub fn count_games(chunk: &[u8]) -> u32 {
    chunk.windows(8).filter(|w| *w == b"[Event \"").count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_and_counting() {
        assert!(valid_username("DrNykterstein"));
        assert!(valid_username("a_b-c"));
        assert!(!valid_username("x"));
        assert!(!valid_username("bad name"));
        assert!(!valid_username("../etc"));
        assert_eq!(count_games(b"[Event \"a\"]\n\n1. e4 *\n\n[Event \"b\"]"), 2);
    }
}
