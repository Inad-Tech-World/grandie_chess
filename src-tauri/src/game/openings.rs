//! Opening names (ECO code + name) from the Lichess chess-openings dataset
//! (CC0, bundled in `resources/openings`). Positions are matched by Zobrist
//! hash, so transpositions are recognised.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position};
use specta::Type;

use super::position::{hash_of, parse_fen, parse_san, parse_uci};

const TSV: [&str; 5] = [
    include_str!("../../resources/openings/a.tsv"),
    include_str!("../../resources/openings/b.tsv"),
    include_str!("../../resources/openings/c.tsv"),
    include_str!("../../resources/openings/d.tsv"),
    include_str!("../../resources/openings/e.tsv"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpeningName {
    pub eco: String,
    pub name: String,
}

pub struct OpeningBook {
    by_hash: HashMap<u64, OpeningName>,
}

impl OpeningBook {
    /// The shared book, built on first use (a few milliseconds).
    pub fn get() -> &'static OpeningBook {
        static BOOK: OnceLock<OpeningBook> = OnceLock::new();
        BOOK.get_or_init(OpeningBook::build)
    }

    fn build() -> OpeningBook {
        let mut by_hash = HashMap::new();
        for file in TSV {
            for line in file.lines().skip(1) {
                let mut cols = line.split('\t');
                let (Some(eco), Some(name), Some(pgn)) = (cols.next(), cols.next(), cols.next()) else { continue };
                let mut pos = Chess::default();
                let mut ok = true;
                for token in pgn.split_whitespace().filter(|t| !t.ends_with('.')) {
                    match parse_san(&pos, token) {
                        Ok(m) => pos.play_unchecked(m),
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    // Later (more specific) entries for the same position win.
                    by_hash.insert(hash_of(&pos), OpeningName { eco: eco.to_string(), name: name.to_string() });
                }
            }
        }
        OpeningBook { by_hash }
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    pub fn lookup(&self, pos: &Chess) -> Option<&OpeningName> {
        self.by_hash.get(&hash_of(pos))
    }

    /// The deepest named position reached along a line of UCI moves.
    pub fn classify(&self, start_fen: Option<&str>, moves: &[String]) -> Option<OpeningName> {
        let mut pos = match start_fen {
            Some(f) => parse_fen(f).ok()?,
            None => Chess::default(),
        };
        let mut best = self.lookup(&pos).cloned();
        // Names stop being useful once the opening is over.
        for uci in moves.iter().take(40) {
            let m = parse_uci(&pos, uci).ok()?;
            pos.play_unchecked(m);
            if let Some(o) = self.lookup(&pos) {
                best = Some(o.clone());
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn names_common_openings_and_transpositions() {
        let book = OpeningBook::get();
        assert!(book.len() > 3000, "{} openings", book.len());
        let italian = book.classify(None, &line("e2e4 e7e5 g1f3 b8c6 f1c4")).unwrap();
        assert_eq!(italian.eco, "C50");
        assert!(italian.name.starts_with("Italian Game"), "{}", italian.name);
        let sicilian = book.classify(None, &line("e2e4 c7c5")).unwrap();
        assert_eq!(sicilian.eco, "B20");
        // QGD Normal Defense (1. d4 d5 2. c4 e6 3. Nc3 Nf6) reached via the English: 1. c4 e6 2. Nc3 d5 3. d4 Nf6.
        let qgd = book.classify(None, &line("c2c4 e7e6 b1c3 d7d5 d2d4 g8f6")).unwrap();
        assert_eq!(qgd.eco, "D35", "{}", qgd.name);
        // Deep lines keep the last named position.
        let deep = book.classify(None, &line("e2e4 c7c5 a2a3 a7a6 h2h3 h7h6")).unwrap();
        assert!(deep.name.starts_with("Sicilian Defense"), "{}", deep.name);
        assert_eq!(book.classify(None, &[]), None);
    }
}
