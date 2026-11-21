//! Local AI bookkeeping in the game database: spend per request and a cache
//! of generated text keyed by everything that shaped it.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

use super::openrouter::Usage;
use crate::error::Result;

/// What a request was for, shown in the usage summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Narrate,
    Explain,
    Transcribe,
    Speech,
    PlayerStyle,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Purpose::Narrate => "narrate",
            Purpose::Explain => "explain",
            Purpose::Transcribe => "transcribe",
            Purpose::Speech => "speech",
            Purpose::PlayerStyle => "player_style",
        }
    }
}

pub fn record(conn: &Connection, purpose: Purpose, model: &str, usage: &Usage, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_usage (at, model, purpose, tokens_in, tokens_out, cost) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![now, model, purpose.as_str(), usage.prompt_tokens as i64, usage.completion_tokens as i64, usage.cost],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageSummary {
    pub requests: u32,
    pub tokens_in: f64,
    pub tokens_out: f64,
    /// Credits (US dollars) OpenRouter reported; requests without a reported
    /// cost aren't included.
    pub cost: f64,
}

pub fn summary(conn: &Connection, since: i64) -> Result<AiUsageSummary> {
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0), COALESCE(SUM(cost), 0) FROM ai_usage WHERE at >= ?1",
        [since],
        |r| {
            Ok(AiUsageSummary {
                requests: r.get::<_, i64>(0)? as u32,
                tokens_in: r.get::<_, i64>(1)? as f64,
                tokens_out: r.get::<_, i64>(2)? as f64,
                cost: r.get(3)?,
            })
        },
    )?)
}

/// A cache key from all the parts that shaped an answer.
pub fn cache_key(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0u8]);
    }
    hex::encode(h.finalize())
}

pub fn cache_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM coach_cache WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn cache_put(conn: &Connection, key: &str, value: &str, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO coach_cache (key, value, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, created_at = excluded.created_at",
        params![key, value, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_usage_and_caches_text() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.get().unwrap();
        record(&conn, Purpose::Narrate, "m", &Usage { prompt_tokens: 100, completion_tokens: 20, cost: Some(0.001) }, 1000).unwrap();
        record(&conn, Purpose::Explain, "m", &Usage { prompt_tokens: 50, completion_tokens: 10, cost: None }, 2000).unwrap();
        let all = summary(&conn, 0).unwrap();
        assert_eq!((all.requests, all.tokens_in, all.tokens_out), (2, 150.0, 30.0));
        assert!((all.cost - 0.001).abs() < 1e-12);
        assert_eq!(summary(&conn, 1500).unwrap().requests, 1);

        let key = cache_key(&["fen", "e2e4", "stockfish"]);
        assert_ne!(key, cache_key(&["fen", "e2e4", "leela"]));
        assert_eq!(cache_get(&conn, &key).unwrap(), None);
        cache_put(&conn, &key, "hello", 1).unwrap();
        cache_put(&conn, &key, "again", 2).unwrap();
        assert_eq!(cache_get(&conn, &key).unwrap().as_deref(), Some("again"));
    }
}
