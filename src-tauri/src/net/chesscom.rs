//! Chess.com published-data API: monthly game archives as PGN.
//! https://www.chess.com/news/view/published-data-api

use reqwest::StatusCode;
use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Deserialize)]
struct Archives {
    archives: Vec<String>,
}

pub fn valid_username(u: &str) -> bool {
    (3..=25).contains(&u.len()) && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// URLs of the user's monthly archives, oldest first.
pub async fn archives(client: &reqwest::Client, username: &str) -> Result<Vec<String>> {
    if !valid_username(username) {
        return Err(Error::other(format!("\"{username}\" is not a valid Chess.com username")));
    }
    let url = format!("https://api.chess.com/pub/player/{}/games/archives", username.to_lowercase());
    let r = client.get(&url).send().await?;
    if r.status() == StatusCode::NOT_FOUND {
        return Err(Error::NotFound(format!("Chess.com has no user \"{username}\"")));
    }
    Ok(r.error_for_status()?.json::<Archives>().await?.archives)
}

/// The (year, month) an archive URL covers (".../games/2026/09").
pub fn archive_month(url: &str) -> Option<(i32, u32)> {
    let mut parts = url.trim_end_matches('/').rsplit('/');
    let month = parts.next()?.parse().ok()?;
    let year = parts.next()?.parse().ok()?;
    Some((year, month))
}

/// Archives for months on or after the month of `since` (unix seconds).
pub fn archives_since(all: Vec<String>, since: Option<i64>) -> Vec<String> {
    let Some(since) = since.and_then(|s| chrono::DateTime::from_timestamp(s, 0)) else { return all };
    use chrono::Datelike;
    let from = (since.year(), since.month());
    all.into_iter().filter(|u| archive_month(u).is_some_and(|m| m >= from)).collect()
}

pub async fn month_pgn(client: &reqwest::Client, archive_url: &str) -> Result<String> {
    let r = client.get(format!("{archive_url}/pgn")).send().await?.error_for_status()?;
    Ok(r.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_and_filtering() {
        let urls: Vec<String> = ["2026/07", "2026/08", "2026/09"]
            .iter()
            .map(|m| format!("https://api.chess.com/pub/player/x/games/{m}"))
            .collect();
        assert_eq!(archive_month(&urls[1]), Some((2026, 8)));
        // 2026-08-15
        let since = chrono::NaiveDate::from_ymd_opt(2026, 8, 15).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        assert_eq!(archives_since(urls.clone(), Some(since)).len(), 2);
        assert_eq!(archives_since(urls, None).len(), 3);
        assert!(valid_username("hikaru"));
        assert!(!valid_username("a b"));
    }
}
