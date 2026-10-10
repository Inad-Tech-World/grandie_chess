//! lc0-specific setup: which options to send for each kind of opponent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::catalog::{OpponentKind, OpponentLevel};
use super::process::Engine;
use crate::error::{Error, Result};
use crate::settings::Settings;

/// Loading a network onto a slow GPU can take a while the first time.
pub const READY_TIMEOUT: Duration = Duration::from_secs(180);

/// Options sent to lc0 before a game or analysis.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lc0Config {
    pub weights: PathBuf,
    pub backend: Option<String>,
    pub backend_opts: Option<String>,
    pub threads: Option<u32>,
    pub minibatch_size: Option<u32>,
    pub max_prefetch: Option<u32>,
    pub smart_pruning_factor: Option<f64>,
    pub temperature: Option<f64>,
    pub temp_decay_moves: Option<u32>,
    pub multipv: Option<u32>,
}

impl Lc0Config {
    /// Full strength with the user's backend choice (analysis, hints, full opponent).
    pub fn full(weights: PathBuf, settings: &Settings) -> Self {
        Lc0Config {
            weights,
            backend: settings.backend.clone(),
            backend_opts: settings.backend_opts.clone(),
            threads: settings.threads,
            ..Default::default()
        }
    }

    /// Settings for a training opponent.
    pub fn for_level(level: &OpponentLevel, weights: PathBuf, settings: &Settings) -> Self {
        match level.kind {
            OpponentKind::Maia => Lc0Config {
                weights,
                // One node per move: the CPU is instant and avoids GPU start-up cost.
                backend: Some("blas".into()),
                threads: Some(1),
                minibatch_size: Some(1),
                max_prefetch: Some(0),
                temperature: settings.opponent_variety.then_some(level.temperature),
                temp_decay_moves: settings.opponent_variety.then_some(level.temp_decay_moves),
                ..Default::default()
            },
            OpponentKind::Leela => {
                let small = level.nodes.unwrap_or(1) <= 64;
                Lc0Config {
                    weights,
                    backend: if small { Some("blas".into()) } else { settings.backend.clone() },
                    backend_opts: if small { None } else { settings.backend_opts.clone() },
                    threads: if small { Some(1) } else { settings.threads },
                    minibatch_size: small.then_some(1),
                    max_prefetch: small.then_some(0),
                    // Exact node counts make each level play at a consistent strength.
                    smart_pruning_factor: Some(0.0),
                    temperature: Some(level.temperature),
                    temp_decay_moves: Some(level.temp_decay_moves),
                    ..Default::default()
                }
            }
            OpponentKind::Full => Lc0Config::full(weights, settings),
        }
    }

    /// The `setoption` commands, in order. Hidden lc0 options (Temperature and
    /// friends) are accepted even though `uci` doesn't list them.
    pub fn commands(&self) -> Vec<(String, String)> {
        let mut out = vec![
            ("WeightsFile".to_string(), self.weights.to_string_lossy().into_owned()),
            ("UCI_ShowWDL".into(), "true".into()),
        ];
        if let Some(b) = &self.backend {
            out.push(("Backend".into(), b.clone()));
        }
        if let Some(o) = &self.backend_opts {
            out.push(("BackendOptions".into(), o.clone()));
        }
        if let Some(t) = self.threads {
            out.push(("Threads".into(), t.to_string()));
        }
        if let Some(m) = self.minibatch_size {
            out.push(("MinibatchSize".into(), m.to_string()));
        }
        if let Some(p) = self.max_prefetch {
            out.push(("MaxPrefetch".into(), p.to_string()));
        }
        if let Some(s) = self.smart_pruning_factor {
            out.push(("SmartPruningFactor".into(), s.to_string()));
        }
        if let Some(t) = self.temperature {
            out.push(("Temperature".into(), t.to_string()));
        }
        if let Some(d) = self.temp_decay_moves {
            out.push(("TempDecayMoves".into(), d.to_string()));
        }
        if let Some(m) = self.multipv {
            out.push(("MultiPV".into(), m.to_string()));
        }
        out
    }
}

/// Start lc0 (or any UCI engine standing in for it), apply the config and wait
/// until the network is loaded.
pub async fn start(path: &Path, config: &Lc0Config) -> Result<Arc<Engine>> {
    if !config.weights.exists() {
        return Err(Error::NotFound(format!("network file missing: {}", config.weights.display())));
    }
    let engine = Engine::spawn(path, &[]).await?;
    for (name, value) in config.commands() {
        engine.set_option(&name, value).await?;
    }
    engine.ready(READY_TIMEOUT).await?;
    Ok(engine)
}

/// Backend lc0 reports on stderr, e.g. "metal (AMD Radeon Pro 560X)".
pub fn backend_in_use(engine: &Engine) -> Option<String> {
    engine.stderr_lines().iter().rev().find_map(|line| parse_backend_line(line))
}

fn parse_backend_line(line: &str) -> Option<String> {
    let rest = line.split("Initialized ").nth(1)?;
    let (backend, device) = rest.split_once(" backend")?;
    let device = device.split("on device ").nth(1).map(str::trim);
    Some(match device {
        Some(d) if !d.is_empty() => format!("{backend} ({d})"),
        _ => backend.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::catalog::Catalog;

    #[test]
    fn maia_runs_one_node_on_the_cpu() {
        let c = Catalog::load();
        let level = c.level("maia-1500").unwrap();
        let cfg = Lc0Config::for_level(level, "/n/maia-1500.pb.gz".into(), &Settings::default());
        let cmds = cfg.commands();
        assert!(cmds.contains(&("Backend".into(), "blas".into())));
        assert!(cmds.contains(&("MinibatchSize".into(), "1".into())));
        assert!(cmds.contains(&("UCI_ShowWDL".into(), "true".into())));
        assert!(cmds.iter().any(|(n, _)| n == "Temperature"));

        let mut no_variety = Settings::default();
        no_variety.opponent_variety = false;
        let cfg = Lc0Config::for_level(level, "/n/m.pb.gz".into(), &no_variety);
        assert!(!cfg.commands().iter().any(|(n, _)| n == "Temperature"));
    }

    #[test]
    fn strong_leela_levels_use_the_users_backend() {
        let c = Catalog::load();
        let mut s = Settings::default();
        s.backend = Some("metal".into());
        s.backend_opts = Some("gpu=1".into());
        let cfg = Lc0Config::for_level(c.level("leela-8").unwrap(), "/n/x".into(), &s);
        assert_eq!(cfg.backend.as_deref(), Some("metal"));
        assert_eq!(cfg.smart_pruning_factor, Some(0.0));
        let weak = Lc0Config::for_level(c.level("leela-1").unwrap(), "/n/x".into(), &s);
        assert_eq!(weak.backend.as_deref(), Some("blas"));
    }

    #[test]
    fn parses_backend_banner() {
        assert_eq!(
            parse_backend_line("Initialized metal backend on device AMD Radeon Pro 560X").as_deref(),
            Some("metal (AMD Radeon Pro 560X)")
        );
        assert_eq!(parse_backend_line("Initialized blas backend").as_deref(), Some("blas"));
        assert_eq!(parse_backend_line("Loading weights file from: x"), None);
    }
}
