//! Parsing of UCI engine output into typed messages.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Score {
    /// Centipawns from the side to move's point of view. lc0's default
    /// `ScoreType=WDL_mu` makes this a display value, not a classical eval.
    Cp(i32),
    /// Moves to mate; negative when the side to move is getting mated.
    Mate(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ScoreBound {
    Exact,
    Lower,
    Upper,
}

/// Win/draw/loss in per mille (W + D + L = 1000) from the side to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Wdl {
    pub w: u32,
    pub d: u32,
    pub l: u32,
}

impl Wdl {
    /// Expected score for the side to move, 0.0..=1.0.
    pub fn expected_score(&self) -> f64 {
        let total = (self.w + self.d + self.l).max(1) as f64;
        (self.w as f64 + self.d as f64 / 2.0) / total
    }

    pub fn flipped(&self) -> Wdl {
        Wdl { w: self.l, d: self.d, l: self.w }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InfoLine {
    pub depth: Option<u32>,
    pub seldepth: Option<u32>,
    pub time_ms: Option<f64>,
    pub nodes: Option<f64>,
    pub nps: Option<f64>,
    pub score: Option<Score>,
    pub bound: Option<ScoreBound>,
    pub wdl: Option<Wdl>,
    pub moves_left: Option<u32>,
    pub multipv: Option<u32>,
    pub hashfull: Option<u32>,
    pub tbhits: Option<f64>,
    pub pv: Vec<String>,
    pub string: Option<String>,
}

impl InfoLine {
    /// True for lines that carry a search result (score and principal variation).
    pub fn is_search_line(&self) -> bool {
        self.score.is_some() && !self.pv.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OptionKind {
    Check { default: bool },
    /// Numbers as f64: TypeScript has no 64-bit integers.
    Spin { default: f64, min: f64, max: f64 },
    Combo { default: String, vars: Vec<String> },
    Button,
    String { default: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UciOption {
    pub name: String,
    pub kind: OptionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UciMessage {
    IdName(String),
    IdAuthor(String),
    UciOk,
    ReadyOk,
    Option(UciOption),
    Info(InfoLine),
    BestMove { best: String, ponder: Option<String> },
    Other(String),
    /// Not sent by engines: the process closed its output.
    Exited,
}

fn num<T: std::str::FromStr>(tok: Option<&&str>) -> Option<T> {
    tok.and_then(|t| t.parse().ok())
}

pub fn parse_line(line: &str) -> UciMessage {
    let line = line.trim();
    let tokens: Vec<&str> = line.split_whitespace().collect();
    match tokens.first().copied() {
        Some("uciok") => UciMessage::UciOk,
        Some("readyok") => UciMessage::ReadyOk,
        Some("id") => match tokens.get(1).copied() {
            Some("name") => UciMessage::IdName(tokens[2..].join(" ")),
            Some("author") => UciMessage::IdAuthor(tokens[2..].join(" ")),
            _ => UciMessage::Other(line.to_string()),
        },
        Some("bestmove") => match tokens.get(1) {
            Some(best) => UciMessage::BestMove {
                best: best.to_string(),
                ponder: match tokens.get(2).copied() {
                    Some("ponder") => tokens.get(3).map(|s| s.to_string()),
                    _ => None,
                },
            },
            None => UciMessage::Other(line.to_string()),
        },
        Some("info") => UciMessage::Info(parse_info(&tokens[1..])),
        Some("option") => parse_option(&tokens[1..])
            .map(UciMessage::Option)
            .unwrap_or_else(|| UciMessage::Other(line.to_string())),
        _ => UciMessage::Other(line.to_string()),
    }
}

fn parse_info(tokens: &[&str]) -> InfoLine {
    let mut info = InfoLine::default();
    let mut i = 0;
    while i < tokens.len() {
        let key = tokens[i];
        let next = tokens.get(i + 1);
        match key {
            "depth" => info.depth = num(next),
            "seldepth" => info.seldepth = num(next),
            "time" => info.time_ms = num(next),
            "nodes" => info.nodes = num(next),
            "nps" => info.nps = num(next),
            "multipv" => info.multipv = num(next),
            "hashfull" => info.hashfull = num(next),
            "tbhits" => info.tbhits = num(next),
            "movesleft" => info.moves_left = num(next),
            "score" => {
                let kind = next.copied();
                let value: Option<i32> = num(tokens.get(i + 2));
                info.score = match (kind, value) {
                    (Some("cp"), Some(v)) => Some(Score::Cp(v)),
                    (Some("mate"), Some(v)) => Some(Score::Mate(v)),
                    _ => None,
                };
                i += 3;
                info.bound = Some(ScoreBound::Exact);
                match tokens.get(i).copied() {
                    Some("lowerbound") => {
                        info.bound = Some(ScoreBound::Lower);
                        i += 1;
                    }
                    Some("upperbound") => {
                        info.bound = Some(ScoreBound::Upper);
                        i += 1;
                    }
                    _ => {}
                }
                continue;
            }
            "wdl" => {
                let w = num(tokens.get(i + 1));
                let d = num(tokens.get(i + 2));
                let l = num(tokens.get(i + 3));
                if let (Some(w), Some(d), Some(l)) = (w, d, l) {
                    info.wdl = Some(Wdl { w, d, l });
                }
                i += 4;
                continue;
            }
            "pv" => {
                info.pv = tokens[i + 1..]
                    .iter()
                    .take_while(|t| !is_info_keyword(t))
                    .map(|s| s.to_string())
                    .collect();
                i += 1 + info.pv.len();
                continue;
            }
            "string" => {
                info.string = Some(tokens[i + 1..].join(" "));
                break;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    info
}

fn is_info_keyword(t: &str) -> bool {
    matches!(
        t,
        "depth" | "seldepth" | "time" | "nodes" | "nps" | "multipv" | "hashfull" | "tbhits"
            | "score" | "wdl" | "string" | "currmove" | "currmovenumber" | "movesleft" | "cpuload"
    )
}

fn parse_option(tokens: &[&str]) -> Option<UciOption> {
    let keywords = ["name", "type", "default", "min", "max", "var"];
    let mut name = String::new();
    let mut kind = String::new();
    let mut default: Option<String> = None;
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    let mut vars = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let key = tokens[i];
        let mut j = i + 1;
        while j < tokens.len() && !keywords.contains(&tokens[j]) {
            j += 1;
        }
        let value = tokens[i + 1..j].join(" ");
        match key {
            "name" => name = value,
            "type" => kind = value,
            "default" => default = Some(value),
            "min" => min = value.parse().ok(),
            "max" => max = value.parse().ok(),
            "var" => vars.push(value),
            _ => {}
        }
        i = j;
    }
    if name.is_empty() {
        return None;
    }
    let default_str = default.clone().unwrap_or_default();
    let kind = match kind.as_str() {
        "check" => OptionKind::Check { default: default_str == "true" },
        "spin" => OptionKind::Spin {
            default: default_str.parse().unwrap_or(0.0),
            min: min.unwrap_or(f64::MIN),
            max: max.unwrap_or(f64::MAX),
        },
        "combo" => OptionKind::Combo { default: default_str, vars },
        "button" => OptionKind::Button,
        _ => OptionKind::String { default: default_str },
    };
    Some(UciOption { name, kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Lines recorded from lc0 v0.32 with UCI_ShowWDL=true and from Stockfish 17.
    const LC0_INFO: &str = "info depth 5 seldepth 12 time 1203 nodes 1873 score cp 24 wdl 112 832 56 hashfull 3 nps 1556 tbhits 0 pv e2e4 e7e5 g1f3 b8c6 f1b5";
    const LC0_MULTIPV: &str = "info depth 3 seldepth 6 time 380 nodes 214 score cp -15 wdl 90 801 109 hashfull 0 nps 563 tbhits 0 multipv 2 pv d7d5 c2c4";
    const SF_MATE: &str = "info depth 24 seldepth 6 multipv 1 score mate -3 nodes 152301 nps 1203000 hashfull 51 tbhits 0 time 126 pv h7h8 g2g8 h8g8";
    const SF_BOUND: &str = "info depth 18 seldepth 25 multipv 1 score cp 31 lowerbound nodes 300000 nps 1000000 time 300 pv e2e4";

    #[test]
    fn parses_lc0_info_with_wdl() {
        let UciMessage::Info(info) = parse_line(LC0_INFO) else { panic!() };
        assert_eq!(info.depth, Some(5));
        assert_eq!(info.seldepth, Some(12));
        assert_eq!(info.nodes, Some(1873.0));
        assert_eq!(info.nps, Some(1556.0));
        assert_eq!(info.score, Some(Score::Cp(24)));
        assert_eq!(info.wdl, Some(Wdl { w: 112, d: 832, l: 56 }));
        assert_eq!(info.pv, vec!["e2e4", "e7e5", "g1f3", "b8c6", "f1b5"]);
        assert!(info.is_search_line());
        let es = info.wdl.unwrap().expected_score();
        assert!((es - 0.528).abs() < 1e-9);
    }

    #[test]
    fn parses_multipv_and_negative_scores() {
        let UciMessage::Info(info) = parse_line(LC0_MULTIPV) else { panic!() };
        assert_eq!(info.multipv, Some(2));
        assert_eq!(info.score, Some(Score::Cp(-15)));
        assert_eq!(info.pv, vec!["d7d5", "c2c4"]);
    }

    #[test]
    fn parses_mate_scores_and_bounds() {
        let UciMessage::Info(info) = parse_line(SF_MATE) else { panic!() };
        assert_eq!(info.score, Some(Score::Mate(-3)));
        assert_eq!(info.multipv, Some(1));
        assert_eq!(info.pv.len(), 3);
        let UciMessage::Info(info) = parse_line(SF_BOUND) else { panic!() };
        assert_eq!(info.bound, Some(ScoreBound::Lower));
        assert_eq!(info.nodes, Some(300000.0));
    }

    #[test]
    fn parses_info_string_and_bestmove() {
        let UciMessage::Info(info) = parse_line("info string e2e4  (322 ) N:     104 (+ 0) (P: 12.34%)") else {
            panic!()
        };
        assert!(info.string.as_deref().unwrap().starts_with("e2e4"));
        assert!(!info.is_search_line());
        assert_eq!(
            parse_line("bestmove e2e4 ponder e7e5"),
            UciMessage::BestMove { best: "e2e4".into(), ponder: Some("e7e5".into()) }
        );
        assert_eq!(parse_line("bestmove g1f3"), UciMessage::BestMove { best: "g1f3".into(), ponder: None });
    }

    #[test]
    fn parses_handshake_and_options() {
        assert_eq!(parse_line("id name Lc0 v0.32.1"), UciMessage::IdName("Lc0 v0.32.1".into()));
        assert_eq!(parse_line("uciok"), UciMessage::UciOk);
        assert_eq!(parse_line("readyok"), UciMessage::ReadyOk);

        let UciMessage::Option(o) = parse_line("option name WeightsFile type string default <autodiscover>") else {
            panic!()
        };
        assert_eq!(o.name, "WeightsFile");
        assert_eq!(o.kind, OptionKind::String { default: "<autodiscover>".into() });

        let UciMessage::Option(o) = parse_line("option name Threads type spin default 0 min 0 max 128") else {
            panic!()
        };
        assert_eq!(o.kind, OptionKind::Spin { default: 0.0, min: 0.0, max: 128.0 });

        let UciMessage::Option(o) =
            parse_line("option name ScoreType type combo default WDL_mu var centipawn var win_percentage var Q var W-L var WDL_mu")
        else {
            panic!()
        };
        let OptionKind::Combo { default, vars } = o.kind else { panic!() };
        assert_eq!(default, "WDL_mu");
        assert_eq!(vars.len(), 5);

        let UciMessage::Option(o) = parse_line("option name UCI_ShowWDL type check default false") else { panic!() };
        assert_eq!(o.kind, OptionKind::Check { default: false });

        let UciMessage::Option(o) = parse_line("option name Clear Hash type button") else { panic!() };
        assert_eq!(o.name, "Clear Hash");
        assert_eq!(o.kind, OptionKind::Button);
    }

    #[test]
    fn unknown_lines_are_kept() {
        assert_eq!(parse_line("       _"), UciMessage::Other("_".into()));
    }
}
