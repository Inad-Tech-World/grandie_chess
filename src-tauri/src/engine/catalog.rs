//! Known networks and opponent levels, loaded from the bundled JSON files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{Error, Result};

const NETWORKS_JSON: &str = include_str!("../../resources/networks.json");
const LEVELS_JSON: &str = include_str!("../../resources/levels.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NetworkKind {
    Leela,
    Maia,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDef {
    pub id: String,
    pub name: String,
    pub kind: NetworkKind,
    pub url: String,
    /// Size in bytes (an f64 because TypeScript has no 64-bit integers).
    pub size: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntry {
    #[serde(flatten)]
    pub def: NetworkDef,
    pub installed: bool,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OpponentKind {
    Maia,
    Leela,
    Full,
}

/// One selectable opponent strength.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpponentLevel {
    pub id: String,
    pub name: String,
    pub kind: OpponentKind,
    /// Network id; for `Full` this is resolved from settings at game start.
    pub network: Option<String>,
    /// Nodes per move (`go nodes N`); `None` means clock-based search.
    pub nodes: Option<u32>,
    pub temperature: f64,
    pub temp_decay_moves: u32,
    /// Approximate rating; `None` for full strength.
    pub elo: Option<u32>,
}

#[derive(Deserialize)]
struct NetworksFile {
    networks: Vec<NetworkDef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LevelDef {
    id: String,
    name: String,
    network: String,
    #[serde(default)]
    nodes: Option<u32>,
    #[serde(default)]
    temperature: f64,
    #[serde(default)]
    temp_decay_moves: u32,
    elo: u32,
}

#[derive(Deserialize)]
struct LevelsFile {
    maia: Vec<LevelDef>,
    leela: Vec<LevelDef>,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub networks: Vec<NetworkDef>,
    pub levels: Vec<OpponentLevel>,
}

impl Catalog {
    pub fn load() -> Catalog {
        let networks: NetworksFile = serde_json::from_str(NETWORKS_JSON).expect("bundled networks.json is valid");
        let levels: LevelsFile = serde_json::from_str(LEVELS_JSON).expect("bundled levels.json is valid");
        let mut out = Vec::new();
        for l in levels.maia {
            out.push(OpponentLevel {
                id: l.id,
                name: l.name,
                kind: OpponentKind::Maia,
                network: Some(l.network),
                nodes: Some(1),
                temperature: 1.0,
                temp_decay_moves: 10,
                elo: Some(l.elo),
            });
        }
        for l in levels.leela {
            out.push(OpponentLevel {
                id: l.id,
                name: l.name,
                kind: OpponentKind::Leela,
                network: Some(l.network),
                nodes: l.nodes,
                temperature: l.temperature,
                temp_decay_moves: l.temp_decay_moves,
                elo: Some(l.elo),
            });
        }
        out.push(OpponentLevel {
            id: "full".into(),
            name: "Leela full strength".into(),
            kind: OpponentKind::Full,
            network: None,
            nodes: None,
            temperature: 0.0,
            temp_decay_moves: 0,
            elo: None,
        });
        Catalog { networks: networks.networks, levels: out }
    }

    pub fn network(&self, id: &str) -> Result<&NetworkDef> {
        self.networks
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| Error::NotFound(format!("unknown network: {id}")))
    }

    pub fn level(&self, id: &str) -> Result<&OpponentLevel> {
        self.levels
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| Error::NotFound(format!("unknown opponent level: {id}")))
    }

    pub fn entries(&self, networks_dir: &Path) -> Vec<NetworkEntry> {
        self.networks
            .iter()
            .map(|def| {
                let path = network_path(networks_dir, &def.id);
                NetworkEntry {
                    installed: path.exists(),
                    path: path.to_string_lossy().into_owned(),
                    def: def.clone(),
                }
            })
            .collect()
    }
}

pub fn network_path(networks_dir: &Path, id: &str) -> PathBuf {
    networks_dir.join(format!("{id}.pb.gz"))
}

/// The installed network to use for full-strength work: the user's choice if
/// installed, otherwise the first installed Leela net in catalog order.
pub fn resolve_full_network(catalog: &Catalog, networks_dir: &Path, preferred: Option<&str>) -> Option<String> {
    if let Some(id) = preferred {
        if network_path(networks_dir, id).exists() {
            return Some(id.to_string());
        }
    }
    catalog
        .networks
        .iter()
        .filter(|n| n.kind == NetworkKind::Leela && n.id != "703810")
        .chain(catalog.networks.iter().filter(|n| n.id == "703810"))
        .find(|n| network_path(networks_dir, &n.id).exists())
        .map(|n| n.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_consistent() {
        let c = Catalog::load();
        assert!(c.networks.len() >= 10);
        for level in &c.levels {
            if let Some(net) = &level.network {
                assert!(c.network(net).is_ok(), "level {} uses unknown network {net}", level.id);
            }
        }
        assert!(c.level("maia-1500").is_ok());
        assert_eq!(c.level("full").unwrap().kind, OpponentKind::Full);
        for n in &c.networks {
            assert!(n.url.starts_with("https://"), "{} must download over HTTPS", n.id);
        }
    }

    #[test]
    fn resolves_full_network_preference() {
        let dir = tempfile::tempdir().unwrap();
        let c = Catalog::load();
        assert_eq!(resolve_full_network(&c, dir.path(), Some("t1-256x10")), None);
        std::fs::write(network_path(dir.path(), "703810"), b"x").unwrap();
        assert_eq!(resolve_full_network(&c, dir.path(), None).as_deref(), Some("703810"));
        std::fs::write(network_path(dir.path(), "791556"), b"x").unwrap();
        assert_eq!(resolve_full_network(&c, dir.path(), None).as_deref(), Some("791556"));
        assert_eq!(resolve_full_network(&c, dir.path(), Some("703810")).as_deref(), Some("703810"));
    }
}
