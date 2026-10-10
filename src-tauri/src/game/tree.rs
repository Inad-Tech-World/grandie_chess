//! Move tree with variations, stored as a flat arena so it maps directly onto
//! a TypeScript array. Node 0 is the root (the starting position).

use serde::{Deserialize, Serialize};
use shakmaty::Chess;
use specta::Type;

use super::position::{apply_uci, fen_of, parse_fen, START_FEN};
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PgnHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: u32,
    pub parent: Option<u32>,
    /// Child ids; the first one is the main continuation.
    pub children: Vec<u32>,
    /// Half-moves from the root.
    pub ply: u32,
    /// Position after this node's move (the start position for the root).
    pub fen: String,
    pub uci: Option<String>,
    pub san: Option<String>,
    pub comment: Option<String>,
    pub nags: Vec<u8>,
    /// Clock after the move, from `[%clk]`.
    pub clock_ms: Option<f64>,
    /// Engine eval from `[%eval]`, kept as text ("0.35", "#-3").
    pub eval: Option<String>,
}

impl TreeNode {
    fn new(id: u32, parent: Option<u32>, ply: u32, fen: String) -> Self {
        TreeNode {
            id,
            parent,
            children: Vec::new(),
            ply,
            fen,
            uci: None,
            san: None,
            comment: None,
            nags: Vec::new(),
            clock_ms: None,
            eval: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameTree {
    pub headers: Vec<PgnHeader>,
    pub nodes: Vec<TreeNode>,
}

impl GameTree {
    pub fn new(start_fen: Option<&str>) -> Result<GameTree> {
        let fen = match start_fen {
            Some(f) => fen_of(&parse_fen(f)?),
            None => START_FEN.to_string(),
        };
        Ok(GameTree { headers: Vec::new(), nodes: vec![TreeNode::new(0, None, 0, fen)] })
    }

    pub fn root(&self) -> &TreeNode {
        &self.nodes[0]
    }

    pub fn node(&self, id: u32) -> Result<&TreeNode> {
        self.nodes.get(id as usize).ok_or_else(|| Error::Chess(format!("no move with id {id}")))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|h| h.name == name).map(|h| h.value.as_str())
    }

    pub fn set_header(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.headers.iter_mut().find(|h| h.name == name) {
            Some(h) => h.value = value,
            None => self.headers.push(PgnHeader { name: name.to_string(), value }),
        }
    }

    pub fn position(&self, id: u32) -> Result<Chess> {
        parse_fen(&self.node(id)?.fen)
    }

    /// Add a move after `parent`, or return the existing child with that move.
    pub fn add_move(&mut self, parent: u32, uci: &str) -> Result<u32> {
        let pos = self.position(parent)?;
        let (_, applied) = apply_uci(&pos, uci)?;
        Ok(self.add_applied(parent, &applied.uci, &applied.san, applied.position.fen))
    }

    pub(crate) fn add_applied(&mut self, parent: u32, uci: &str, san: &str, fen: String) -> u32 {
        if let Some(&existing) = self.nodes[parent as usize].children.iter().find(|&&c| self.nodes[c as usize].uci.as_deref() == Some(uci))
        {
            return existing;
        }
        let id = self.nodes.len() as u32;
        let ply = self.nodes[parent as usize].ply + 1;
        let mut node = TreeNode::new(id, Some(parent), ply, fen);
        node.uci = Some(uci.to_string());
        node.san = Some(san.to_string());
        self.nodes.push(node);
        self.nodes[parent as usize].children.push(id);
        id
    }

    /// Node ids along the main line, excluding the root.
    pub fn mainline(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = 0u32;
        while let Some(&next) = self.nodes[cur as usize].children.first() {
            out.push(next);
            cur = next;
        }
        out
    }

    /// Node ids from the root (exclusive) to `id` (inclusive).
    pub fn path_to(&self, id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == 0 {
                break;
            }
            out.push(c);
            cur = self.nodes[c as usize].parent;
        }
        out.reverse();
        out
    }

    pub fn uci_moves_to(&self, id: u32) -> Vec<String> {
        self.path_to(id).iter().filter_map(|&n| self.nodes[n as usize].uci.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_variations_without_duplicates() {
        let mut t = GameTree::new(None).unwrap();
        let e4 = t.add_move(0, "e2e4").unwrap();
        let e5 = t.add_move(e4, "e7e5").unwrap();
        let c5 = t.add_move(e4, "c7c5").unwrap();
        assert_eq!(t.add_move(e4, "e7e5").unwrap(), e5);
        assert_eq!(t.node(e4).unwrap().children, vec![e5, c5]);
        assert_eq!(t.mainline(), vec![e4, e5]);
        assert_eq!(t.path_to(c5), vec![e4, c5]);
        assert_eq!(t.uci_moves_to(c5), vec!["e2e4", "c7c5"]);
        assert_eq!(t.node(c5).unwrap().san.as_deref(), Some("c5"));
        assert_eq!(t.node(c5).unwrap().ply, 2);
        assert!(t.add_move(c5, "e1e3").is_err());
    }
}
