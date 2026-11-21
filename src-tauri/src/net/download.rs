//! Streaming file downloads with progress reporting.

use std::path::Path;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::error::{Error, Result};

pub const USER_AGENT: &str = concat!("GrandieChess/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Inad-Tech-World/grandie_chess)");

/// Longest silence allowed while reading a response, so a stalled server
/// fails instead of hanging forever.
pub const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(READ_TIMEOUT)
        .build()
        .expect("HTTP client builds")
}

/// What a download must match to be accepted.
#[derive(Debug, Clone, Copy, Default)]
pub struct Expect<'a> {
    pub size: Option<u64>,
    /// Lower-case hex SHA-256 of the whole file.
    pub sha256: Option<&'a str>,
}

impl Expect<'_> {
    pub fn size(size: u64) -> Self {
        Expect { size: Some(size), sha256: None }
    }
}

/// Download `url` to `dest`, writing to `dest.part` first so a cancelled or
/// failed download never leaves a truncated file behind. `progress` gets
/// `(received, total)` in bytes about ten times a second.
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expect: Expect<'_>,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64> {
    use sha2::Digest;
    let response = client.get(url).send().await?.error_for_status()?;
    let total = response.content_length().or(expect.size);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = response.bytes_stream();
    let mut received: u64 = 0;
    let mut hasher = sha2::Sha256::new();
    let mut last = Instant::now();
    progress(0, total);
    let result: Result<()> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if expect.sha256.is_some() {
                hasher.update(&chunk);
            }
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            if last.elapsed() >= Duration::from_millis(100) {
                progress(received, total);
                last = Instant::now();
            }
        }
        file.flush().await?;
        Ok(())
    }
    .await;
    drop(file);
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    if let Some(expected) = expect.size {
        if received != expected {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::other(format!("download incomplete: got {received} bytes, expected {expected}")));
        }
    }
    if let Some(expected) = expect.sha256 {
        let got = hex::encode(hasher.finalize());
        if !got.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::other("the download is damaged or was changed (checksum mismatch); please try again"));
        }
    }
    tokio::fs::rename(&part, dest).await?;
    progress(received, Some(received));
    Ok(received)
}

/// lc0 network files are gzip-compressed protobufs.
pub fn looks_like_network(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 2];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).map(|_| magic == [0x1f, 0x8b]).unwrap_or(false)
}
