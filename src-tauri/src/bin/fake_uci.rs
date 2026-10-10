//! A tiny scripted UCI engine for tests. It always plays the
//! alphabetically first legal move and reports a fixed evaluation, so engine
//! features can be tested without lc0 or a GPU.
//!
//! Environment variables:
//! - `FAKE_UCI_DELAY_MS`: think time per move (default 0)
//! - `FAKE_UCI_WDL`: "W D L" in per mille from the side to move (default "300 400 300")

use std::io::{self, BufRead, Write};
use std::time::Duration;

use grandie_chess_lib::game::position::{parse_fen, parse_uci, uci_of, START_FEN};
use shakmaty::{Chess, Position};

fn parse_position(tokens: &[&str]) -> Option<Chess> {
    let (fen, rest) = match tokens.get(1).copied() {
        Some("startpos") => (START_FEN.to_string(), &tokens[2..]),
        Some("fen") => {
            let end = tokens.iter().position(|t| *t == "moves").unwrap_or(tokens.len());
            (tokens[2..end].join(" "), &tokens[end..])
        }
        _ => return None,
    };
    let mut pos = parse_fen(&fen).ok()?;
    if rest.first() == Some(&"moves") {
        for uci in &rest[1..] {
            let m = parse_uci(&pos, uci).ok()?;
            pos.play_unchecked(m);
        }
    }
    Some(pos)
}

fn choose(pos: &Chess) -> Option<String> {
    let mut moves: Vec<String> = pos.legal_moves().into_iter().map(uci_of).collect();
    moves.sort();
    moves.into_iter().next()
}

fn main() -> io::Result<()> {
    let delay = std::env::var("FAKE_UCI_DELAY_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(0u64);
    let wdl = std::env::var("FAKE_UCI_WDL").unwrap_or_else(|_| "300 400 300".into());
    let stdin = io::stdin();
    let mut out = io::stdout();
    let mut pos = Chess::default();
    let mut pending: Option<String> = None;
    for line in stdin.lock().lines() {
        let line = line?;
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.first().copied() {
            Some("uci") => {
                writeln!(out, "id name FakeUCI 1.0")?;
                writeln!(out, "id author Grandie Chess tests")?;
                writeln!(out, "option name WeightsFile type string default <autodiscover>")?;
                writeln!(out, "option name Threads type spin default 1 min 1 max 8")?;
                writeln!(out, "option name MultiPV type spin default 1 min 1 max 5")?;
                writeln!(out, "option name UCI_ShowWDL type check default false")?;
                writeln!(out, "uciok")?;
            }
            Some("isready") => writeln!(out, "readyok")?,
            Some("position") => {
                if let Some(p) = parse_position(&tokens) {
                    pos = p;
                } else {
                    writeln!(out, "info string bad position command")?;
                }
            }
            Some("go") => {
                let best = choose(&pos).unwrap_or_else(|| "0000".into());
                std::thread::sleep(Duration::from_millis(delay));
                writeln!(
                    out,
                    "info depth 1 seldepth 1 time {delay} nodes 1 score cp 0 wdl {wdl} nps 1 tbhits 0 pv {best}"
                )?;
                if tokens.contains(&"infinite") {
                    pending = Some(best);
                } else {
                    writeln!(out, "bestmove {best}")?;
                }
            }
            Some("stop") => {
                if let Some(best) = pending.take() {
                    writeln!(out, "bestmove {best}")?;
                }
            }
            Some("quit") => break,
            _ => {}
        }
        out.flush()?;
    }
    Ok(())
}
