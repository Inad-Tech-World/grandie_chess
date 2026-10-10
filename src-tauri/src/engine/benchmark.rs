//! Picks the fastest lc0 backend on this Mac by timing raw network
//! evaluation (`lc0 backendbench`) on every Metal GPU and on the CPU.
//! lc0's default GPU is not always the fast one: on Intel MacBook Pros it is
//! usually the integrated GPU.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackendResult {
    pub backend: String,
    pub backend_opts: Option<String>,
    /// Device name reported by lc0, e.g. "AMD Radeon Pro 560X".
    pub device: Option<String>,
    /// Best throughput seen, in network evaluations per second.
    pub nps: f64,
    pub error: Option<String>,
}

pub fn candidates() -> Vec<(String, Option<String>)> {
    let mut list: Vec<(String, Option<String>)> =
        (0..3).map(|gpu| ("metal".to_string(), Some(format!("gpu={gpu}")))).collect();
    list.push(("blas".into(), None));
    list
}

/// Benchmark one backend. Returns a result with `error` set if lc0 fails
/// (for example a GPU index that doesn't exist).
pub async fn run_one(lc0: &Path, weights: &Path, backend: &str, opts: Option<&str>) -> BackendResult {
    let mut cmd = Command::new(lc0);
    cmd.arg("backendbench")
        .arg(format!("--weights={}", weights.display()))
        .arg(format!("--backend={backend}"))
        .args(["--start-batch-size=64", "--max-batch-size=128", "--batch-step=64", "--batches=4"])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(o) = opts {
        cmd.arg(format!("--backend-opts={o}"));
    }
    let mut result = BackendResult {
        backend: backend.to_string(),
        backend_opts: opts.map(str::to_string),
        device: None,
        nps: 0.0,
        error: None,
    };
    let output = match tokio::time::timeout(Duration::from_secs(180), cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            result.error = Some(e.to_string());
            return result;
        }
        Err(_) => {
            result.error = Some("timed out".into());
            return result;
        }
    };
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let parsed = parse_output(&text);
    result.device = parsed.0;
    result.nps = parsed.1;
    if result.nps <= 0.0 {
        let reason = text
            .lines()
            .find(|l| l.contains("xception") || l.to_lowercase().contains("error"))
            .unwrap_or("no throughput reported")
            .trim()
            .to_string();
        result.error = Some(reason);
    }
    result
}

fn parse_output(text: &str) -> (Option<String>, f64) {
    let mut device = None;
    let mut best: f64 = 0.0;
    for line in text.lines() {
        if let Some(d) = line.split("on device ").nth(1) {
            device = Some(d.trim().to_string());
        }
        if let Some(rest) = line.split("throughput ").nth(1) {
            if let Some(v) = rest.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()) {
                best = best.max(v);
            }
        }
    }
    (device, best)
}

/// The fastest working result, preferring a GPU when speeds are close.
pub fn pick_best(results: &[BackendResult]) -> Option<&BackendResult> {
    results
        .iter()
        .filter(|r| r.error.is_none() && r.nps > 0.0)
        .max_by(|a, b| a.nps.partial_cmp(&b.nps).unwrap_or(std::cmp::Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RADEON: &str = "Loading weights file from: x\nInitialized metal backend on device AMD Radeon Pro 560X\n\
Benchmark batch size 64 with inference average time 170.433ms - throughput 375.513 nps.\n\
Benchmark batch size 128 with inference average time 253.317ms - throughput 505.295 nps.\n";

    #[test]
    fn parses_device_and_best_throughput() {
        let (device, nps) = parse_output(RADEON);
        assert_eq!(device.as_deref(), Some("AMD Radeon Pro 560X"));
        assert!((nps - 505.295).abs() < 1e-9);
        assert_eq!(parse_output("Unhandled exception: bad gpu"), (None, 0.0));
    }

    #[test]
    fn picks_fastest_working_backend() {
        let r = |b: &str, nps: f64, err: bool| BackendResult {
            backend: b.into(),
            backend_opts: None,
            device: None,
            nps,
            error: err.then(|| "x".into()),
        };
        let results = vec![r("metal0", 13.0, false), r("metal1", 505.0, false), r("metal2", 0.0, true), r("blas", 106.0, false)];
        assert_eq!(pick_best(&results).unwrap().backend, "metal1");
        assert!(pick_best(&[r("x", 0.0, true)]).is_none());
    }
}
